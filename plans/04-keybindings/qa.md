# 04 — QA

## Automated

| Test | Asserts |
|---|---|
| `defaults_on_fresh_state` | 7 actions, default combos. |
| `rebind_flow` | Persisted on disk, survives a new core. |
| `conflict_rejected` / `empty_action_rejected` / `reserved_combo_rejected` | `Error`, disk unchanged. |
| `missing_action_filled` / `unknown_action_ignored` | Forward and backward compatible loading. |
| `favourites_root_change_affects_only_new_sessions` | Old dirs untouched. |
| `unwritable_root_rejected` | Error, setting unchanged. |
| `rebound_key_favourites` (kittest) | A rebind through the dialog → the new key copies the file, and the old key does nothing. |
| `typing_does_not_trigger` (kittest) | Text entry is safe with letter bindings. |

## Manual (each OS)

| # | Step | Expected |
|---|---|---|
| 1 | Open Settings via ⚙ and via Ctrl+, / Cmd+, | Dialog shows bindings with platform labels (⌘N on mac, Ctrl+N elsewhere). |
| 2 | Add F to Toggle favourite, Save, press F | Toggles. Enter still works. |
| 3 | Remove Enter from Toggle favourite, Save | Only F works. The ★ tooltip shows "F". |
| 4 | Try binding → to Toggle favourite | Inline conflict naming "Next". Save disabled. |
| 5 | Remove all bindings from Next | Error: needs at least one. |
| 6 | Try capturing Cmd+Q / Ctrl+Q | Rejected as reserved. |
| 7 | Capture: press Shift alone, then Shift+D | Lone Shift ignored. `Shift+D` captured. |
| 8 | Esc during capture | Cancels capture only. |
| 9 | Change bindings, close without saving, reopen | The draft was discarded. |
| 10 | Reset to defaults, Save | Defaults restored. |
| 11 | Bind N to Next, open New Session, type "Nunta" | Text typed. No navigation. |
| 12 | Switch the OS layout to French AZERTY, press the physical key bound earlier | Same action fires. |
| 13 | Change the favourites root to another drive, create a session | New session under the new root. Old sessions unaffected. |
| 14 | Restart | Custom bindings and root persist. |
| 15 | NumpadEnter | Toggles favourite by default. |
