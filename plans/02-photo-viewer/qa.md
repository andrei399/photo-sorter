# 02 — QA

## Fixtures

Keep a `qa-photos/` folder outside the repo (never commit real photos):
- 300 real camera JPEGs at 24 MP and 100 at 45 MP+, including **portrait shots with EXIF orientation 6/8**
- 1 PNG, 1 WebP, 1 GIF, 1 BMP, 1 TIFF
- a zero-byte `broken.jpg`, a truncated JPEG (first 50 KB), and a `.txt` renamed to `.jpg`
- names `IMG_2.jpg`, `IMG_10.jpg`, `ünïcødé ñame.jpg`, `space name.JPG`, a folder named `Party #1 100%`
- a 5,000-file folder (`for i in $(seq 1 5000); do cp a.jpg img_$i.jpg; done`)
- the same 300 JPEGs on a USB 3 stick and an SD card (slow storage)

## Automated

| Test | Asserts |
|---|---|
| `open_folder_filters_and_sorts` | Natural sort, case-insensitive ext, skips txt, hidden and `._*` files and subfolders. |
| `last_folder_persisted_and_reopened` | Persisted on disk. A new core auto-emits `FolderOpened`. |
| `missing_folder_errors` / `empty_folder_ok` | As named. |
| `jpeg_scaled_to_fit` / `never_upscales` / `non_jpeg_resized_to_fit` | Output sizes. |
| `glimpse_uses_embedded_thumbnail` / `glimpse_from_file_head` | Thumbnail used, oriented, works on 64 KB. |
| `orientation_applied` | All 8 EXIF orientations come out upright. |
| `corrupt_input_errors` | `Err`, no panic. |
| `prefetch_window_and_eviction` | Window filled in priority order, old entries evicted. |
| `viewer_ui` (kittest) | Keyboard navigation updates the counter. Stops at the ends. |

## Benchmarks (record in the PR, native hardware per OS)

`cargo run --release --example decode_bench -- <folder>` at a 2560×1440 target:

| Metric | Target |
|---|---|
| 24 MP out-of-camera JPEG (~10 MB) full decode, median | ≤ 130 ms (spike: ~126 ms on 8 MB synthetic) |
| 45–60 MP JPEG (~25 MB) full decode, median | ≤ 300 ms |
| Glimpse (EXIF thumbnail) | ≤ 1 ms |
| Prefetched → displayed | next frame (≤ 16 ms at 60 Hz) |
| Held → on 24 MP, frames with a blank screen | 0 |
| Peak RSS browsing 300 photos at 1440p | ≤ 250 MB |
| Cold start to first photo (last folder auto-reopened) | ≤ 400 ms |

## Manual (each OS, native)

| # | Step | Expected |
|---|---|---|
| 1 | Open folder → `qa-photos` | First photo in < 400 ms, overlay `1 / N`. |
| 2 | Press → ×10, then ← ×10 | Each photo appears instantly. No flash or blank frame. |
| 3 | Hold → for 10 s (24 MP), then (45 MP) | A continuous stream of photos. Soft glimpses while moving fast are expected, sharpening on release. Never blank, never freezes. |
| 4 | Home / End | Glimpse appears immediately, sharp within ~130 ms (24 MP) / ~300 ms (60 MP). |
| 5 | Portrait EXIF photos | Upright. |
| 6 | ← on the first, → on the last | Edge shake, no wrap. |
| 7 | `broken.jpg`, truncated JPEG, renamed txt | "Can't display" message. Arrows still work. |
| 8 | `Party #1 100%` and unicode names | Load fine. |
| 9 | 5,000-file folder | Opens in < 1 s. Navigation unaffected. |
| 10 | USB stick / SD card folder | Sequential browsing stays smooth after the first 2 photos (prefetch). |
| 11 | Drag a folder onto the window (Win/mac/X11) | Opens. Dropping a file opens its folder at that file. |
| 12 | Resize the window from small to maximized, move it to a 4K monitor | The photo re-sharpens within ~200 ms after resizing stops. |
| 13 | Restart | The last folder reopens. Unplug or rename it first → empty screen, no error. |
| 14 | Task manager while holding → through 300 photos | Memory plateaus. All cores busy while holding, ~0 % when idle. |
| 15 | HiDPI 200 % / Retina | Photo is sharp (decoded at physical pixels, not logical). |
| 16 | `decode_bench` on the release build | Within ~10 % of the spike numbers in plan.md. |
