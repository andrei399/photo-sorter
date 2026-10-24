mod common;

use common::{restart, test_core};
use photo_sorter::core::{Command, Event};
use photo_sorter::error::AppError;
use std::fs;

fn file_names(event: Event) -> Vec<String> {
    match event {
        Event::FolderOpened(folder) => folder.photos.into_iter().map(|photo| photo.file_name).collect(),
        other => panic!("expected FolderOpened, got {other:?}"),
    }
}

#[test]
fn open_folder_lists_photos_in_natural_order_skipping_junk() {
    let core = test_core();
    let photos = tempfile::tempdir().unwrap();
    for name in [
        "IMG_10.JPG",
        "IMG_2.jpg",
        "notes.txt",
        ".hidden.jpg",
        "._IMG_2.jpg",
        "pano.TIFF",
    ] {
        fs::write(photos.path().join(name), b"x").unwrap();
    }
    fs::create_dir(photos.path().join("sub")).unwrap();
    fs::write(photos.path().join("sub/IMG_1.jpg"), b"x").unwrap();

    core.send(Command::OpenFolder(photos.path().to_path_buf()));

    assert_eq!(
        file_names(core.expect_event()),
        ["IMG_2.jpg", "IMG_10.JPG", "pano.TIFF"]
    );
}

#[test]
fn last_folder_is_persisted_atomically_and_reopened_on_restart() {
    let core = test_core();
    let photos = tempfile::tempdir().unwrap();
    fs::write(photos.path().join("a.jpg"), b"x").unwrap();

    core.send(Command::OpenFolder(photos.path().to_path_buf()));
    core.expect_event();

    assert_eq!(
        core.state_on_disk()["last_opened_folder"],
        photos.path().to_str().unwrap()
    );
    assert!(!core.dir.path().join("state.json.tmp").exists());

    let core = restart(core);
    core.drain_startup();
    assert_eq!(file_names(core.expect_event()), ["a.jpg"]);
}

#[test]
fn restart_with_vanished_last_folder_stays_quiet() {
    let core = test_core();
    let photos = tempfile::tempdir().unwrap();
    core.send(Command::OpenFolder(photos.path().to_path_buf()));
    core.expect_event();
    drop(photos);

    let core = restart(core);
    core.drain_startup();
    core.expect_no_event();
}

#[test]
fn missing_folder_reports_not_found_and_core_keeps_working() {
    let core = test_core();

    core.send(Command::OpenFolder("/definitely/not/here".into()));
    assert!(matches!(core.expect_event(), Event::Error(AppError::NotFound(_))));

    let photos = tempfile::tempdir().unwrap();
    core.send(Command::OpenFolder(photos.path().to_path_buf()));
    assert_eq!(file_names(core.expect_event()), Vec::<String>::new());
}
