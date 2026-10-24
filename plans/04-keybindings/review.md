# 04 — Review

## Plan critique

**What's right**
- **Physical keys.** A Romanian or AZERTY user who binds F keeps the same physical key across layout switches.
- **The `command` modifier** is egui's native cross-platform Cmd/Ctrl, so one stored binding is correct on Mac and PC.
- **An exhaustive `match` on `Action`**: forgetting to wire a new action doesn't compile.
- **One hash lookup per key event**, rebuilt only when settings change.
- **The conflict check is a pure function shared by UI and processor**: instant feedback and authoritative validation without duplicated logic.
- **Native means no webview stealing keys.** Ctrl+N, Ctrl+P and F5 are all ours.

**Risks**
| Risk | Mitigation |
|---|---|
| `physical_key` isn't reported on every platform or backend (it's `None` → falls back to the logical key) | Fallback built in. QA checks a layout switch on each OS. |
| Labels for physical keys show QWERTY names on other layouts (stored `Q` shows "Q" though AZERTY prints "A") | Cosmetic. Accepted. |
| Binding plain letters triggers actions while typing | Handled by `wants_keyboard_input()` plus modal suspension. Covered by a kittest. |
| macOS menu bar intercepting Cmd+Q / Cmd+W | Both are reserved, so the user can't bind them. |
| `egui::Key` serde names change between egui versions | Tiny risk. The `missing_action_filled` and load tests catch it on upgrade. |

**Deliberately skipped**
- Global (system-wide) hotkeys, chord sequences, import/export, per-session bindings, a generic settings framework (1 path + 1 map: the struct *is* the framework).

## Code review checklist

- [ ] Matching uses `physical_key.unwrap_or(key)` and `modifiers.command`, not `ctrl`.
- [ ] Conflict, empty and reserved checks run in `processor` on save. The UI reuses the pure check.
- [ ] Missing actions are filled on load. Unknown actions are ignored.
- [ ] `KeyMap` is rebuilt only on `SettingsChanged`.
- [ ] Capture mode ignores lone modifiers. Esc cancels.
- [ ] `Cmd+,` always opens Settings and can't be overridden.
- [ ] Dialog edits apply to a draft until saved.
- [ ] Changing `favourites_root` never alters existing sessions.
