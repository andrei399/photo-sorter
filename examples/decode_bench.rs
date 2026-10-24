//! Decoder benchmark (phase 02 spike + release QA).
//!
//! cargo run --release --example decode_bench -- <folder-with-jpegs> [target_w target_h]
//! cargo run --release --example decode_bench -- --generate <folder>   # synthetic 24 MP + 45 MP camera-like JPEGs
//!
//! Single-threaded, decode-to-fit-target timings. CPU numbers are valid under WSL; GPU numbers are not.

use fast_image_resize::{PixelType, ResizeAlg, ResizeOptions, Resizer, images::Image};
use std::path::{Path, PathBuf};
use std::time::Instant;

const REPETITIONS: usize = 3;

/// Decodes `bytes` to fit `target` and returns the output size.
type DecodeCandidate = fn(&[u8], [u32; 2]) -> [usize; 2];

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.first().map(String::as_str) == Some("--generate") {
        let folder = PathBuf::from(args.get(1).expect("--generate <folder>"));
        generate_camera_like_jpegs(&folder);
        return;
    }
    let folder = PathBuf::from(args.first().expect("usage: decode_bench <folder> [target_w target_h]"));
    let target = [
        args.get(1).map_or(2560, |w| w.parse().expect("target_w")),
        args.get(2).map_or(1440, |h| h.parse().expect("target_h")),
    ];

    let jpegs = list_jpegs(&folder);
    println!(
        "{} JPEGs, target {}x{}, {} reps each\n",
        jpegs.len(),
        target[0],
        target[1],
        REPETITIONS
    );
    let candidates: [(&str, DecodeCandidate); 5] = [
        ("photo_sorter decode_full", decode_ours_full),
        ("photo_sorter decode_glimpse (EXIF thumb)", decode_ours_glimpse),
        ("turbojpeg scaled", decode_turbojpeg_scaled),
        ("zune-jpeg full + resize", decode_zune_then_resize),
        ("jpeg-decoder scaled", decode_jpeg_decoder_scaled),
    ];
    let files: Vec<Vec<u8>> = jpegs
        .iter()
        .map(|path| std::fs::read(path).expect("read jpeg"))
        .collect();
    for (name, decode) in candidates {
        let mut timings_ms: Vec<f64> = Vec::new();
        let mut last_size = [0, 0];
        for bytes in &files {
            for _ in 0..REPETITIONS {
                let started = Instant::now();
                last_size = decode(bytes, target);
                timings_ms.push(started.elapsed().as_secs_f64() * 1000.0);
            }
        }
        timings_ms.sort_by(f64::total_cmp);
        let median = timings_ms[timings_ms.len() / 2];
        let p95 = timings_ms[(timings_ms.len() * 95 / 100).min(timings_ms.len() - 1)];
        println!(
            "{name:44} median {median:7.1} ms   p95 {p95:7.1} ms   (last output {}x{})",
            last_size[0], last_size[1]
        );
    }
}

fn list_jpegs(folder: &Path) -> Vec<PathBuf> {
    let mut jpegs: Vec<PathBuf> = std::fs::read_dir(folder)
        .expect("read folder")
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| {
            path.extension()
                .is_some_and(|ext| ext.eq_ignore_ascii_case("jpg") || ext.eq_ignore_ascii_case("jpeg"))
        })
        .collect();
    jpegs.sort();
    jpegs
}

fn decode_ours_full(bytes: &[u8], target: [u32; 2]) -> [usize; 2] {
    photo_sorter::imaging::decode::decode_full(bytes, target, 16384)
        .expect("decode")
        .size
}

fn decode_ours_glimpse(bytes: &[u8], _target: [u32; 2]) -> [usize; 2] {
    photo_sorter::imaging::decode::decode_glimpse(bytes).map_or([0, 0], |image| image.size)
}

fn fit_ratio(width: usize, height: usize, target: [u32; 2]) -> f64 {
    (target[0] as f64 / width as f64)
        .min(target[1] as f64 / height as f64)
        .min(1.0)
}

