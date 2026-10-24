mod common;

use common::jpeg_bytes;
use photo_sorter::imaging::prefetch::{ImageCache, Slot};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

fn write_photos(count: usize) -> (tempfile::TempDir, Vec<PathBuf>) {
    let dir = tempfile::tempdir().unwrap();
    let paths = (0..count)
        .map(|i| {
            let path = dir.path().join(format!("img_{i:02}.jpg"));
            std::fs::write(&path, jpeg_bytes(320, 240, |x, y| [(x + i as u32) as u8, y as u8, 0])).unwrap();
            path
        })
        .collect();
    (dir, paths)
}

fn full_is_ready(cache: &ImageCache, path: &Path) -> bool {
    cache
        .entry(path)
        .is_some_and(|entry| matches!(entry.full, Slot::Ready(_)))
}

/// Calls `want` like the UI does each frame until `settled` holds.
fn want_until(
    cache: &mut ImageCache,
    ctx: &egui::Context,
    paths: &[PathBuf],
    index: usize,
    settled: impl Fn(&ImageCache) -> bool,
) {
    let deadline = Instant::now() + Duration::from_secs(10);
    while Instant::now() < deadline {
        cache.want(ctx, paths, index, [1920, 1080]);
        if settled(cache) {
            return;
        }
        std::thread::sleep(Duration::from_millis(5));
    }
    panic!("prefetch window never settled for index {index}");
}

#[test]
fn prefetches_window_around_current_and_evicts_when_jumping_away() {
    let (_dir, paths) = write_photos(20);
    let ctx = egui::Context::default();
    let mut cache = ImageCache::new();

    want_until(&mut cache, &ctx, &paths, 10, |cache| {
        (8..=14).all(|i| full_is_ready(cache, &paths[i]))
    });
    assert!(cache.best_texture(&paths[10]).is_some());
    assert!(!full_is_ready(&cache, &paths[16]), "outside the window");

    // Jumping to the start travels backward: the window is the current photo plus what's ahead of it in the list.
    want_until(&mut cache, &ctx, &paths, 0, |cache| {
        (0..=2).all(|i| full_is_ready(cache, &paths[i]))
    });
    assert!(
        (8..=14).all(|i| !full_is_ready(&cache, &paths[i])),
        "far textures must be freed"
    );
}

#[test]
fn unreadable_photo_becomes_failed_slot() {
    let (dir, mut paths) = write_photos(3);
    let broken = dir.path().join("broken.jpg");
    std::fs::write(&broken, b"not a jpeg").unwrap();
    paths.insert(1, broken.clone());
    let ctx = egui::Context::default();
    let mut cache = ImageCache::new();

    want_until(&mut cache, &ctx, &paths, 1, |cache| {
        cache
            .entry(&broken)
            .is_some_and(|entry| matches!(entry.full, Slot::Failed(_)))
    });
    assert!(cache.best_texture(&broken).is_none());
}
