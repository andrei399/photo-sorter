use crate::error::{AppError, AppResult};
use crate::models::store::Store;
use std::fs;
use std::path::{Path, PathBuf};

/// What the imaging pipeline can decode. RAW joins in phase 06.
const VIEWABLE_EXTENSIONS: &[&str] = &["jpg", "jpeg", "png", "webp", "gif", "bmp", "tif", "tiff"];

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Photo {
    pub path: PathBuf,
    pub file_name: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PhotoFolder {
    pub folder: PathBuf,
    pub photos: Vec<Photo>,
}

/// Non-recursive: camera folders (DCIM/100CANON) are flat. Natural order keeps IMG_2 before IMG_10,
/// which for one camera is also chronological.
pub fn scan_photo_folder(folder: &Path) -> AppResult<PhotoFolder> {
    let entries = fs::read_dir(folder).map_err(|error| match error.kind() {
        std::io::ErrorKind::NotFound => AppError::NotFound(folder.display().to_string()),
        _ => error.into(),
    })?;
    let mut photos: Vec<Photo> = entries
        .filter_map(Result::ok)
        .filter(|entry| entry.file_type().is_ok_and(|kind| kind.is_file()))
        .filter_map(|entry| {
            let file_name = entry.file_name().into_string().ok()?;
            is_viewable_photo(&file_name).then(|| Photo {
                path: entry.path(),
                file_name,
            })
        })
        .collect();
    photos.sort_by(|a, b| natord::compare_ignore_case(&a.file_name, &b.file_name));
    Ok(PhotoFolder {
        folder: folder.to_path_buf(),
        photos,
    })
}

/// Hidden files and macOS `._*` AppleDouble files (common on SD cards touched by a Mac) are never photos.
fn is_viewable_photo(file_name: &str) -> bool {
    !file_name.starts_with('.')
        && Path::new(file_name)
            .extension()
            .and_then(|ext| ext.to_str())
            .is_some_and(|ext| VIEWABLE_EXTENSIONS.iter().any(|known| known.eq_ignore_ascii_case(ext)))
}

pub fn open_photo_folder(store: &mut Store, folder: &Path) -> AppResult<PhotoFolder> {
    let photo_folder = scan_photo_folder(folder)?;
    if store.state.last_opened_folder.as_deref() != Some(folder) {
        store.state.last_opened_folder = Some(folder.to_path_buf());
        store.save()?;
    }
    Ok(photo_folder)
}

/// Startup convenience: silently nothing when the last folder is gone (unplugged SD card).
pub fn reopen_last_folder(store: &Store) -> Option<PhotoFolder> {
    scan_photo_folder(store.state.last_opened_folder.as_deref()?).ok()
}
