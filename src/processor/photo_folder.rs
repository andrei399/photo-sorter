use crate::error::{AppError, AppResult};
use crate::imaging::raw_preview::RAW_EXTENSIONS;
use crate::models::store::Store;
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

/// What the imaging pipeline decodes directly; RAWs are shown through their embedded JPEG.
const VIEWABLE_EXTENSIONS: &[&str] = &["jpg", "jpeg", "png", "webp", "gif", "bmp", "tif", "tiff"];
/// Lightroom/darktable edits; they travel with the shot so the edits survive favouriting.
const SIDECAR_EXTENSION: &str = "xmp";

/// One shot, shown once even when the camera wrote it as several files (`IMG_0042.JPG` + `.CR3` + `.xmp`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Photo {
    /// The file displayed: the camera's JPEG when there is one, else the RAW.
    pub path: PathBuf,
    pub file_name: String,
    /// The shot's other files, copied along when favouriting.
    pub companions: Vec<PathBuf>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PhotoFolder {
    pub folder: PathBuf,
    pub photos: Vec<Photo>,
}

/// Declaration order is display preference.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum ShotFileKind {
    Viewable,
    Raw,
    Sidecar,
}

impl ShotFileKind {
    fn of_extension(extension: &str) -> Option<Self> {
        let is_one_of = |known: &[&str]| known.iter().any(|k| k.eq_ignore_ascii_case(extension));
        if is_one_of(VIEWABLE_EXTENSIONS) {
            Some(Self::Viewable)
        } else if is_one_of(RAW_EXTENSIONS) {
            Some(Self::Raw)
        } else if extension.eq_ignore_ascii_case(SIDECAR_EXTENSION) {
            Some(Self::Sidecar)
        } else {
            None
        }
    }
}

/// `("IMG_0042", ".CR3.xmp")`: the stem shared by every file of a shot, and the rest of the name.
/// `photo.v2.xmp` stays with `photo.v2`, because `v2` isn't a photo extension.
pub fn split_shot_name(file_name: &str) -> (&str, &str) {
    let Some((stem, extension)) = split_extension(file_name) else {
        return (file_name, "");
    };
    if extension[1..].eq_ignore_ascii_case(SIDECAR_EXTENSION)
        && let Some((inner_stem, inner_extension)) = split_extension(stem)
        && matches!(
            ShotFileKind::of_extension(&inner_extension[1..]),
            Some(ShotFileKind::Viewable | ShotFileKind::Raw)
        )
    {
        return (inner_stem, &file_name[inner_stem.len()..]);
    }
    (stem, extension)
}

fn split_extension(name: &str) -> Option<(&str, &str)> {
    name.rfind('.').filter(|&dot| dot > 0).map(|dot| name.split_at(dot))
}

/// Non-recursive: camera folders (DCIM/100CANON) are flat. Natural order keeps IMG_2 before IMG_10,
/// which for one camera is also chronological. Files sharing a case-insensitive stem are one shot.
// ponytail: grouping is by name only; two cameras writing the same names into one folder get merged. Compare EXIF capture time if that's reported.
pub fn scan_photo_folder(folder: &Path) -> AppResult<PhotoFolder> {
    let entries = fs::read_dir(folder).map_err(|error| match error.kind() {
        std::io::ErrorKind::NotFound => AppError::NotFound(folder.display().to_string()),
        _ => error.into(),
    })?;
    let mut shots: HashMap<String, Vec<(ShotFileKind, String)>> = HashMap::new();
    for entry in entries.filter_map(Result::ok) {
        let Ok(file_name) = entry.file_name().into_string() else {
            continue;
        };
        // Hidden files and macOS `._*` AppleDouble files (common on SD cards touched by a Mac) are never photos.
        if file_name.starts_with('.') || !entry.file_type().is_ok_and(|kind| kind.is_file()) {
            continue;
        }
        let (stem, rest) = split_shot_name(&file_name);
        let extension = rest.rsplit('.').next().unwrap_or_default();
        if let Some(kind) = ShotFileKind::of_extension(extension) {
            shots.entry(stem.to_lowercase()).or_default().push((kind, file_name));
        }
    }
    let mut photos: Vec<Photo> = shots
        .into_values()
        .filter_map(|mut files| {
            files.sort();
            let mut files = files.into_iter();
            let (display_kind, file_name) = files.next()?;
            (display_kind != ShotFileKind::Sidecar).then(|| Photo {
                path: folder.join(&file_name),
                file_name,
                companions: files.map(|(_, name)| folder.join(name)).collect(),
            })
        })
        .collect();
    photos.sort_by(|a, b| natord::compare_ignore_case(&a.file_name, &b.file_name));
    Ok(PhotoFolder {
        folder: folder.to_path_buf(),
        photos,
    })
}

/// Re-derived from disk on every favourite, so the UI can't make the core copy files outside the shot.
pub fn companions_of(source: &Path) -> Vec<PathBuf> {
    source
        .parent()
        .and_then(|folder| scan_photo_folder(folder).ok())
        .and_then(|folder| folder.photos.into_iter().find(|photo| photo.path == source))
        .map(|photo| photo.companions)
        .unwrap_or_default()
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
