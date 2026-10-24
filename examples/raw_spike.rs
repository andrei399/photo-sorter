//! Phase 06 spike: for every RAW in a folder, how big is the embedded preview, how much of the file do we
//! read to get it, and how long does it take. `cargo run --release --example raw_spike -- qa-photos/raw`

use photo_sorter::imaging::decode::decode_full;
use photo_sorter::imaging::raw_preview::{decode_raw_glimpse, extract_raw_preview_from, is_raw_file};
use std::io::{BufReader, Read, Seek, SeekFrom};
use std::time::Instant;

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

fn main() {
    let folder = std::env::args().nth(1).unwrap_or_else(|| "qa-photos/raw".into());
    let mut paths: Vec<_> = std::fs::read_dir(&folder)
        .expect("folder")
        .filter_map(|e| Some(e.ok()?.path()))
        .filter(|p| is_raw_file(p))
        .collect();
    paths.sort();
    println!("| File | Preview | Orientation | Bytes read / file (preview bytes) | Extract ms | Glimpse |");
    println!("|---|---|---|---|---|---|");
    for path in paths {
        let file_len = std::fs::metadata(&path).unwrap().len();
        let mut reader = CountingReader {
            inner: BufReader::with_capacity(4096, std::fs::File::open(&path).unwrap()),
            bytes_read: 0,
        };
        let started = Instant::now();
        let result = extract_raw_preview_from(&mut reader);
        let extract_ms = started.elapsed().as_secs_f64() * 1000.0;
        let name = path.file_name().unwrap().to_string_lossy();
        let glimpse_started = Instant::now();
        let glimpse = decode_raw_glimpse(&path).map(|g| {
            format!(
                "{}×{} in {:.1} ms",
                g.size[0],
                g.size[1],
                glimpse_started.elapsed().as_secs_f64() * 1000.0
            )
        });
        match result {
            Ok(preview) => {
                let image = decode_full(&preview.jpeg, [u32::MAX, u32::MAX], usize::MAX, preview.orientation);
                if let (Ok(image), Some(dump_dir)) = (&image, std::env::var_os("RAW_SPIKE_DUMP")) {
                    let thumb = decode_full(&preview.jpeg, [400, 400], usize::MAX, preview.orientation).unwrap();
                    let rgba: Vec<u8> = thumb.pixels.iter().flat_map(|p| p.to_array()).collect();
                    image::RgbaImage::from_raw(thumb.size[0] as u32, thumb.size[1] as u32, rgba)
                        .unwrap()
                        .save(std::path::Path::new(&dump_dir).join(format!("{name}.png")))
                        .unwrap();
                    let _ = image;
                }
                let size = image
                    .map(|i| format!("{}×{}", i.size[0], i.size[1]))
                    .unwrap_or_else(|e| e.to_string());
                println!(
                    "| {name} | {size} | {:?} | {:.1} % ({:.2} MB, preview {:.2} MB) | {extract_ms:.1} | {} |",
                    preview.orientation,
                    reader.bytes_read as f64 * 100.0 / file_len as f64,
                    reader.bytes_read as f64 / 1e6,
                    preview.jpeg.len() as f64 / 1e6,
                    glimpse.unwrap_or_else(|| "none".into())
                );
            }
            Err(error) => println!("| {name} | ERROR {error} | | | | |"),
        }
    }
}
