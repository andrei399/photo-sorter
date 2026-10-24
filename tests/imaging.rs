mod common;

use common::{jpeg_bytes, with_exif};
use egui::Color32;
use photo_sorter::imaging::decode::{decode_full, decode_glimpse};

const RED: [u8; 3] = [230, 20, 20];
const BLUE: [u8; 3] = [20, 20, 230];

fn is_red(color: Color32) -> bool {
    color.r() > 150 && color.b() < 100
}

#[test]
fn jpeg_is_decoded_at_smallest_dct_scale_covering_the_target() {
    let jpeg = jpeg_bytes(4000, 3000, |x, y| [(x % 256) as u8, (y % 256) as u8, 90]);

    let image = decode_full(&jpeg, [1920, 1080], 16384, None).unwrap();

    // Fit is 1440×1080; 3/8 scale (1500×1125) is the smallest DCT step at or above it.
    assert_eq!(image.size, [1500, 1125]);
}

#[test]
fn jpeg_never_exceeds_max_texture_side() {
    let jpeg = jpeg_bytes(4000, 3000, |_, _| BLUE);
    let image = decode_full(&jpeg, [8000, 8000], 2048, None).unwrap();
    assert!(image.size[0] <= 2048 && image.size[1] <= 2048, "{:?}", image.size);
}

#[test]
fn small_images_are_never_upscaled() {
    let png = {
        let mut bytes = Vec::new();
        image::RgbaImage::from_pixel(800, 600, image::Rgba([1, 2, 3, 255]))
            .write_to(&mut std::io::Cursor::new(&mut bytes), image::ImageFormat::Png)
            .unwrap();
        bytes
    };
    assert_eq!(decode_full(&png, [3840, 2160], 16384, None).unwrap().size, [800, 600]);
}

#[test]
fn non_jpeg_formats_are_resized_to_fit() {
    let mut png = Vec::new();
    image::RgbaImage::from_pixel(2000, 1000, image::Rgba([1, 2, 3, 255]))
        .write_to(&mut std::io::Cursor::new(&mut png), image::ImageFormat::Png)
        .unwrap();
    assert_eq!(decode_full(&png, [1000, 1000], 16384, None).unwrap().size, [1000, 500]);
}

/// Stored image: 64×32, red top-left quadrant. Each EXIF orientation must move it to the right corner.
#[test]
fn all_eight_exif_orientations_display_upright() {
    let stored = jpeg_bytes(64, 32, |x, y| if x < 32 && y < 16 { RED } else { BLUE });
    // (orientation, expected size, corner where the red quadrant must end up: (right?, bottom?))
    let expectations = [
        (1, [64, 32], (false, false)),
        (2, [64, 32], (true, false)),
        (3, [64, 32], (true, true)),
        (4, [64, 32], (false, true)),
        (5, [32, 64], (false, false)),
        (6, [32, 64], (true, false)),
        (7, [32, 64], (true, true)),
        (8, [32, 64], (false, true)),
    ];
    for (orientation, expected_size, (red_right, red_bottom)) in expectations {
        let image = decode_full(&with_exif(&stored, orientation, None), [1000, 1000], 16384, None).unwrap();
        assert_eq!(image.size, expected_size, "orientation {orientation}");
        let [width, height] = image.size;
        let sample_x = if red_right { width * 7 / 8 } else { width / 8 };
        let sample_y = if red_bottom { height * 7 / 8 } else { height / 8 };
        let opposite = image.pixels[(height - 1 - sample_y) * width + (width - 1 - sample_x)];
        assert!(
            is_red(image.pixels[sample_y * width + sample_x]),
            "orientation {orientation}: red corner missing"
        );
        assert!(!is_red(opposite), "orientation {orientation}: opposite corner is red");
    }
}

#[test]
fn glimpse_uses_embedded_thumbnail_with_orientation() {
    let thumbnail = jpeg_bytes(160, 120, |_, _| RED);
    let photo = with_exif(&jpeg_bytes(1600, 1200, |_, _| BLUE), 6, Some(&thumbnail));

    let glimpse = decode_glimpse(&photo).expect("thumbnail present");

    assert_eq!(glimpse.size, [120, 160]);
    assert!(
        is_red(glimpse.pixels[0]),
        "glimpse must come from the thumbnail, not the main image"
    );
}

#[test]
fn glimpse_works_from_file_head_only() {
    let thumbnail = jpeg_bytes(160, 120, |_, _| RED);
    let photo = with_exif(
        &jpeg_bytes(2000, 1500, |x, _| [(x % 256) as u8, 0, 0]),
        1,
        Some(&thumbnail),
    );
    assert!(decode_glimpse(&photo[..64 * 1024]).is_some());
}

#[test]
fn glimpse_is_none_without_thumbnail() {
    assert!(decode_glimpse(&jpeg_bytes(100, 100, |_, _| BLUE)).is_none());
    assert!(decode_glimpse(&with_exif(&jpeg_bytes(100, 100, |_, _| BLUE), 1, None)).is_none());
}

#[test]
fn corrupt_input_is_an_error_not_a_panic() {
    let jpeg = jpeg_bytes(400, 300, |_, _| BLUE);
    assert!(decode_full(&jpeg[..jpeg.len() / 3], [100, 100], 16384, None).is_err());
    assert!(decode_full(&[], [100, 100], 16384, None).is_err());
    assert!(decode_full(b"definitely not an image", [100, 100], 16384, None).is_err());
    assert!(decode_full(&[0xFF, 0xD8, 0xFF, 0xE0, 0, 0], [100, 100], 16384, None).is_err());
}
