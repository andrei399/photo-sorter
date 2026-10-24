# 01 — QA

## Automated (every push, all 3 OSes)

| Test | File | Asserts |
|---|---|---|
| `fresh_install_emits_default_snapshot` | `tests/core_state.rs` | The first event is `Snapshot(default)`. No state file is created until a mutation happens. |
| `corrupt_state_is_quarantined` | `tests/core_state.rs` | Write `{garbage` to `state.json` → spawn core → `Snapshot(default)` → `state.json.corrupt-*` exists with the original bytes. |
| `save_is_atomic` | `tests/core_state.rs` | Mutate (via a test-only command or phase 02's `OpenFolder`) → no `state.json.tmp` left behind → a new core from the same dir reports the same state. |
| `unknown_fields_ignored` | `tests/core_state.rs` | A state file with `{"future_field": 1}` loads. |
| `core_survives_bad_commands` | `tests/core_state.rs` | A command that errors yields `Event::Error`, and the next command still works. |
| fmt / clippy `-D warnings` | CI | Clean. |

## Manual smoke (each OS, native, not WSLg)

| # | Step | Expected |
|---|---|---|
| 1 | `cargo run --release` | The window appears in < 300 ms, titled "Photo Sorter", dark, with the "Open a folder to start" button. |
| 2 | Leave it idle for 1 minute, watch the task manager | ~0 % CPU. |
| 3 | Resize below 800×500 | It stops at the minimum. |
| 4 | Move and resize the window, quit, relaunch | Same position and size. |
| 5 | `PHOTO_SORTER_RENDERER=glow cargo run` | Renders the same. |
| 6 | Windows VM without GPU drivers | Starts (glow fallback logged). |
| 7 | Linux: GNOME Wayland, KDE Wayland, an X11 session | Opens with correct decorations and HiDPI scaling (test at 150 %/200 %). |
| 8 | macOS Retina | Text is crisp (pixels_per_point = 2). |
| 9 | Screen reader (Narrator / VoiceOver / Orca) on the button | Reads "Open folder". |

Data-dir locations to verify:
- Windows: `%APPDATA%\Nodus Veritatis\Photo Sorter\data\`
- macOS: `~/Library/Application Support/com.Nodus-Veritatis.Photo-Sorter/`
- Linux: `~/.local/share/photosorter/`
