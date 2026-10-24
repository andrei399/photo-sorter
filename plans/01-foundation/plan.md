# 01 — Foundation

Goal: a running native egui window, the layered crate, the core actor with a persisted state store, and CI that builds and tests on Windows, macOS and Linux. No features yet. Every later phase starts by adding a `Command`, a processor function and a UI piece, never by fixing plumbing.

## Prerequisites

- Rust stable (≥ 1.85, edition 2024).
- `cmake` and `nasm` on every build machine (libjpeg-turbo is vendored in phase 02, and nasm enables its SIMD paths). Install them now so CI is ready.
- Linux (Ubuntu/WSL):
  ```bash
  sudo apt install build-essential cmake nasm pkg-config \
    libxkbcommon-dev libwayland-dev libxcb-render0-dev libxcb-shape0-dev libxcb-xfixes0-dev
  ```
- macOS: Xcode Command Line Tools, `brew install cmake nasm`.
- Windows: MSVC Build Tools, `winget install Kitware.CMake NASM.NASM`.

## Steps

### 1. Crate

```bash
cd ~/photo-sorter
cargo init --name photo_sorter
git init && git add -A && git commit -m "chore: init crate"
```
One crate with both `src/lib.rs` (everything) and `src/main.rs` (≤ 30 lines), so integration tests in `tests/` can use the library.

### 2. Dependencies (`Cargo.toml`)

```toml
[dependencies]
eframe = { version = "0.33", default-features = false, features = ["wgpu", "glow", "accesskit", "default_fonts", "persistence"] }
egui = { version = "0.33", features = ["serde"] }
serde = { version = "1", features = ["derive"] }
serde_json = "1"
thiserror = "2"
uuid = { version = "1", features = ["v4", "serde"] }
directories = "6"
crossbeam-channel = "0.5"
log = "0.4"
env_logger = "0.11"

[dev-dependencies]
tempfile = "3"

[profile.release]
lto = "thin"
codegen-units = 1
panic = "abort"     # smaller and faster; the actor surfaces errors as Events, so nothing relies on unwinding

[profile.dev.package."*"]
opt-level = 2       # decoders are unusably slow unoptimized; keep our own code debuggable
```
Pin the exact eframe/egui version that's current when you start, and bump deliberately (egui has breaking changes every minor version).

Why each eframe feature:
- `wgpu` is the primary renderer: Metal on macOS, DX12/Vulkan elsewhere.
- `glow` is the OpenGL fallback for VMs and old GPUs (step 7).
- `accesskit` provides screen-reader support.
- `persistence` saves window size and position only. App state is ours.

### 3. Layered module tree

```
src/
  main.rs         # parse env, init logger, run eframe (renderer fallback)
  lib.rs          # pub mod app, core, error, imaging, models, processor, ui, integrations
  app.rs          # PhotoSorterApp (UI state) + eframe::App impl
  error.rs
  core/mod.rs     # ENDPOINT layer: Command, Event, spawn_core()
  processor/mod.rs# LOGIC layer
  models/mod.rs   # PERSISTENCE layer: state.rs, store.rs
  ui/mod.rs       # VIEW layer
  imaging/mod.rs  # (phase 02)
  integrations/mod.rs  # (phase 05)
```
Rules for reviewers:
- `ui/` never touches the filesystem or `models::Store`. It reads `PhotoSorterApp` fields and sends `Command`s.
- `core/` only matches a `Command` → calls `processor` → sends an `Event`. No business branching.
- `processor/` holds all decisions.

### 4. Error type (`error.rs`)

```rust
#[derive(Debug, Clone, thiserror::Error)]
pub enum AppError {
    #[error("{0}")]               Io(String),
    #[error("not found: {0}")]    NotFound(String),
    #[error("{0}")]               Invalid(String),
}
impl From<std::io::Error> for AppError { fn from(e: std::io::Error) -> Self { Self::Io(e.to_string()) } }
pub type AppResult<T> = Result<T, AppError>;
```
`Clone` so errors can ride inside `Event`s. Messages are written for humans because they're shown in toasts.

### 5. State store (`models/`)

`models/state.rs`: the root persisted document. Start minimal. Later phases add fields, each with `#[serde(default)]` so old files still load.
```rust
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct PersistedState {
    #[serde(default)] pub schema_version: u32,
    #[serde(default)] pub last_opened_folder: Option<PathBuf>,
}
```

