mod common;

use common::raw_fixtures::cr3_raw;
use common::{
    app_harness, create_session, files_in, jpeg_bytes, photo_folder_with, run_until, run_until_label, test_core, toggle,
};
use egui::Key;
use photo_sorter::core::{Command, Event};
use photo_sorter::models::settings::Settings;
use photo_sorter::processor::photo_folder::PhotoFolder;
use std::fs;

fn open_folder(core: &common::TestCore, folder: &std::path::Path) -> PhotoFolder {
    core.send(Command::OpenFolder(folder.to_path_buf()));
    match core.expect_event() {
        Event::FolderOpened(folder) => folder,
        other => panic!("expected FolderOpened, got {other:?}"),
    }
}

#[test]
fn shot_files_are_grouped_into_one_photo_preferring_the_jpeg() {
    let core = test_core();
    let folder = photo_folder_with(&[
        ("A.JPG", b"jpg"),
        ("a.CR3", b"raw"),
        ("A.CR3.xmp", b"edits"),
        ("B.NEF", b"raw"),
        ("orphan.xmp", b"edits"),
        ("notes.txt", b"x"),
    ]);

    let photos = open_folder(&core, folder.path()).photos;

    let summary: Vec<(&str, Vec<String>)> = photos
        .iter()
        .map(|photo| {
            let companions = photo
                .companions
                .iter()
                .map(|path| path.file_name().unwrap().to_string_lossy().into_owned())
                .collect();
            (photo.file_name.as_str(), companions)
        })
        .collect();
    assert_eq!(
        summary,
        [
            ("A.JPG", vec!["a.CR3".to_owned(), "A.CR3.xmp".to_owned()]),
            ("B.NEF", vec![])
        ]
    );
}

#[test]
fn favouriting_copies_the_whole_shot_and_unfavouriting_trashes_it() {
    let core = test_core();
    let folder = photo_folder_with(&[
        ("A.JPG", b"jpg"),
        ("A.CR3", b"raw"),
        ("A.CR3.xmp", b"edits"),
        ("B.JPG", b"x"),
    ]);
    create_session(&core, "Wedding");
    let session_dir = core.favourites_root().join("Wedding");
    let source = folder.path().join("A.JPG");

    assert!(matches!(
        toggle(&core, &source),
        Event::FavouriteToggled { favourited: true, .. }
    ));
    assert_eq!(files_in(&session_dir), ["A.CR3", "A.CR3.xmp", "A.JPG"]);
    for name in ["A.JPG", "A.CR3", "A.CR3.xmp"] {
        assert_eq!(
            fs::read(session_dir.join(name)).unwrap(),
            fs::read(folder.path().join(name)).unwrap()
        );
    }
    let favourite = &core.state_on_disk()["sessions"][0]["favourites"][source.to_str().unwrap()];
    assert_eq!(favourite["file_name"], "A.JPG");
    assert_eq!(
        favourite["companion_file_names"],
        serde_json::json!(["A.CR3", "A.CR3.xmp"])
    );

    assert!(matches!(
        toggle(&core, &source),
        Event::FavouriteToggled { favourited: false, .. }
    ));
    assert_eq!(
        files_in(&session_dir),
        ["A.CR3.trashed", "A.CR3.xmp.trashed", "A.JPG.trashed"]
    );
}

#[test]
fn name_collision_suffixes_every_file_of_the_shot_alike() {
    let core = test_core();
    let camera_a = photo_folder_with(&[("IMG_0001.JPG", b"a"), ("IMG_0001.CR3", b"a raw")]);
    let camera_b = photo_folder_with(&[
        ("IMG_0001.JPG", b"b"),
        ("IMG_0001.CR3", b"b raw"),
        ("IMG_0001.CR3.xmp", b"b"),
    ]);
    create_session(&core, "Two shooters");

    toggle(&core, &camera_a.path().join("IMG_0001.JPG"));
    toggle(&core, &camera_b.path().join("IMG_0001.JPG"));

    let session_dir = core.favourites_root().join("Two shooters");
    assert_eq!(
        files_in(&session_dir),
        [
            "IMG_0001 (2).CR3",
            "IMG_0001 (2).CR3.xmp",
            "IMG_0001 (2).JPG",
            "IMG_0001.CR3",
            "IMG_0001.JPG"
        ]
    );
    assert_eq!(fs::read(session_dir.join("IMG_0001 (2).CR3")).unwrap(), b"b raw");
}

