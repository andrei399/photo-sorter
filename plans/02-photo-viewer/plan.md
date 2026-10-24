# 02 — Photo Viewer & Imaging Pipeline

Goal: open a folder and browse its photos full-screen. ←/→ is **instant** (the next photo appears within one frame when prefetched), cold jumps never show a blank screen, and memory stays bounded. This phase is where "blazingly fast" is won or lost.

## Step 0a — Design gate (house rule)

Build **3 distinct static HTML mocks** of the viewer screen and publish them (html-communication / Artifact). Report the URL and **stop until Andrei picks one**. The mocks are for layout and information hierarchy only; the egui build reproduces the layout, not pixel-perfect CSS.
1. **Pure cinema**: black, photo edge to edge, the top bar fades in on mouse move (Windows Photos style).
2. **Thin top bar**: a persistent 36 px bar with folder name, `12 / 340`, session picker (phase 03, mocked now) and ★.
3. **Bottom strip**: a ±5 neighbour thumbnail strip under the photo. Thumbnails are nearly free here: they're the EXIF thumbnails the glimpse tier already loads.

All three must show where the ★ and the active session name live.

## Step 0b — Decoder spike (time-box: half a day)

Create `examples/decode_bench.rs`: for every JPEG in a folder, decode to fit a 2560×1440 target and print the median and p95 ms per file, single-threaded. Compare:
| Candidate | Approach |
|---|---|
| `turbojpeg` (libjpeg-turbo, vendored, SIMD via nasm) | DCT-scaled decode at the smallest M/8 factor ≥ fit |
| `zune-jpeg` (pure Rust, SIMD) | full decode + `fast_image_resize` down to fit |
| `jpeg-decoder` (pure Rust) | `Decoder::scale()` DCT scaling |

Use real 24 MP and 45 MP camera JPEGs on native hardware, not WSLg. Expected: turbojpeg scaled wins by 3–5×. Pick it if it wins by > 1.5× and builds cleanly in CI on all 3 OSes. If a pure-Rust option comes within 1.5×, take it (no cmake/nasm). **Write the numbers and the decision into this file** before step 1. The rest of this plan assumes turbojpeg.

### Spike result (2026-10-24, i7-14700KF, single thread, target 2560×1440)

| Candidate | 9 real camera JPEGs (A7 III, D850, A7R IV; 21–49 MB, 20–60 MP) median / p95 | 6 synthetic (24 + 45 MP, 8–16 MB) median |
|---|---|---|
| turbojpeg DCT-scaled | **265 / 497 ms** | **126 ms** |
| jpeg-decoder scaled | 317 / 636 ms | 168 ms |
| zune-jpeg full + Lanczos resize | 406 / 661 ms | 291 ms |
| turbojpeg ⅛ scale | 219 ms | 99 ms |
| EXIF thumbnail (glimpse) | **0.1 ms** | — |

Findings:
- **Huffman decoding dominates**: cost grows with the compressed size, and DCT scaling only saves ~20 %. A ⅛-scale "glimpse" decode is nearly as slow as a full one, so the glimpse tier uses the **embedded EXIF thumbnail** instead. All 9 real files had one (5–12 KB, ~160×120). It's read from the first 64 KB of the file only.
- **Decision: turbojpeg.** It's only ~1.2× faster than jpeg-decoder, which misses the 1.5× bar, but we override that bar: jpeg-decoder's coarse ½/¼/⅛ steps produce up to 2× the target size (more memory and upload time), it's in maintenance mode, and the cmake/nasm cost is already paid in CI. It also beats pure-Rust zune-jpeg + resize by 1.5–2.3×.
- These Commons files are re-encoded at very high quality. Typical out-of-camera files (24 MP fine, ~10 MB) land around the synthetic numbers (~100–130 ms). Sequential browsing hides this through prefetch; cold jumps show the glimpse first.

## Steps

### 1. Folder scan (`processor/photo_folder.rs`)

```rust
/// What the imaging pipeline can decode. RAW joins in phase 06.
const VIEWABLE_EXTENSIONS: &[&str] = &["jpg", "jpeg", "png", "webp", "gif", "bmp", "tif", "tiff"];

#[derive(Debug, Clone)]
pub struct Photo { pub path: PathBuf, pub file_name: String }

#[derive(Debug, Clone)]
pub struct PhotoFolder { pub folder: PathBuf, pub photos: Vec<Photo> }

pub fn scan_photo_folder(folder: &Path) -> AppResult<PhotoFolder>
```
- Non-recursive `read_dir`. Case-insensitive extension match (`IMG_0001.JPG`). Skip hidden files and macOS `._*` AppleDouble files.
- **Natural sort** with `natord::compare_ignore_case`, so `IMG_2` comes before `IMG_10`. Filenames from one camera are already chronological, so EXIF-date sorting is skipped.
- Missing folder → `NotFound`. Empty folder → `Ok` with an empty list.

