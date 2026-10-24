# Photo Sorter — Master Plan

Point the app at a folder of photos, browse them full-screen like Windows Photos, press **Enter** (or the on-screen ★) to favourite. Favourites are copied into the folder of the **active favourite session** (one session per event: "Wedding Ana & Mihai", "Bucharest Marathon"…). Each session folder is a plain directory, so it can be dragged into Google Drive, or synced to Drive automatically.

**100 % Rust, native UI, built to be blazingly fast.** Targets: **Windows 10/11, macOS 13+, Linux (X11 + Wayland; Ubuntu 22.04+, Fedora 39+)**.

## Phases

Each phase folder has four files: `plan.md` (step-by-step build instructions), `review.md` (a critique of the plan: risks, trade-offs, what to watch in code review), `qa.md` (automated and manual test plan), `acceptance.md` (Given/When/Then criteria a phase must pass before it's done).

| # | Folder | Outcome | Depends on |
|---|--------|---------|-----------|
| 01 | [01-foundation](01-foundation/) | eframe app skeleton, layered crate, core actor, persisted store, CI on all 3 OSes | — |
| 02 | [02-photo-viewer](02-photo-viewer/) | Imaging pipeline (scaled decoding, parallel prefetch, GPU textures), full-screen viewer, ←/→ | 01 |
| 03 | [03-favourite-sessions](03-favourite-sessions/) | Sessions (Ctrl/Cmd+N), favourite toggle (Enter/★), copy into the session folder, open the folder in the OS | 02 |
| 04 | [04-keybindings](04-keybindings/) | Rebindable shortcuts, settings dialog | 03 |
| 05 | [05-google-drive-sync](05-google-drive-sync/) | Automatic Drive upload into a folder per session (zero-code Drive for desktop path, plus native API sync) | 03 |
| 06 | [06-raw-and-pairs](06-raw-and-pairs/) | RAW preview (CR2/CR3/NEF/ARW/RAF/DNG…), RAW+JPEG pairs favourited together | 02, 03 |
| 07 | [07-release](07-release/) | Signed installers for all 3 OSes, auto-update | all |

MVP = 01 → 02 → 03, which is usable at a real event. 04 and 05 can run in parallel after 03 (file ownership below). 06 and 07 come last.

## Why native, and where the speed comes from

Drawing UI is cheap in a photo viewer; the expensive part is **decoding 24–50 MP JPEGs**. Owning the decode pipeline is the whole point of going native:

1. **Decode only the pixels the screen can show.** libjpeg-turbo decodes a JPEG directly at a reduced DCT scale (M/8) and straight into the GPU texture buffer. Huffman decoding still costs time in proportion to file size, so a big file takes ~100–260 ms on a fast CPU (measured in the phase 02 spike). That's 1.5× faster than a pure-Rust full decode + resize, with about 4× less memory.
2. **Parallel prefetch.** A thread pool decodes the photos around the current one (+1, −1, +2, +3, −2) in priority order and uploads them as GPU textures before they're needed. Pressing → swaps a texture handle within the next frame.
3. **An instant glimpse for cold jumps.** Every camera JPEG embeds a ~160×120 EXIF thumbnail inside its first 64 KB. It decodes in ~0.1 ms, so on a cache miss (Home/End, a fast held arrow) the photo's composition is on screen at once, and the sharp decode replaces it. Glimpses are prefetched 12 photos ahead. You never see a blank screen.
4. **No web runtime.** Cold start ~100–200 ms, ~50–150 MB of memory, identical behaviour on all three OSes (no WebKitGTK slowness on Linux).

## Architecture decisions

| Decision | Choice | Why |
|---|---|---|
| UI | **egui via eframe**, wgpu renderer (Metal/DX12/Vulkan) with a glow (OpenGL) fallback | Immediate mode suits a viewer with a handful of controls: no widget tree, no state sync, GPU-rendered. AccessKit gives screen-reader support. iced (more code) and Slint (a separate markup language) were rejected for this scope. |
| Concurrency model | **Core actor**: one thread owns the `Store` and processes `Command`s → emits `Event`s | Single writer, no locks, ops serialized for free (no copy vs. trash races). The UI thread never blocks on disk. The command channel is also the test seam. |
| JPEG decode | **libjpeg-turbo** (`turbojpeg` crate, vendored build) with DCT scaling | Fastest JPEG decoder available, SIMD, scaled decode. A phase 02 spike confirms it against pure-Rust `zune-jpeg`. |
| Other formats | `image` crate (PNG, WebP, GIF first frame, BMP, TIFF) + `fast_image_resize` | Cameras shoot JPEG/RAW; the rest is just a correct fallback. AVIF/HEIC are skipped (C deps, rarely shot). |
| Font | **Inter** Regular + SemiBold bundled (OFL, ~830 KB) | egui's built-in font lacks ★ ☆ ⌘. Inter has them, plus Romanian ș/ț, and matches the mocks. Glyphs missing from Inter (e.g. CJK filenames) draw as boxes and never crash. |
| EXIF orientation | `kamadak-exif`, rotate the downscaled buffer on the worker thread | Costs a few ms at screen size. |
| File dialogs / open folder | `rfd` (native dialogs, XDG portal on Linux), `opener` (reveal in Explorer/Finder/Files) | Small, native. |
| Favourite = | **Copy** source → session folder (atomic: `.partial` then rename) | The session folder is self-contained and safe to upload, move or delete. Hard links break across drives and alias edits. |
| Un-favourite | Move our copy to the **OS trash** (`trash` crate) | Never hard-delete user files. We only touch the file we created, which is recorded in state. |
| Persistence | One `state.json` in the OS data dir (`directories` crate), atomic write | No DB. Nothing is written into photo folders, so nothing leaks into Drive uploads. |
| Drive auto-upload | Path A: favourites root inside the **Google Drive for desktop** folder (zero code). Path B: native **Drive API v3** with the `drive.file` scope on a tokio runtime thread | Path A works today on Win/Mac. Path B covers Linux and users without the Drive client. |
| Packaging | **cargo-packager** + `cargo-packager-updater` | Builds NSIS/MSI, DMG and .app, deb and AppImage, with signed updates, from a plain Rust crate. |
| Privacy | No servers of ours. OAuth tokens live in the OS keychain. Uploads go straight from the user's machine to Google. | Matches the privacy-first stance of Nodus Veritatis. |

`gen_fe_types` doesn't apply here: there's no separate frontend, and types are shared at compile time.

## Target repo layout (end state)

```
photo-sorter/
├── Cargo.toml                   # one crate: lib (all logic) + bin (main.rs)
├── src/
│   ├── main.rs                  # eframe bootstrap, renderer fallback (thin)
│   ├── lib.rs                   # module tree, re-exports for tests
│   ├── app.rs                   # PhotoSorterApp: eframe::App, owns UI state, drains core/imaging events
│   ├── error.rs                 # AppError
│   ├── ui/                      # VIEW layer: viewer, top_bar, new_session_dialog, settings_dialog, drive_panel
│   ├── core/                    # ENDPOINT layer: Command / Event enums + actor loop → processor
│   ├── processor/               # LOGIC layer: photo_folder, sessions, favourites, settings, drive_sync
│   ├── imaging/                 # PIXELS: decode (turbojpeg/image/exif), prefetch (pool + texture cache)
│   ├── models/                  # PERSISTENCE layer: state, session, settings, store
│   └── integrations/            # EXTERNAL layer: Google OAuth + Drive REST client
└── tests/                       # whole flows: drive the core actor; egui_kittest for key-press → disk flows
```

Data flow:
```
 keyboard/mouse ─▶ ui (egui, UI thread) ──Command──▶ core actor (owns Store) ──▶ processor ──▶ disk / state.json
        ▲                 │                                │
        │                 │ want(index±n)                  └──Event──▶ app.rs (drained every frame)
        │                 ▼
        └── TextureHandle ◀── imaging pool (rayon) ◀── file bytes
```

## Parallel-work file ownership (04 ∥ 05)

- **04-keybindings** owns `src/models/settings.rs`, `src/processor/settings.rs`, `src/ui/settings_dialog.rs`, `src/ui/keybindings.rs`.
- **05-google-drive-sync** owns `src/integrations/**`, `src/processor/drive_sync.rs`, `src/ui/drive_panel.rs`.
- Shared touch points (`core/mod.rs` Command/Event enums, `models/state.rs`) are merged by whoever lands second, after a rebase.

## Bold ideas worth considering (not scheduled)

1. **Cull stats**: "312 viewed / 48 favourited / 15 % keep rate" per session. ~20 lines, and photographers love it.
2. **Focus-check zoom**: hold Space to show the photo at 100 % under the cursor. Natively, that's one extra full-res decode of a crop region. The most likely next request from photographers.
3. **Share link on finish**: after Drive sync completes, one click copies the Drive folder share link to the clipboard for the client (one extra `permissions.create` call in phase 05).

## Dev environment note (WSL2)

The Linux build runs in WSL2 through WSLg, but GPU acceleration there is limited, so **performance numbers from WSLg are meaningless**. Measure on native Windows, macOS and Linux. Windows and macOS binaries are built by CI (phase 01 sets up the matrix) or on the native host.
