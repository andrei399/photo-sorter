use crate::error::{AppError, AppResult};
use crate::models::session::Favourite;
use crate::models::store::Store;
use std::fs;
use std::path::Path;

/// Moves a file to the OS recycle bin. Injected so tests can observe it without touching the real trash.
pub type MoveToTrash = fn(&Path) -> std::io::Result<()>;

pub fn move_to_os_trash(path: &Path) -> std::io::Result<()> {
    trash::delete(path).map_err(std::io::Error::other)
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ToggleOutcome {
    Favourited,
    /// `kept_file` is set when the copy couldn't be trashed: it stays on disk rather than being hard-deleted.
    Unfavourited {
        kept_file: Option<String>,
    },
}

/// Runs on the core actor, so toggles are serialized: no copy-vs-trash races on rapid Enter presses.
// ponytail: copies block the core queue (~1 s for 60 MB from an SD card); move to a copy worker if users notice.
pub fn toggle_favourite(store: &mut Store, source: &Path, move_to_trash: MoveToTrash) -> AppResult<ToggleOutcome> {
    let session = store
        .state
        .active_session_mut()
        .ok_or_else(|| AppError::Invalid("Create a session first (Ctrl+N)".into()))?;

    let outcome = match session.favourites.get(source) {
        Some(favourite) => {
            let copy = session.dir.join(&favourite.file_name);
            let kept_file = match copy.exists().then(|| move_to_trash(&copy)) {
                Some(Err(error)) => Some(format!(
                    "Couldn't move {} to the trash ({error}); the file was kept",
                    copy.display()
                )),
                _ => None,
            };
            session.favourites.remove(source);
            ToggleOutcome::Unfavourited { kept_file }
        }
        None => {
            let file_name = copy_into_session_dir(source, &session.dir)?;
            session.favourites.insert(source.to_path_buf(), Favourite { file_name });
            ToggleOutcome::Favourited
        }
    };
    store.save()?;
    Ok(outcome)
}

/// Copies via `<name>.partial` + rename, so neither the user nor Google Drive ever sees a half-written file
/// under its final name. Returns the name used inside `session_dir`.
fn copy_into_session_dir(source: &Path, session_dir: &Path) -> AppResult<String> {
    let source_name = source
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or_else(|| AppError::Invalid(format!("Unsupported file name: {}", source.display())))?;
    // The session folder may have been deleted or live on an unplugged drive that is back now.
    fs::create_dir_all(session_dir)?;
    let file_name = free_name_in(session_dir, source_name);
    let partial_path = session_dir.join(format!("{file_name}.partial"));
    let copy_result =
        fs::copy(source, &partial_path).and_then(|_| fs::rename(&partial_path, session_dir.join(&file_name)));
    if let Err(error) = copy_result {
        let _ = fs::remove_file(&partial_path);
        return Err(error.into());
    }
    Ok(file_name)
}

/// `IMG_0001.JPG`, then `IMG_0001 (2).JPG`, `IMG_0001 (3).JPG`… (two cameras happily produce the same names).
fn free_name_in(dir: &Path, file_name: &str) -> String {
    if !dir.join(file_name).exists() {
        return file_name.to_owned();
    }
    let path = Path::new(file_name);
    let stem = path.file_stem().and_then(|s| s.to_str()).unwrap_or(file_name);
    let extension = path
        .extension()
        .and_then(|e| e.to_str())
        .map(|e| format!(".{e}"))
        .unwrap_or_default();
    (2..)
        .map(|n| format!("{stem} ({n}){extension}"))
        .find(|candidate| !dir.join(candidate).exists())
        .expect("an unbounded range always yields a free name")
}