### 2. Core wiring

- `Command::OpenFolder(PathBuf)` → `Event::FolderOpened(PhotoFolder)`, and persist `last_opened_folder`.
- On core startup: after `Snapshot`, if `last_opened_folder` exists and is readable, emit `FolderOpened` for it. If it's missing (unplugged SD card), emit nothing.

### 3. Decoder (`imaging/decode.rs`)

```rust
/// Display-sized, upright. JPEG decodes straight into the texture's `Vec<Color32>` (no conversion pass).
pub fn decode_full(bytes: &[u8], target_px: [u32; 2], max_texture_side: usize) -> AppResult<egui::ColorImage>
/// The EXIF thumbnail (~160×120), oriented. Works on just the first 64 KB of the file.
pub fn decode_glimpse(bytes: &[u8]) -> Option<egui::ColorImage>
```
1. Sniff the format by **magic bytes**, not extension. `FF D8 FF` → JPEG path, anything else → `image` crate path.
2. Read the EXIF orientation (`kamadak-exif`, `Reader::read_from_container`). Orientations 5–8 swap width and height for fit calculations.
3. **JPEG**: `turbojpeg` read header → compute `fit = min(target_w / w, target_h / h, 1.0)` (on the oriented dimensions) → choose the **smallest libjpeg-turbo scaling factor (M/8) ≥ fit** → decompress straight to `PixelFormat::RGBA` into a `Vec<Color32>` via `bytemuck` (JPEG is opaque, so unmultiplied equals premultiplied, with no copy).
4. **Other formats**: `image::load_from_memory` → `fast_image_resize` (Lanczos3) down to fit if it's larger. Never upscale.
5. Apply the orientation to the (now small) buffer with `image::imageops` rotate/flip.
6. Clamp to `max_texture_side`, which the caller passes in from `ctx.input(|i| i.max_texture_side)`.

The output is at most ~1.25× the viewport in each dimension, and the GPU's linear filtering handles the rest.

Deps: `turbojpeg = { version = "1", features = ["cmake"] }` (vendored build), `image = { version = "0.25", default-features = false, features = ["png", "webp", "gif", "bmp", "tiff"] }`, `fast_image_resize`, `kamadak-exif`, `natord`, `rayon`.

### 4. Prefetching texture cache (`imaging/prefetch.rs`)

```rust
pub struct ImageCache {
    pool: rayon::ThreadPool,                       // available_parallelism − 1, min 2; thread name "decode-N"
    entries: HashMap<PathBuf, CacheEntry>,
    results: Receiver<DecodeResult>,
    results_tx: Sender<DecodeResult>,
    focus: Arc<AtomicUsize>,                       // current index, read by workers to skip stale jobs
}

pub enum Slot { NotRequested, Pending, Ready(egui::TextureHandle), Failed(String) }
pub struct CacheEntry { pub glimpse: Slot, pub full: Slot, full_decoded_for: [u32; 2] }

impl ImageCache {
    /// Called every frame by the viewer. Cheap when nothing changed.
    pub fn want(&mut self, ctx: &egui::Context, photos: &[Photo], index: usize, viewport_px: [u32; 2])
    pub fn get(&self, path: &Path) -> Option<&CacheEntry>
}
```
`want`:
1. Drain `results` into `entries`.
2. Priority window based on travel direction: moving forward → `[i, i+1, i+2, i+3, i−1, i−2]`, backward → mirrored.
3. Request glimpses for the wide window (−5…+12, cheap head reads) first, then full decodes for the priority window. Each `NotRequested` slot becomes `Pending` and gets a `pool.spawn_fifo(job)`.
4. **Evict** every entry outside the window. Dropping the `TextureHandle` frees the GPU texture. Memory is bounded at about 6 viewport-sized textures (~6 × 15 MB at 1440p).
5. If the viewport grew by more than 10 % beyond the current photo's `decoded_for` (window resized or moved to a bigger monitor), re-request `Full` once the size has been stable for 150 ms.

