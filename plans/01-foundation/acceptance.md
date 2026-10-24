# 01 — Acceptance Criteria

- [ ] **AC1 — Runs everywhere**: Given a fresh clone with prerequisites on Windows, macOS or Linux (X11 or Wayland), when I run `cargo run --release`, then the native Photo Sorter window opens with the empty start screen in under 300 ms.
- [ ] **AC2 — Idle is free**: Given the app is open and untouched, CPU usage is ~0 %.
- [ ] **AC3 — Graphics fallback**: Given a machine where wgpu can't initialize, the app falls back to OpenGL and still opens.
- [ ] **AC4 — State persists safely**: Given a mutation, state is written atomically. A kill mid-save leaves either the old or the new state, never a partial file.
- [ ] **AC5 — Corruption is non-fatal**: Given invalid `state.json`, the app starts with defaults and keeps the bad file as `state.json.corrupt-<timestamp>`.
- [ ] **AC6 — Layering**: `ui/`, `core/`, `processor/`, `models/`, `imaging/` and `integrations/` exist. The UI never touches disk, and the core only delegates.
- [ ] **AC7 — Testable without a window**: Flow tests drive the core actor headlessly and pass in CI on all three OSes.
- [ ] **AC8 — CI**: fmt, clippy (`-D warnings`), tests and a release build are green on all three OSes.
