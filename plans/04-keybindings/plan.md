# 04 — Keybindings & Settings

Goal: every shortcut (favourite, next, previous, first, last, new session, open folder) can be rebound from a Settings dialog. Settings persist. The favourites root is changeable in the same dialog.

## Step 0 — Design gate

Mock 2 variants of the Settings dialog: (a) a single modal with a "Shortcuts" table plus a "Storage" section, (b) a larger window with a left nav. Publish, report, **stop until picked**. With 7 actions and 1 path, (a) is probably enough.

## Steps

### 1. Types (`models/settings.rs`)

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub enum Action { ToggleFavourite, Next, Previous, First, Last, NewSession, QuickSwitcher, OpenFolder }

/// `command` = Cmd on macOS, Ctrl elsewhere (egui's Modifiers::COMMAND), so one stored binding works on every OS.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct KeyCombo { pub command: bool, pub alt: bool, pub shift: bool, pub key: egui::Key }

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Settings {
    pub favourites_root: PathBuf,
    #[serde(default = "default_keybindings")]
    pub keybindings: BTreeMap<Action, Vec<KeyCombo>>,          // several combos per action allowed
}
```
- `egui::Key` serializes as a readable name (`"Enter"`, `"ArrowRight"`, `"N"`) via egui's `serde` feature, so `state.json` stays human-editable.
- Defaults: `ToggleFavourite: [Enter]`, `Next: [ArrowRight]`, `Previous: [ArrowLeft]`, `First: [Home]`, `Last: [End]`, `NewSession: [Cmd+N]`, `QuickSwitcher: [Cmd+K]`, `OpenFolder: [Cmd+O]`. egui reports NumpadEnter as `Enter`, so it's covered.
- Load-time merge: actions missing from a stored map (added in a later version) get their defaults. Unknown actions from a newer version are dropped by a custom deserializer, not treated as an error.

### 2. Validation (`processor/settings.rs`)

```rust
pub fn update_settings(store: &mut Store, new_settings: Settings) -> AppResult<Settings>
```
- Reject a combo bound to two actions → `Invalid("Cmd+N is already used by New session")`.
- Reject an action with zero combos, so the user can't lock themselves out of favouriting.
- Reject reserved combos: `Cmd+Q`, `Cmd+W`, `Alt+F4`, `Cmd+,` (always opens Settings).
- `favourites_root`: must be absolute. `create_dir_all`, then a writability probe (create and delete a temp file). It affects **new** sessions only, and existing sessions keep their `dir`.
- Save and return the normalized settings.

Core: `Command::UpdateSettings(Settings)`, `Command::ResetKeybindings` → `Event::SettingsChanged(Settings)` or `Event::Error`. The initial `Snapshot` already carries settings.

### 3. Matching (`ui/keybindings.rs`)

Replace phase 02's hard-coded table:
```rust
pub struct KeyMap { by_combo: HashMap<KeyCombo, Action> }      // rebuilt only on SettingsChanged

impl KeyMap {
    pub fn from_settings(settings: &Settings) -> Self
    /// Reads this frame's key events; uses `physical_key` when present so bindings survive layout switches.
    pub fn triggered_actions(&self, ctx: &egui::Context) -> Vec<Action>
}
pub fn combo_label(ctx: &egui::Context, combo: KeyCombo) -> String   // via ctx.format_shortcut → "⌘N" on mac, "Ctrl+N" elsewhere
```
- Iterate `ctx.input(|i| i.events)` for `Event::Key { pressed: true, physical_key, key, modifiers, .. }`. Build a `KeyCombo` from `physical_key.unwrap_or(key)` + `modifiers.command/alt/shift`, then do one hash lookup.
- Skip when `ctx.wants_keyboard_input()` or a modal is open.
- Dispatch in `app.rs` with an exhaustive `match action { … }`. Adding an `Action` without handling it is a compile error.
- `Cmd+,` → open Settings is checked before the map, and it's not rebindable.

### 4. Settings dialog (`ui/settings_dialog.rs`)

- Open it with a ⚙ button in the top bar and `Cmd+,`.
- **Shortcuts grid** (`egui::Grid`): Action | binding chips (each with ✕) | "+ Add". "+ Add" enters **capture mode** ("Press a key…"): the next `Event::Key` that isn't a lone modifier becomes the combo, and Esc cancels capture. Conflicts are shown inline right away (the UI runs the same pure check from `processor::settings::find_conflicts`, and Rust validation on save stays authoritative).
- "Reset to defaults".
- **Storage**: the favourites root path plus "Change…" (`rfd` pick_folder), with help text: "New sessions are created here. Tip: choose a folder inside Google Drive to upload automatically." (phase 05 Path A.)
- Save → `Command::UpdateSettings`. Errors are shown inline. Edits apply to a draft copy until saved.

### 5. On-screen hints

The ★ tooltip, the session menu items and the Open folder button show `combo_label` for their action, so they stay correct after rebinding.

### 6. Tests

`tests/settings.rs` (core flows):
1. **defaults_on_fresh_state**: the snapshot has all 7 actions with default combos.
2. **rebind_flow**: `UpdateSettings(ToggleFavourite → [F])` → `SettingsChanged` → `state_on_disk()` contains it → a new core on the same dir has it.
3. **conflict_rejected**, **empty_action_rejected**, **reserved_combo_rejected** → `Error`, disk unchanged.
4. **missing_action_filled**: a stored map without `OpenFolder` loads with the default added. **unknown_action_ignored**: `"Zoom"` in the file loads without error.
5. **favourites_root_change_affects_only_new_sessions**.
6. **unwritable_root_rejected** (Unix: chmod 555).

`tests/settings_ui.rs` (`egui_kittest`):
7. **rebound_key_favourites**: open a folder with an active session → rebind ToggleFavourite to F through the dialog (click "+ Add", press F, Save) → close → press F → the copy exists in the session dir. Press Enter → nothing happens (no longer bound).
8. **typing_does_not_trigger**: bind N to Next, open the New Session dialog, type "Nunta" → the field contains "Nunta" and the index is unchanged.