#[test]
fn a_failed_companion_copy_leaves_nothing_behind() {
    let core = test_core();
    let folder = photo_folder_with(&[("A.JPG", b"jpg"), ("A.CR3", b"raw")]);
    create_session(&core, "Failing");
    let session_dir = core.favourites_root().join("Failing");
    // A directory where the RAW's temporary copy must go makes that copy fail on every OS.
    fs::create_dir(session_dir.join("A.CR3.partial")).unwrap();

    let event = toggle(&core, &folder.path().join("A.JPG"));

    assert!(matches!(event, Event::FavouriteFailed { .. }));
    assert_eq!(files_in(&session_dir), ["A.CR3.partial"]);
    assert_eq!(core.state_on_disk()["sessions"][0]["favourites"], serde_json::json!({}));
}

#[test]
fn companions_can_be_turned_off() {
    let core = test_core();
    let folder = photo_folder_with(&[("A.JPG", b"jpg"), ("A.CR3", b"raw")]);
    create_session(&core, "JPEG only");
    let mut settings: Settings = serde_json::from_value(core.state_on_disk()["settings"].clone()).unwrap();
    assert!(settings.copy_companions, "on by default");
    settings.copy_companions = false;
    core.send(Command::UpdateSettings(settings));
    assert!(matches!(core.expect_event(), Event::SettingsChanged(s) if !s.copy_companions));

    toggle(&core, &folder.path().join("A.JPG"));

    assert_eq!(files_in(&core.favourites_root().join("JPEG only")), ["A.JPG"]);
}

#[test]
fn viewer_shows_the_pair_once_with_a_badge_and_enter_copies_both() {
    let folder = tempfile::tempdir().unwrap();
    let jpeg = jpeg_bytes(600, 400, |x, _| [x as u8, 80, 80]);
    fs::write(folder.path().join("IMG_1.JPG"), &jpeg).unwrap();
    fs::write(
        folder.path().join("IMG_1.CR3"),
        cr3_raw(&jpeg, &jpeg_bytes(160, 120, |_, _| [9; 3]), 1),
    )
    .unwrap();
    fs::write(
        folder.path().join("IMG_2.CR3"),
        cr3_raw(&jpeg, &jpeg_bytes(160, 120, |_, _| [9; 3]), 1),
    )
    .unwrap();
    let state_dir = tempfile::tempdir().unwrap();
    let session_dir = state_dir.path().join("Party");
    let mut harness = app_harness(state_dir.path());
    harness.state_mut().send(Command::CreateSession {
        name: "Party".into(),
        dir: Some(session_dir.clone()),
    });
    harness
        .state_mut()
        .send(Command::OpenFolder(folder.path().to_path_buf()));

    run_until_label(&mut harness, "IMG_1.JPG · 1 / 2 JPG+RAW");
    harness.key_press(Key::Enter);
    run_until(&mut harness, "both files in the session folder", |_| {
        session_dir.join("IMG_1.JPG").exists() && session_dir.join("IMG_1.CR3").exists()
    });

    harness.key_press(Key::ArrowRight);
    run_until_label(&mut harness, "IMG_2.CR3 · 2 / 2 RAW");
    run_until(&mut harness, "RAW preview decoded", |h| {
        h.state()
            .image_cache
            .entry(&folder.path().join("IMG_2.CR3"))
            .is_some_and(|entry| matches!(entry.full, photo_sorter::imaging::prefetch::Slot::Ready(_)))
    });
}