Job (on a worker):
1. If the job is now outside its window (`focus` moved), report `Skipped`, which resets the slot to `NotRequested` so it's re-requested if the user comes back. Stale work costs ~0.
2. Glimpse: read the first 64 KB → `decode_glimpse`. Full: `std::fs::read(path)` → `decode_full`. Then `ctx.load_texture(name, image, TextureOptions::LINEAR)`. `load_texture` is thread-safe, and the GPU upload happens on the next frame, so the UI thread never touches pixels.
3. Send a `DecodeResult`, then `ctx.request_repaint()`.
4. Glimpse and full live in separate slots. The viewer shows full if ready, else glimpse. A full result made for an outdated viewport size is ignored.

<!-- ponytail: FIFO pool + stale-skip instead of a real priority queue; switch to a shared BinaryHeap if held-arrow benchmarks show queueing delay -->

### 5. Viewer UI (`ui/viewer.rs`), per the picked mock

- Central panel, background `#000`. Paint the current texture with `ui.painter().image(tex.id(), fitted_rect, uv, Color32::WHITE)`. `fitted_rect` is aspect-fit inside the panel and centred.
- The glimpse is drawn the same way (upscaled, briefly soft). `Pending` with nothing at all keeps painting **the previous photo** until something arrives, so you never see a blank frame. `Failed` shows "Can't display IMG_0042.JPG: <reason>" centred, and navigation keeps working.
- Overlay: file name, `index+1 / total`, and ◀ ▶ click zones on the left and right thirds.
- Ends: navigation **stops** at the first and last photo (no wrap). Show a 150 ms edge-shake on the overlay.
- Empty folder: "No photos in this folder".
- `ctx.request_repaint()` only during animations. Otherwise egui sleeps.

### 6. Opening folders

- **Open folder** button and `Mod+O` → `rfd::FileDialog::new().pick_folder()` (blocking is fine: the dialog is modal) → `Command::OpenFolder`.
- **Drag-and-drop**: read `ctx.input(|i| i.raw.dropped_files)`. A folder opens directly. A file opens its parent folder and jumps to that file. Works on Windows, macOS and X11. winit doesn't support Wayland drag-and-drop, so the button is the path there (documented).

### 7. Keyboard (hard-coded here, rebindable in phase 04)

`ui/keybindings.rs` from day one:
```rust
pub enum Action { Next, Previous, First, Last, OpenFolder }
pub fn triggered_actions(ctx: &egui::Context) -> Vec<Action>   // default table for now
```
- `ArrowRight`/`ArrowLeft` → next/previous (`key_pressed` includes auto-repeat, so held arrows work), `Home`/`End`, `Mod+O`.
- Skip everything when `ctx.wants_keyboard_input()` (text field focused).

### 8. Tests

No binary fixtures: tests generate JPEGs with `image` and splice EXIF segments (orientation, thumbnail) byte by byte, the way cameras write them.

`tests/photo_folder.rs` (core flows):
1. **open_folder_filters_and_sorts**: `IMG_10.JPG`, `IMG_2.jpg`, `notes.txt`, `.hidden.jpg`, `._IMG_2.jpg`, `sub/IMG_1.jpg` → `FolderOpened` with `[IMG_2.jpg, IMG_10.JPG]`.
2. **last_folder_persisted_and_reopened**: open → `state_on_disk()` has the folder → spawn a new core on the same dir → it emits `Snapshot` then `FolderOpened`.
3. **missing_folder_errors** → `Event::Error(NotFound)`. **empty_folder_ok**.

`tests/imaging.rs`:
4. **jpeg_scaled_to_fit**: 4000×3000 at target 1920×1080 → exactly 1500×1125 (3/8, the smallest DCT step ≥ fit). Never above `max_texture_side`.
5. **glimpse_uses_embedded_thumbnail**: EXIF built in-test with an IFD1 thumbnail and orientation 6 → a 120×160 glimpse from the thumbnail. It works from the first 64 KB only, and returns `None` without a thumbnail.
6. **orientation_applied**: EXIF orientations 1–8 spliced in-test into one asymmetric JPEG. Each displays with the marked quadrant in the right corner and swapped dimensions for 5–8. No fixture files needed.
7. **never_upscales**: an 800×600 PNG at a 4K target stays 800×600.
8. **corrupt_input_errors**: a truncated JPEG and zero bytes → `Err`, no panic.
9. **prefetch_window_and_eviction**: a headless `egui::Context::default()`, 20 generated photos, `want(index 10)` in a loop until it settles → `Ready` for indices 9–13 and 8. Then `want(index 0)` → the 8–13 entries are evicted and 0–3 are ready.

`tests/viewer_ui.rs` (`egui_kittest` harness, headless): open a fixture folder → press ArrowRight twice → the label `3 / N` exists. Press End → `N / N`.
