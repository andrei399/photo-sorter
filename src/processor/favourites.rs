use crate::error::{AppError, AppResult};
use crate::models::session::Favourite;
use crate::models::store::Store;
use crate::processor::photo_folder::{self, split_shot_name};
use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};

/// Moves a file to the OS recycle bin. Injected so tests can observe it without touching the real trash.
pub type MoveToTrash = fn(&Path) -> std::io::Result<()>;

pub fn move_to_os_trash(path: &Path) -> std::io::Result<()> {
    trash::delete(path).map_err(std::io::Error::other)
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ToggleOutcome {
    Favourited,
    /// Set when copies couldn't be trashed: they stay on disk rather than being hard-deleted.
    Unfavourited {
        kept_files_warning: Option<String>,
    },
}

/// Runs on the core actor, so toggles are serialized: no copy-vs-trash races on rapid Enter presses.
// ponytail: copies block the core queue (~1 s for 60 MB from an SD card); move to a copy worker if users notice.
pub fn toggle_favourite(store: &mut Store, source: &Path, move_to_trash: MoveToTrash) -> AppResult<ToggleOutcome> {
    let copy_companions = store.state.settings.copy_companions;
    let session = store
        .state
        .active_session_mut()
        .ok_or_else(|| AppError::Invalid("Create a session first (Ctrl+N)".into()))?;

    let outcome = match session.favourites.remove(source) {
        Some(favourite) => {
            let kept_files: Vec<String> = favourite
                .file_names()
                .map(|file_name| session.dir.join(file_name))
                .filter_map(|copy| match copy.exists().then(|| move_to_trash(&copy)) {
                    Some(Err(error)) => Some(format!(
                        "Couldn't move {} to the trash ({error}); the file was kept",
                        copy.display()
                    )),
                    _ => None,
                })
                .collect();
            ToggleOutcome::Unfavourited {
                kept_files_warning: (!kept_files.is_empty()).then(|| kept_files.join("\n")),
            }
        }
        None => {
            let mut shot_files = vec![source.to_path_buf()];
            if copy_companions {
                shot_files.extend(photo_folder::companions_of(source));
            }
            // A name whose old cloud copy isn't deleted yet would look already uploaded, so it isn't reused.
            let names_in_cloud: HashSet<&str> = session
                .remote_file_ids
                .values()
                .flat_map(|remote_files| remote_files.keys().map(String::as_str))
                .collect();
            let mut copied_names = copy_shot_into_session_dir(&shot_files, &session.dir, &names_in_cloud)?.into_iter();
            let file_name = copied_names.next().expect("the shot always includes the source");
            session.favourites.insert(
                source.to_path_buf(),
                Favourite {
                    file_name,
                    companion_file_names: copied_names.collect(),
                },
            );
            ToggleOutcome::Favourited
        }
    };
    store.save()?;
    Ok(outcome)
}

/// All or nothing: a shot is never half in the session folder. Returns the names used inside `session_dir`,
/// in the order of `shot_files`.
fn copy_shot_into_session_dir(
    shot_files: &[PathBuf],
    session_dir: &Path,
    names_in_cloud: &HashSet<&str>,
) -> AppResult<Vec<String>> {
    let source_names = shot_files
        .iter()
        .map(|file| {
            file.file_name()
                .and_then(|name| name.to_str())
                .ok_or_else(|| AppError::Invalid(format!("Unsupported file name: {}", file.display())))
        })
        .collect::<AppResult<Vec<&str>>>()?;
    // The session folder may have been deleted or live on an unplugged drive that is back now.
    fs::create_dir_all(session_dir)?;
    let copy_names = free_shot_names_in(session_dir, &source_names, names_in_cloud);
    for (copied_count, (source, copy_name)) in shot_files.iter().zip(&copy_names).enumerate() {
        if let Err(error) = copy_atomically(source, &session_dir.join(copy_name)) {
            // Copies made moments ago in this toggle, never shown as a favourite: deleting them loses nothing.
            for done in &copy_names[..copied_count] {
                let _ = fs::remove_file(session_dir.join(done));
            }
            return Err(error);
        }
    }
    Ok(copy_names)
}

/// Via `<name>.partial` + rename, so neither the user nor Google Drive ever sees a half-written file
/// under its final name.
fn copy_atomically(source: &Path, destination: &Path) -> AppResult<()> {
    let mut partial_path = destination.as_os_str().to_owned();
    partial_path.push(".partial");
    let copy_result = fs::copy(source, &partial_path).and_then(|_| fs::rename(&partial_path, destination));
    if let Err(error) = copy_result {
        let _ = fs::remove_file(&partial_path);
        return Err(error.into());
    }
    Ok(())
}

/// `IMG_0001.JPG`, then `IMG_0001 (2).JPG`… (two cameras happily produce the same names). Every file of a shot
/// gets the same suffix (`IMG_0001 (2).CR3`, `IMG_0001 (2).CR3.xmp`) so Lightroom still pairs them.
fn free_shot_names_in(dir: &Path, file_names: &[&str], names_in_cloud: &HashSet<&str>) -> Vec<String> {
    let with_suffix = |suffix: &str| -> Vec<String> {
        file_names
            .iter()
            .map(|name| {
                let (stem, rest) = split_shot_name(name);
                format!("{stem}{suffix}{rest}")
            })
            .collect()
    };
    std::iter::once(String::new())
        .chain((2..).map(|n| format!(" ({n})")))
        .map(|suffix| with_suffix(&suffix))
        .find(|names| {
            names
                .iter()
                .all(|name| !dir.join(name).exists() && !names_in_cloud.contains(name.as_str()))
        })
        .expect("an unbounded range always yields free names")
}
