mod common;

use common::raw_fixtures::{cr3_raw, raf_raw, tiff_raw};
use common::{jpeg_bytes, with_exif};
use photo_sorter::imaging::decode::decode_full;
use photo_sorter::imaging::prefetch::{ImageCache, Slot};
use photo_sorter::imaging::raw_preview::{decode_raw_glimpse, extract_raw_preview, extract_raw_preview_from};
use std::io::{Cursor, Read, Seek, SeekFrom};
use std::path::PathBuf;
use std::time::{Duration, Instant};

/// Landscape 2400×1600 preview, as cameras store it; every fixture says "rotated 90°", so it must come out portrait.
fn preview() -> Vec<u8> {
    jpeg_bytes(2400, 1600, |x, y| [(x / 10) as u8, (y / 10) as u8, 90])
}

fn thumbnail() -> Vec<u8> {
    jpeg_bytes(160, 120, |_, _| [200, 30, 30])
}

/// (name, file bytes). Orientation 6 and 8 both swap axes.
fn fixtures() -> Vec<(&'static str, Vec<u8>)> {
    vec![
        ("photo.NEF", tiff_raw(&preview(), 6)),
        ("photo.CR3", cr3_raw(&preview(), &thumbnail(), 8)),
        ("photo.RAF", raf_raw(&with_exif(&preview(), 6, Some(&thumbnail())))),
    ]
}

struct CountingReader<R> {
    inner: R,
    bytes_read: u64,
}

impl<R: Read> Read for CountingReader<R> {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        let n = self.inner.read(buf)?;
        self.bytes_read += n as u64;
        Ok(n)
    }
}

impl<R: Seek> Seek for CountingReader<R> {
    fn seek(&mut self, pos: SeekFrom) -> std::io::Result<u64> {
        self.inner.seek(pos)
    }
}

#[test]
fn extracts_upright_full_size_preview_per_format() {
    for (name, raw) in fixtures() {
        let preview = extract_raw_preview_from(Cursor::new(&raw)).unwrap_or_else(|e| panic!("{name}: {e}"));

        assert!(preview.jpeg.starts_with(&[0xFF, 0xD8]), "{name}");
        let image = decode_full(&preview.jpeg, [u32::MAX, u32::MAX], usize::MAX, preview.orientation).unwrap();
        assert_eq!(image.size, [1600, 2400], "{name}: full size and upright");
    }
}

#[test]
fn reads_only_the_preview_range() {
    for (name, raw) in fixtures() {
        let mut reader = CountingReader {
            inner: Cursor::new(&raw),
            bytes_read: 0,
        };
        let preview = extract_raw_preview_from(&mut reader).unwrap();

        assert!(
            reader.bytes_read * 10 <= raw.len() as u64,
            "{name}: read {} of {}",
            reader.bytes_read,
            raw.len()
        );
        assert!(
            reader.bytes_read <= preview.jpeg.len() as u64 + 4096,
            "{name}: header overhead too big"
        );
    }
}

#[test]
fn glimpse_is_small_and_upright() {
    let dir = tempfile::tempdir().unwrap();
    for (name, raw) in fixtures() {
        let path = dir.path().join(name);
        std::fs::write(&path, raw).unwrap();

        let glimpse = decode_raw_glimpse(&path).unwrap_or_else(|| panic!("{name}: no glimpse"));

        let [width, height] = glimpse.size;
        assert!(height > width && height <= 640, "{name}: {:?}", glimpse.size);
    }
}

#[test]
fn corrupt_raws_error_and_never_panic() {
    for (name, mut raw) in fixtures() {
        for cut in (0..raw.len()).step_by(raw.len() / 97).chain(0..64) {
            let _ = extract_raw_preview_from(Cursor::new(&raw[..cut]));
        }
        // Flip every header byte in turn: offsets and lengths become garbage.
        for at in 0..1024.min(raw.len()) {
            raw[at] ^= 0xFF;
            let _ = extract_raw_preview_from(Cursor::new(&raw));
            raw[at] ^= 0xFF;
        }
        assert!(
            extract_raw_preview_from(Cursor::new(&raw[..raw.len() / 20])).is_err(),
            "{name}"
        );
    }
    assert!(extract_raw_preview_from(Cursor::new(b"definitely not a raw")).is_err());
}

#[test]
fn raw_photos_decode_through_the_prefetch_pipeline() {
    let dir = tempfile::tempdir().unwrap();
    let paths: Vec<PathBuf> = fixtures()
        .into_iter()
        .map(|(name, raw)| {
            let path = dir.path().join(name);
            std::fs::write(&path, raw).unwrap();
            path
        })
        .collect();
    let ctx = egui::Context::default();
    let mut cache = ImageCache::new();

    let deadline = Instant::now() + Duration::from_secs(10);
    let all_ready = |cache: &ImageCache| {
        paths.iter().all(|path| {
            cache
                .entry(path)
                .is_some_and(|entry| matches!(entry.full, Slot::Ready(_)))
        })
    };
    while !all_ready(&cache) {
        assert!(Instant::now() < deadline, "RAW previews never decoded");
        cache.want(&ctx, &paths, 0, [1920, 1080]);
        std::thread::sleep(Duration::from_millis(5));
    }
    for path in &paths {
        let [width, height] = cache.best_texture(path).unwrap().size();
        assert!(height > width && height >= 1080, "{}: {width}×{height}", path.display());
    }
    assert!(extract_raw_preview(&dir.path().join("missing.CR3")).is_err());
}
