# 01 — Review

## Plan critique

**What's right**
- **The actor owns the store.** Single writer, zero locks, operations serialized by construction. The UI thread only does `try_recv`, so it can never stall on disk. That's the foundation of "blazingly fast" feeling consistent rather than fast on average.
- **The command channel is the test seam.** Tests send the same `Command`s the UI does and assert `Event`s plus disk, with no window and no GPU. That's a holistic flow test without UI mocking.
- **egui redraws only on demand**, so the app idles at ~0 % CPU, which matters on laptops at events.
- **The wgpu → glow fallback** avoids the classic "black window on a Windows VM / old Intel GPU" failure.
- **`opt-level = 2` for dependencies in dev**: without it, decoding in debug builds is 10–30× slower and every dev session feels broken.

**Risks**
| Risk | Likelihood | Mitigation |
|---|---|---|
| egui minor releases break APIs | High (every ~3 months) | Pin the exact version. Upgrade deliberately in its own PR. Our UI surface is small (5 views). |
| An immediate-mode UI looks "programmer-ish" | Medium | Custom `Visuals`, a good font (bundle Inter via `include_bytes!`, ~300 KB), generous spacing. Validate with the design mocks in phases 02–05. |
| wgpu fails on exotic setups beyond what glow covers (remote desktop, no GPU at all) | Low | glow + Mesa llvmpipe works in software. `PHOTO_SORTER_RENDERER` env override for support. |
| Unbounded channels grow if the core is slow | Very low | Commands are user-paced (keypresses). Not worth bounding. |
| Wayland quirks (window decorations, drag-drop) | Medium on Linux | winit handles decorations. Drag-drop on Wayland is limited in winit (see phase 02). Test GNOME Wayland + KDE. |
| `panic = "abort"` in release loses state on a panic | Low | The store is saved after every mutation, so at most one in-flight operation is lost. Panics are bugs anyway, and the actor returns errors as Events. |

**Deliberately skipped**
- An async runtime in the core: everything there is local disk I/O that takes milliseconds, so plain threads are simpler. Tokio arrives in phase 05, only inside the Drive integration.
- A logging framework beyond `env_logger`, i18n, a theming system.

## Code review checklist

- [ ] `ui/` never imports `models::Store` or `std::fs`.
- [ ] `core/` handlers are one-line delegations to `processor`.
- [ ] `Store::save` writes tmp → `sync_all()` → `rename`. No direct `fs::write` to `state.json`.
- [ ] Corrupt state is quarantined, not overwritten. Every new field has `#[serde(default)]`.
- [ ] The UI drains events with `try_recv` only, never blocking `recv`.
- [ ] The core calls `request_repaint` after every event.
- [ ] No `unwrap()`/`expect()` outside `main.rs` and tests.
- [ ] CI covers fmt, clippy `-D warnings`, tests and a release build on 3 OSes.
