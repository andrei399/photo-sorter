use crate::error::{AppError, AppResult};
use crate::models::settings::{Action, KeyCombo, Keybindings, Settings};
use crate::models::store::Store;
use egui::Key;
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

/// Quit/close belong to the OS, and Cmd+, must always reach Settings so a bad binding can be undone.
pub const RESERVED_COMBOS: [KeyCombo; 4] = [
    KeyCombo::command(Key::Q),
    KeyCombo::command(Key::W),
    KeyCombo::alt(Key::F4),
    KeyCombo::command(Key::Comma),
];

#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum KeybindingProblem {
    #[error("{} needs at least one shortcut", .0.label())]
    Unbound(Action),
    #[error("{combo} is reserved by the system")]
    Reserved { action: Action, combo: KeyCombo },
    #[error("{combo} is bound to both {} and {}", .first.label(), .second.label())]
    Conflict {
        combo: KeyCombo,
        first: Action,
        second: Action,
    },
}

impl KeybindingProblem {
    /// The row the Settings dialog explains the problem under.
    pub fn action(self) -> Action {
        match self {
            Self::Unbound(action) | Self::Reserved { action, .. } | Self::Conflict { second: action, .. } => action,
        }
    }

    /// The key chip(s) to highlight.
    pub fn combo(self) -> Option<KeyCombo> {
        match self {
            Self::Unbound(_) => None,
            Self::Reserved { combo, .. } | Self::Conflict { combo, .. } => Some(combo),
        }
    }
}

/// Pure, so the Settings dialog can flag problems before Save; `update_settings` runs it again authoritatively.
pub fn find_keybinding_problem(keybindings: &Keybindings) -> Option<KeybindingProblem> {
    let mut owner_by_combo = HashMap::new();
    for action in Action::ALL {
        let combos = keybindings.get(&action).map(Vec::as_slice).unwrap_or_default();
        if combos.is_empty() {
            return Some(KeybindingProblem::Unbound(action));
        }
        for &combo in combos {
            if RESERVED_COMBOS.contains(&combo) {
                return Some(KeybindingProblem::Reserved { action, combo });
            }
            if let Some(first) = owner_by_combo.insert(combo, action)
                && first != action
            {
                return Some(KeybindingProblem::Conflict {
                    combo,
                    first,
                    second: action,
                });
            }
        }
    }
    None
}

/// The favourites root only affects sessions created afterwards; existing sessions keep their folder.
pub fn update_settings(store: &mut Store, new_settings: Settings) -> AppResult<Settings> {
    if let Some(problem) = find_keybinding_problem(&new_settings.keybindings) {
        return Err(AppError::Invalid(problem.to_string()));
    }
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

/// Google Drive for desktop exposes "My Drive" as a local folder; anything saved under it uploads.
/// Path probing only, no API. Returns the suggested favourites root inside it.
// ponytail: probes on the core thread; a dead mapped network drive on Windows can stall it for seconds. Move to a thread if reported.
pub fn detect_google_drive_favourites_root() -> Option<PathBuf> {
    google_drive_candidates()
        .into_iter()
        .find(|my_drive| my_drive.is_dir())
        .map(|my_drive| my_drive.join("Photo Sorter"))
}

#[cfg(windows)]
fn google_drive_candidates() -> Vec<PathBuf> {
    let legacy = directories::UserDirs::new().map(|dirs| dirs.home_dir().join("Google Drive"));
    // G: is the default letter, so it goes first.
    std::iter::once('G')
        .chain(('D'..='Z').filter(|letter| *letter != 'G'))
        .map(|letter| PathBuf::from(format!("{letter}:\\My Drive")))
        .chain(legacy)
        .collect()
}

#[cfg(target_os = "macos")]
fn google_drive_candidates() -> Vec<PathBuf> {
    let Some(cloud_storage) = directories::UserDirs::new().map(|dirs| dirs.home_dir().join("Library/CloudStorage"))
    else {
        return Vec::new();
    };
    fs::read_dir(cloud_storage)
        .into_iter()
        .flatten()
        .filter_map(Result::ok)
        .filter(|entry| entry.file_name().to_string_lossy().starts_with("GoogleDrive-"))
        .map(|entry| entry.path().join("My Drive"))
        .collect()
}

/// Linux has no Drive desktop client; phase 05's native sync covers it.
#[cfg(not(any(windows, target_os = "macos")))]
fn google_drive_candidates() -> Vec<PathBuf> {
    Vec::new()
}
