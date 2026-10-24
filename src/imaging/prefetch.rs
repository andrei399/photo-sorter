//! Decodes photos around the current one on a thread pool and keeps them as GPU textures,
//! so pressing → swaps a texture handle instead of decoding.

use crate::imaging::decode::{decode_full, decode_glimpse};
use crossbeam_channel::{Receiver, Sender, unbounded};
use std::collections::HashMap;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::{Duration, Instant};

/// Full decodes kept around the current photo, in priority order for forward travel (mirrored for backward).
const FULL_WINDOW_FORWARD: [isize; 7] = [0, 1, 2, 3, 4, -1, -2];
/// Glimpses are ~0.1 ms and read only the file head, so they cover a much wider window.
const GLIMPSE_RADIUS_BEHIND: usize = 5;
const GLIMPSE_RADIUS_AHEAD: usize = 12;
/// EXIF (APP1) can't exceed 64 KiB, so the thumbnail is always inside the file head.
const EXIF_HEAD_BYTES: u64 = 64 * 1024;
const RESIZE_SETTLE_DELAY: Duration = Duration::from_millis(150);

#[derive(Default)]
pub enum Slot {
    #[default]
    NotRequested,
    Pending,
    Ready(egui::TextureHandle),
    Failed(String),
}

#[derive(Default)]
pub struct CacheEntry {
    pub glimpse: Slot,
    pub full: Slot,
    /// Viewport size the full decode was made for; a much bigger window triggers a sharper re-decode.
    full_decoded_for: [u32; 2],
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Tier {
    Glimpse,
    Full,
}

struct DecodeJob {
    path: PathBuf,
    index: usize,
    tier: Tier,
    target_px: [u32; 2],
    max_texture_side: usize,
}

enum DecodeOutcome {
    Ready(egui::TextureHandle),
    Failed(String),
    /// The user moved away before the job started; the slot goes back to NotRequested.
    Skipped,
}

struct DecodeResult {
    path: PathBuf,
    tier: Tier,
    target_px: [u32; 2],
    outcome: DecodeOutcome,
}

pub struct ImageCache {
    pool: rayon::ThreadPool,
    entries: HashMap<PathBuf, CacheEntry>,
    results_tx: Sender<DecodeResult>,
    results_rx: Receiver<DecodeResult>,
    /// Current index, read by workers to drop stale jobs before doing any I/O.
    focus: Arc<AtomicUsize>,
    previous_index: Option<usize>,
    travelling_backward: bool,
    viewport_px: [u32; 2],
    viewport_changed_at: Instant,
}

impl Default for ImageCache {
    fn default() -> Self {
        Self::new()
    }
}

impl ImageCache {
    pub fn new() -> Self {
        let threads = std::thread::available_parallelism().map_or(2, |n| n.get().saturating_sub(1).max(2));
        let pool = rayon::ThreadPoolBuilder::new()
            .num_threads(threads)
            .thread_name(|i| format!("decode-{i}"))
            .build()
            .expect("decode thread pool");
        let (results_tx, results_rx) = unbounded();
        Self {
            pool,
            entries: HashMap::new(),
            results_tx,
            results_rx,
            focus: Arc::new(AtomicUsize::new(0)),
            previous_index: None,
            travelling_backward: false,
            viewport_px: [0, 0],
            viewport_changed_at: Instant::now(),
        }
    }

    pub fn entry(&self, path: &Path) -> Option<&CacheEntry> {
        self.entries.get(path)
    }

    /// Sharpest texture available for `path`: full decode, else the glimpse.
    pub fn best_texture(&self, path: &Path) -> Option<&egui::TextureHandle> {
        let entry = self.entries.get(path)?;
        match (&entry.full, &entry.glimpse) {
            (Slot::Ready(texture), _) | (_, Slot::Ready(texture)) => Some(texture),
            _ => None,
        }
    }

    /// Called every frame by the viewer: absorbs finished decodes, schedules missing ones, evicts far ones.
    pub fn want(&mut self, ctx: &egui::Context, photo_paths: &[PathBuf], index: usize, viewport_px: [u32; 2]) {
        self.absorb_results();
        if photo_paths.is_empty() {
            self.entries.clear();
            return;
        }
        let index = index.min(photo_paths.len() - 1);
        if let Some(previous) = self.previous_index
            && previous != index
        {
            self.travelling_backward = index < previous;
        }
        self.previous_index = Some(index);
        self.focus.store(index, Ordering::Relaxed);
        if viewport_px != self.viewport_px {
            self.viewport_px = viewport_px;
            self.viewport_changed_at = Instant::now();
        }

        let full_window = self.full_window(index, photo_paths.len());
        let glimpse_window =
            index.saturating_sub(GLIMPSE_RADIUS_BEHIND)..=(index + GLIMPSE_RADIUS_AHEAD).min(photo_paths.len() - 1);
        let max_texture_side = ctx.input(|i| i.max_texture_side);

        self.redecode_current_if_window_grew(ctx, &photo_paths[index], index, max_texture_side);

        // Glimpses first: they finish in microseconds and guarantee something is on screen.
        for glimpse_index in [index].into_iter().chain(glimpse_window.clone()) {
            self.request(
                ctx,
                &photo_paths[glimpse_index],
                glimpse_index,
                Tier::Glimpse,
                max_texture_side,
            );
        }
        for &full_index in &full_window {
            self.request(ctx, &photo_paths[full_index], full_index, Tier::Full, max_texture_side);
        }

        let keep_full: Vec<&PathBuf> = full_window.iter().map(|&i| &photo_paths[i]).collect();
        let keep_glimpse = &photo_paths[glimpse_window];
        self.entries.retain(|path, entry| {
            if !keep_full.contains(&path) {
                entry.full = Slot::NotRequested;
            }
            keep_glimpse.contains(path) || keep_full.contains(&path)
        });
    }

