use crate::error::{AppError, AppResult};
use crate::models::session::{Session, SessionId};
use crate::models::store::Store;
use std::collections::{BTreeMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};

const MAX_FOLDER_NAME_CHARS: usize = 100;
const WINDOWS_RESERVED_NAMES: &[&str] = &[
    "CON", "PRN", "AUX", "NUL", "COM1", "COM2", "COM3", "COM4", "COM5", "COM6", "COM7", "COM8", "COM9", "LPT1", "LPT2",
    "LPT3", "LPT4", "LPT5", "LPT6", "LPT7", "LPT8", "LPT9",
];

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SessionSummary {
    pub id: SessionId,
    pub name: String,
    pub dir: PathBuf,
    pub favourite_count: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SessionDirInfo {
    pub exists: bool,
    pub file_count: usize,
}

/// Pure (no I/O) so the New Session dialog can preview the folder name live.
/// Produces a name that is legal on Windows, macOS and Linux alike.
pub fn session_name_to_folder_name(name: &str) -> AppResult<String> {
    let replaced: String = name
        .trim()
        .chars()
        .map(|c| {
            if c.is_control() || r#"<>:"/\|?*"#.contains(c) {
                '_'
            } else {
                c
            }
        })
        .take(MAX_FOLDER_NAME_CHARS)
        .collect();
    // Windows silently drops trailing dots and spaces, which would make two sessions share a folder.
    let folder_name = replaced.trim_end_matches(['.', ' ']).to_owned();
    if folder_name.is_empty() {
        return Err(AppError::Invalid("Session name can't be empty".into()));
    }
    let stem = folder_name.split('.').next().unwrap_or_default();
    if WINDOWS_RESERVED_NAMES
        .iter()
        .any(|reserved| reserved.eq_ignore_ascii_case(stem.trim_end()))
    {
        return Err(AppError::Invalid(format!(
            "\"{folder_name}\" is a reserved name on Windows"
        )));
    }
    Ok(folder_name)
}

pub fn create_session(store: &mut Store, name: &str, dir: Option<PathBuf>) -> AppResult<SessionId> {
    let name = name.trim().to_owned();
    let folder_name = session_name_to_folder_name(&name)?;
    let dir = dir.unwrap_or_else(|| store.state.settings.favourites_root.join(folder_name));
    if store
        .state
        .sessions
        .iter()
        .any(|session| session.name.to_lowercase() == name.to_lowercase())
    {
        return Err(AppError::Invalid(format!("A session named \"{name}\" already exists")));
    }
    if let Some(owner) = store.state.sessions.iter().find(|session| session.dir == dir) {
        return Err(AppError::Invalid(format!(
            "Session \"{}\" already uses {}",
            owner.name,
            dir.display()
        )));
    }
    fs::create_dir_all(&dir)?;

    let id = SessionId::new_random();
    store.state.sessions.push(Session {
        id,
        name,
        dir,
        favourites: BTreeMap::new(),
    });
    store.state.active_session_id = Some(id);
    store.save()?;
    Ok(id)
}

pub fn set_active_session(store: &mut Store, id: SessionId) -> AppResult<()> {
    if !store.state.sessions.iter().any(|session| session.id == id) {
        return Err(AppError::NotFound("session".into()));
    }
    store.state.active_session_id = Some(id);
    store.save()
}

/// Removes the session from the app only: its folder and every file in it stay untouched.
pub fn forget_session(store: &mut Store, id: SessionId) -> AppResult<()> {
    store.state.sessions.retain(|session| session.id != id);
    if store.state.active_session_id == Some(id) {
        store.state.active_session_id = None;
    }
    store.save()
}

pub fn inspect_session_dir(dir: &Path) -> SessionDirInfo {
    match fs::read_dir(dir) {
        Ok(entries) => SessionDirInfo {
            exists: true,
            file_count: entries.filter_map(Result::ok).count(),
        },
        Err(_) => SessionDirInfo {
            exists: false,
            file_count: 0,
        },
    }
}

pub fn session_summaries(store: &Store) -> Vec<SessionSummary> {
    store
        .state
        .sessions
        .iter()
        .map(|session| SessionSummary {
            id: session.id,
            name: session.name.clone(),
            dir: session.dir.clone(),
            favourite_count: session.favourites.len(),
        })
        .collect()
}

/// Source paths favourited in the active session. Copies the user deleted or renamed outside the app
/// are dropped first, so the ★ always mirrors what is actually in the folder.
pub fn favourites_of_active_session(store: &mut Store) -> AppResult<(Option<SessionId>, HashSet<PathBuf>)> {
    let Some(session) = store.state.active_session_mut() else {
        return Ok((None, HashSet::new()));
    };
    let count_before = session.favourites.len();
    let dir = session.dir.clone();
    session
        .favourites
        .retain(|_, favourite| dir.join(&favourite.file_name).exists());
    let pruned = session.favourites.len() != count_before;
    let result = (Some(session.id), session.favourites.keys().cloned().collect());
    if pruned {
        store.save()?;
    }
    Ok(result)
}