`models/store.rs`:
```rust
pub struct Store { path: PathBuf, pub state: PersistedState }

impl Store {
    /// Missing file → default. Corrupt file → renamed to state.json.corrupt-<unix ts>, start default, log a warning.
    pub fn load_or_default(path: PathBuf) -> AppResult<Self>
    /// serialize → write state.json.tmp → sync_all → rename over state.json
    pub fn save(&self) -> AppResult<()>
}
```
- Location: `directories::ProjectDirs::from("com", "Nodus Veritatis", "Photo Sorter").data_dir().join("state.json")`. Create the dir if it's missing.
- Plain `std::fs`. The store lives only on the core thread, so there's no async and no mutex.

### 6. Core actor (`core/mod.rs`)

```rust
pub enum Command { GetSnapshot, Shutdown }                 // grows each phase
pub enum Event   { Snapshot(PersistedState), Error(AppError) }

pub struct CoreHandle { pub commands: Sender<Command>, pub events: Receiver<Event> }

/// `on_event` wakes the UI (eframe passes ctx.request_repaint); tests pass a no-op.
pub fn spawn_core(state_path: PathBuf, on_event: impl Fn() + Send + 'static) -> AppResult<CoreHandle>
```
- `crossbeam-channel` unbounded both ways. One named thread `"core"` owns the `Store`.
- Loop: `for cmd in commands { let event = handle(&mut store, cmd); events.send(event); on_event(); }`.
- `handle` is a `match` that delegates to `processor`. Every processor error becomes `Event::Error`. The actor never panics on user input.
- On start, emit `Event::Snapshot` so the UI renders from persisted state.

### 7. App + bootstrap

`app.rs`:
```rust
pub struct PhotoSorterApp { core: CoreHandle, state: Option<PersistedState>, toasts: Vec<Toast> }

impl eframe::App for PhotoSorterApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        while let Ok(event) = self.core.events.try_recv() { self.apply(event); }   // drain, never block
        ui::draw(ctx, self);
    }
}
```

`main.rs`:
```rust
fn main() -> eframe::Result {
    env_logger::init();
    let run = |renderer| eframe::run_native("Photo Sorter", native_options(renderer),
        Box::new(|cc| Ok(Box::new(PhotoSorterApp::new(cc)?))));
    // wgpu first; some VMs/old GPUs lack DX12/Vulkan/Metal → retry once with OpenGL.
    run(eframe::Renderer::Wgpu).or_else(|e| { log::warn!("wgpu failed: {e}; falling back to glow"); run(eframe::Renderer::Glow) })
}
```
`native_options`: title "Photo Sorter", inner size 1280×800, min 800×500, app icon (placeholder PNG via `include_bytes!`), `persist_window: true`. Allow forcing a renderer with `PHOTO_SORTER_RENDERER=glow` for support cases.

`PhotoSorterApp::new` spawns the core with `cc.egui_ctx.clone()` captured in `on_event` (`ctx.request_repaint()`). egui only redraws on input or a request, so the app uses **~0 % CPU when idle**.

Theme: `ctx.set_visuals(egui::Visuals::dark())`, panel fill `#111`. The central panel shows "Open a folder to start" plus an **Open folder** button, a no-op until phase 02.

### 8. Tests

`tests/common/mod.rs`:
```rust
pub struct TestCore { pub handle: CoreHandle, pub dir: TempDir }
pub fn test_core() -> TestCore                      // spawns core with state at tempdir/state.json
impl TestCore {
    pub fn send(&self, cmd: Command)
    pub fn expect_event(&self) -> Event              // recv_timeout(5 s), panics with a clear message on timeout
    pub fn state_on_disk(&self) -> serde_json::Value // read tempdir/state.json fresh from disk
}
```
`tests/core_state.rs`: see qa.md. Every later phase reuses `test_core()`.

### 9. CI (`.github/workflows/ci.yml`)

Matrix `ubuntu-22.04`, `macos-latest`, `windows-latest`:
1. checkout, `dtolnay/rust-toolchain@stable` (with clippy), `Swatinem/rust-cache@v2`
2. install cmake + nasm (apt / brew / choco), plus the Linux deps from Prerequisites
3. `cargo fmt --check`
4. `cargo clippy --all-targets -- -D warnings`
5. `cargo test`
6. `cargo build --release` to prove it links on each OS

## Done when

All items in `acceptance.md` pass and CI is green on all three OSes.