    fn full_window(&self, index: usize, photo_count: usize) -> Vec<usize> {
        FULL_WINDOW_FORWARD
            .iter()
            .map(|&offset| if self.travelling_backward { -offset } else { offset })
            .filter_map(|offset| index.checked_add_signed(offset))
            .filter(|&i| i < photo_count)
            .collect()
    }

    fn redecode_current_if_window_grew(
        &mut self,
        ctx: &egui::Context,
        current_path: &Path,
        index: usize,
        max_texture_side: usize,
    ) {
        let Some(entry) = self.entries.get_mut(current_path) else {
            return;
        };
        let grew_beyond = |now: u32, then: u32| now as f64 > then as f64 * 1.1;
        let window_grew = matches!(entry.full, Slot::Ready(_))
            && (grew_beyond(self.viewport_px[0], entry.full_decoded_for[0])
                || grew_beyond(self.viewport_px[1], entry.full_decoded_for[1]));
        if !window_grew {
            return;
        }
        let settled_for = self.viewport_changed_at.elapsed();
        if settled_for < RESIZE_SETTLE_DELAY {
            ctx.request_repaint_after(RESIZE_SETTLE_DELAY - settled_for);
            return;
        }
        // The old texture stays on screen: results for the previous target size are ignored from now on.
        entry.full_decoded_for = self.viewport_px;
        self.spawn(ctx, current_path, index, Tier::Full, max_texture_side);
    }

    fn request(&mut self, ctx: &egui::Context, path: &Path, index: usize, tier: Tier, max_texture_side: usize) {
        let entry = self.entries.entry(path.to_path_buf()).or_default();
        let slot = match tier {
            Tier::Glimpse => &mut entry.glimpse,
            Tier::Full => &mut entry.full,
        };
        if matches!(slot, Slot::NotRequested) {
            *slot = Slot::Pending;
            if tier == Tier::Full {
                entry.full_decoded_for = self.viewport_px;
            }
            self.spawn(ctx, path, index, tier, max_texture_side);
        }
    }

    fn spawn(&self, ctx: &egui::Context, path: &Path, index: usize, tier: Tier, max_texture_side: usize) {
        let job = DecodeJob {
            path: path.to_path_buf(),
            index,
            tier,
            target_px: self.viewport_px,
            max_texture_side,
        };
        let (ctx, focus, results_tx) = (ctx.clone(), Arc::clone(&self.focus), self.results_tx.clone());
        self.pool.spawn_fifo(move || {
            let result = run_decode_job(&ctx, job, &focus);
            // The cache was dropped (folder closed): nobody wants this texture anymore.
            let _ = results_tx.send(result);
            ctx.request_repaint();
        });
    }

    fn absorb_results(&mut self) {
        while let Ok(result) = self.results_rx.try_recv() {
            let Some(entry) = self.entries.get_mut(&result.path) else {
                continue;
            };
            let slot = match result.tier {
                Tier::Glimpse => &mut entry.glimpse,
                Tier::Full => &mut entry.full,
            };
            // A re-decode for a bigger window may finish after an older one; never downgrade.
            if result.tier == Tier::Full && result.target_px != entry.full_decoded_for {
                continue;
            }
            *slot = match result.outcome {
                DecodeOutcome::Ready(texture) => Slot::Ready(texture),
                DecodeOutcome::Failed(message) => Slot::Failed(message),
                DecodeOutcome::Skipped => Slot::NotRequested,
            };
        }
    }
}

fn run_decode_job(ctx: &egui::Context, job: DecodeJob, focus: &AtomicUsize) -> DecodeResult {
    let distance = focus.load(Ordering::Relaxed).abs_diff(job.index);
    let still_wanted = match job.tier {
        Tier::Glimpse => distance <= GLIMPSE_RADIUS_AHEAD,
        Tier::Full => distance <= FULL_WINDOW_FORWARD.len(),
    };
    let outcome = if !still_wanted {
        DecodeOutcome::Skipped
    } else {
        let decoded = match job.tier {
            Tier::Glimpse => read_file_head(&job.path)
                .map_err(|error| error.to_string())
                .and_then(|head| decode_glimpse(&head).ok_or_else(|| "no embedded thumbnail".to_owned())),
            Tier::Full => std::fs::read(&job.path)
                .map_err(|error| error.to_string())
                .and_then(|bytes| {
                    decode_full(&bytes, job.target_px, job.max_texture_side).map_err(|error| error.to_string())
                }),
        };
        match decoded {
            Ok(image) => {
                let name = format!(
                    "{}#{}",
                    job.path.display(),
                    if job.tier == Tier::Full { "full" } else { "glimpse" }
                );
                DecodeOutcome::Ready(ctx.load_texture(name, image, egui::TextureOptions::LINEAR))
            }
            Err(message) => DecodeOutcome::Failed(message),
        }
    };
    DecodeResult {
        path: job.path,
        tier: job.tier,
        target_px: job.target_px,
        outcome,
    }
}

fn read_file_head(path: &Path) -> std::io::Result<Vec<u8>> {
    let mut head = Vec::with_capacity(EXIF_HEAD_BYTES as usize);
    std::fs::File::open(path)?
        .take(EXIF_HEAD_BYTES)
        .read_to_end(&mut head)?;
    Ok(head)
}
