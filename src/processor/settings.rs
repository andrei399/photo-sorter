use crate::error::{AppError, AppResult};
use crate::models::settings::{Action, KeyCombo, Keybindings, Settings};
use crate::models::store::Store;
use egui::Key;
use std::collections::HashMap;
use std::fs;
use std::path::Path;

/// Quit/close belong to the OS, and Cmd+, must always reach Settings so a bad binding can be undone.
pub const RESERVED_COMBOS: [KeyCombo; 4] = [
    KeyCombo::command(Key::Q),
    KeyCombo::command(Key::W),
    KeyCombo::alt(Key::F4),
    KeyCombo::command(Key::Comma),
];

/// Pure, so the Settings dialog can flag problems before Save; `update_settings` runs it again authoritatively.
pub fn validate_keybindings(keybindings: &Keybindings) -> AppResult<()> {
    let mut owner_by_combo = HashMap::new();
    for action in Action::ALL {
        let combos = keybindings.get(&action).map(Vec::as_slice).unwrap_or_default();
        if combos.is_empty() {
            return Err(AppError::Invalid(format!(
                "{} needs at least one shortcut",
                action.label()
            )));
        }
        for combo in combos {
            if RESERVED_COMBOS.contains(combo) {
                return Err(AppError::Invalid(format!("{combo} is reserved by the system")));
            }
            if let Some(owner) = owner_by_combo.insert(*combo, action)
                && owner != action
            {
                return Err(AppError::Invalid(format!(
                    "{combo} is bound to both {} and {}",
                    owner.label(),
                    action.label()
                )));
            }
        }
    }
    Ok(())
}

/// The favourites root only affects sessions created afterwards; existing sessions keep their folder.
pub fn update_settings(store: &mut Store, new_settings: Settings) -> AppResult<Settings> {
    validate_keybindings(&new_settings.keybindings)?;
    // Only probe a changed root: an unplugged drive must not block saving keybindings.
    if new_settings.favourites_root != store.state.settings.favourites_root {
        ensure_writable_dir(&new_settings.favourites_root)?;
    }
    store.state.settings = new_settings;
    store.save()?;
    Ok(store.state.settings.clone())
}

fn ensure_writable_dir(dir: &Path) -> AppResult<()> {
    if !dir.is_absolute() {
        return Err(AppError::Invalid(format!("{} is not a full path", dir.display())));
    }
    let unusable = |error: std::io::Error| AppError::Invalid(format!("Can't save into {}: {error}", dir.display()));
    fs::create_dir_all(dir).map_err(unusable)?;
    let probe = dir.join(".photo-sorter-write-test");
    fs::write(&probe, b"").map_err(unusable)?;
    fs::remove_file(&probe).map_err(unusable)?;
    Ok(())
}
