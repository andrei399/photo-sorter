//! Turns encoded photo bytes into a display-sized, upright `ColorImage`.
//! JPEGs (what cameras shoot) are decoded by libjpeg-turbo directly at a reduced DCT scale.
//! Entropy decoding still dominates (cost ∝ file size), so cold photos get an instant glimpse
//! from the EXIF thumbnail every camera embeds, while the full decode runs.

use crate::error::{AppError, AppResult};
use egui::{Color32, ColorImage};
use fast_image_resize::{PixelType, ResizeAlg, ResizeOptions, Resizer, images::Image};
use std::io::Cursor;

const JPEG_MAGIC: [u8; 3] = [0xFF, 0xD8, 0xFF];

/// Output is at most ~1.15× `target_px` per side for JPEG (next DCT step up), exact fit for other formats,
/// and never larger than the source. `orientation_override` replaces the embedded EXIF (RAW previews lack it).
pub fn decode_full(
    bytes: &[u8],
    target_px: [u32; 2],
    max_texture_side: usize,
    orientation_override: Option<Orientation>,
) -> AppResult<ColorImage> {
    let orientation = orientation_override.unwrap_or_else(|| exif_orientation(read_exif(bytes).as_ref()));
    let (pixels, size) = if bytes.starts_with(&JPEG_MAGIC) {
        decode_jpeg_scaled(bytes, target_px, orientation, max_texture_side)?
    } else {
        decode_other_format_to_fit(bytes, target_px, orientation, max_texture_side)?
    };
    let (pixels, size) = apply_orientation(pixels, size, orientation);
    Ok(ColorImage::new(size, pixels))
}

/// The ~160×120 EXIF thumbnail (DCF requires it in camera JPEGs); decodes in well under a millisecond.
pub fn decode_glimpse(bytes: &[u8]) -> Option<ColorImage> {
    let exif = read_exif(bytes)?;
    let thumbnail_field = |tag| exif.get_field(tag, exif::In::THUMBNAIL)?.value.get_uint(0);
    let offset = thumbnail_field(exif::Tag::JPEGInterchangeFormat)? as usize;
    let length = thumbnail_field(exif::Tag::JPEGInterchangeFormatLength)? as usize;
    let thumbnail = exif.buf().get(offset..offset.checked_add(length)?)?;
    let (pixels, size) = decode_jpeg_scaled(thumbnail, [u32::MAX, u32::MAX], Orientation::Upright, usize::MAX).ok()?;
    let (pixels, size) = apply_orientation(pixels, size, exif_orientation(Some(&exif)));
    Some(ColorImage::new(size, pixels))
}

fn decode_jpeg_scaled(
    bytes: &[u8],
    target_px: [u32; 2],
    orientation: Orientation,
    max_texture_side: usize,
) -> AppResult<(Vec<Color32>, [usize; 2])> {
    let mut decompressor = turbojpeg::Decompressor::new().map_err(decode_error)?;
    let header = decompressor.read_header(bytes).map_err(decode_error)?;
    let [oriented_w, oriented_h] = orientation.oriented_size([header.width, header.height]);
    let fit = fit_ratio([oriented_w, oriented_h], target_px);

    let mut eighths = (1..=8).find(|&e| e as f64 / 8.0 >= fit).unwrap_or(8);
    while eighths > 1
        && turbojpeg::ScalingFactor::new(eighths, 8).scale(header.width.max(header.height)) > max_texture_side
    {
        eighths -= 1;
    }
    let scaling = turbojpeg::ScalingFactor::new(eighths, 8);
    decompressor.set_scaling_factor(scaling).map_err(decode_error)?;
    let scaled = header.scaled(scaling);

    // Decode straight into the texture's pixel buffer: JPEG is opaque, so unmultiplied RGBA == egui's premultiplied.
    let mut pixels = vec![Color32::BLACK; scaled.width * scaled.height];
    decompressor
        .decompress(
            bytes,
            turbojpeg::Image {
                pixels: bytemuck::cast_slice_mut(&mut pixels),
                width: scaled.width,
                pitch: 4 * scaled.width,
                height: scaled.height,
                format: turbojpeg::PixelFormat::RGBA,
            },
        )
        .map_err(decode_error)?;
    Ok((pixels, [scaled.width, scaled.height]))
}