fn decode_turbojpeg_scaled(bytes: &[u8], target: [u32; 2]) -> [usize; 2] {
    let mut decompressor = turbojpeg::Decompressor::new().expect("decompressor");
    let header = decompressor.read_header(bytes).expect("header");
    let fit = fit_ratio(header.width, header.height, target);
    let scaling = (1..=8)
        .map(|eighths| turbojpeg::ScalingFactor::new(eighths, 8))
        .find(|factor| factor.num() as f64 / 8.0 >= fit)
        .unwrap_or(turbojpeg::ScalingFactor::ONE);
    decompressor.set_scaling_factor(scaling).expect("scaling");
    let scaled = header.scaled(scaling);
    let mut pixels = vec![0u8; 4 * scaled.width * scaled.height];
    decompressor
        .decompress(
            bytes,
            turbojpeg::Image {
                pixels: pixels.as_mut_slice(),
                width: scaled.width,
                pitch: 4 * scaled.width,
                height: scaled.height,
                format: turbojpeg::PixelFormat::RGBA,
            },
        )
        .expect("decompress");
    [scaled.width, scaled.height]
}

fn decode_zune_then_resize(bytes: &[u8], target: [u32; 2]) -> [usize; 2] {
    use zune_jpeg::zune_core::{colorspace::ColorSpace, options::DecoderOptions};
    let options = DecoderOptions::default().jpeg_set_out_colorspace(ColorSpace::RGBA);
    let mut decoder = zune_jpeg::JpegDecoder::new_with_options(bytes, options);
    let pixels = decoder.decode().expect("zune decode");
    let (width, height) = decoder.dimensions().expect("dimensions");
    let fit = fit_ratio(width, height, target);
    let (out_w, out_h) = ((width as f64 * fit) as u32, (height as f64 * fit) as u32);
    let source = Image::from_vec_u8(width as u32, height as u32, pixels, PixelType::U8x4).expect("image");
    let mut destination = Image::new(out_w, out_h, PixelType::U8x4);
    Resizer::new()
        .resize(
            &source,
            &mut destination,
            &ResizeOptions::new().resize_alg(ResizeAlg::Convolution(fast_image_resize::FilterType::Lanczos3)),
        )
        .expect("resize");
    [out_w as usize, out_h as usize]
}

fn decode_jpeg_decoder_scaled(bytes: &[u8], target: [u32; 2]) -> [usize; 2] {
    let mut decoder = jpeg_decoder::Decoder::new(bytes);
    decoder.read_info().expect("info");
    let (width, height) = decoder.scale(target[0] as u16, target[1] as u16).expect("scale");
    decoder.decode().expect("decode");
    [width as usize, height as usize]
}

/// Smooth gradients + texture + sensor-like noise, so compressed size and decode cost resemble real photos.
fn generate_camera_like_jpegs(folder: &Path) {
    std::fs::create_dir_all(folder).expect("create folder");
    let sizes: [(&str, u32, u32); 2] = [("24mp", 6000, 4000), ("45mp", 8256, 5504)];
    for (label, width, height) in sizes {
        for variant in 0..3u32 {
            let mut rng_state = 0x9E37_79B9u32.wrapping_add(variant * 7919);
            let image = image::RgbImage::from_fn(width, height, |x, y| {
                rng_state ^= rng_state << 13;
                rng_state ^= rng_state >> 17;
                rng_state ^= rng_state << 5;
                let noise = (rng_state % 24) as i32 - 12;
                let texture = (((x / 7) ^ (y / 11)) % 32) as i32;
                let channel = |base: u32, scale: u32| {
                    let gradient = ((x * scale / width + y * (255 - scale) / height + base * variant) % 256) as i32;
                    (gradient + texture + noise).clamp(0, 255) as u8
                };
                image::Rgb([channel(40, 200), channel(90, 120), channel(160, 60)])
            });
            let path = folder.join(format!("synthetic_{label}_{variant}.jpg"));
            let mut file = std::io::BufWriter::new(std::fs::File::create(&path).expect("create jpeg"));
            image::codecs::jpeg::JpegEncoder::new_with_quality(&mut file, 92)
                .encode_image(&image)
                .expect("encode");
            println!("wrote {}", path.display());
        }
    }
}