/// PNG/WebP/GIF(first frame)/BMP/TIFF: rare in camera workflows, so a plain full decode + Lanczos is fine.
fn decode_other_format_to_fit(
    bytes: &[u8],
    target_px: [u32; 2],
    orientation: Orientation,
    max_texture_side: usize,
) -> AppResult<(Vec<Color32>, [usize; 2])> {
    let rgba = image::load_from_memory(bytes).map_err(decode_error)?.into_rgba8();
    let (width, height) = rgba.dimensions();
    let oriented = orientation.oriented_size([width as usize, height as usize]);
    let fit = fit_ratio(oriented, target_px).min(max_texture_side as f64 / width.max(height) as f64);
    let (out_w, out_h) = (
        ((width as f64 * fit).round() as u32).max(1),
        ((height as f64 * fit).round() as u32).max(1),
    );

    let rgba = if (out_w, out_h) == (width, height) {
        rgba.into_raw()
    } else {
        let source = Image::from_vec_u8(width, height, rgba.into_raw(), PixelType::U8x4).map_err(decode_error)?;
        let mut destination = Image::new(out_w, out_h, PixelType::U8x4);
        Resizer::new()
            .resize(
                &source,
                &mut destination,
                &ResizeOptions::new().resize_alg(ResizeAlg::Convolution(fast_image_resize::FilterType::Lanczos3)),
            )
            .map_err(decode_error)?;
        destination.into_vec()
    };
    let size = [out_w as usize, out_h as usize];
    Ok((ColorImage::from_rgba_unmultiplied(size, &rgba).pixels, size))
}

fn fit_ratio([width, height]: [usize; 2], target_px: [u32; 2]) -> f64 {
    (target_px[0] as f64 / width as f64)
        .min(target_px[1] as f64 / height as f64)
        .min(1.0)
}

fn decode_error(error: impl std::fmt::Display) -> AppError {
    AppError::Invalid(format!("can't decode image: {error}"))
}

/// EXIF orientation tag values 1–8.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Orientation {
    Upright,
    FlippedHorizontally,
    Rotated180,
    FlippedVertically,
    Transposed,
    Rotated90Clockwise,
    Transversed,
    Rotated270Clockwise,
}

impl Orientation {
    pub fn from_exif_value(value: u32) -> Self {
        match value {
            2 => Self::FlippedHorizontally,
            3 => Self::Rotated180,
            4 => Self::FlippedVertically,
            5 => Self::Transposed,
            6 => Self::Rotated90Clockwise,
            7 => Self::Transversed,
            8 => Self::Rotated270Clockwise,
            _ => Self::Upright,
        }
    }

    fn swaps_axes(self) -> bool {
        matches!(
            self,
            Self::Transposed | Self::Rotated90Clockwise | Self::Transversed | Self::Rotated270Clockwise
        )
    }

    fn oriented_size(self, [width, height]: [usize; 2]) -> [usize; 2] {
        if self.swaps_axes() {
            [height, width]
        } else {
            [width, height]
        }
    }
}

fn read_exif(bytes: &[u8]) -> Option<exif::Exif> {
    exif::Reader::new().read_from_container(&mut Cursor::new(bytes)).ok()
}

fn exif_orientation(exif: Option<&exif::Exif>) -> Orientation {
    exif.and_then(|exif| {
        exif.get_field(exif::Tag::Orientation, exif::In::PRIMARY)?
            .value
            .get_uint(0)
    })
    .map_or(Orientation::Upright, Orientation::from_exif_value)
}

/// Runs on the already-downscaled buffer, so it costs a few ms at most.
// ponytail: per-pixel index mapping; switch to tiled transposition if portrait shots show up in profiles.
fn apply_orientation(
    pixels: Vec<Color32>,
    [width, height]: [usize; 2],
    orientation: Orientation,
) -> (Vec<Color32>, [usize; 2]) {
    if orientation == Orientation::Upright {
        return (pixels, [width, height]);
    }
    let [out_w, out_h] = orientation.oriented_size([width, height]);
    let mut oriented = Vec::with_capacity(pixels.len());
    for y in 0..out_h {
        for x in 0..out_w {
            let (source_x, source_y) = match orientation {
                Orientation::Upright => (x, y),
                Orientation::FlippedHorizontally => (width - 1 - x, y),
                Orientation::Rotated180 => (width - 1 - x, height - 1 - y),
                Orientation::FlippedVertically => (x, height - 1 - y),
                Orientation::Transposed => (y, x),
                Orientation::Rotated90Clockwise => (y, height - 1 - x),
                Orientation::Transversed => (width - 1 - y, height - 1 - x),
                Orientation::Rotated270Clockwise => (width - 1 - y, x),
            };
            oriented.push(pixels[source_y * width + source_x]);
        }
    }
    (oriented, [out_w, out_h])
}
