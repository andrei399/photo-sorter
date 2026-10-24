mod common;

use common::{
    create_session, failing_trash, files_in, photo_folder_with, restart, test_core, test_core_with_trash, toggle,
};
use photo_sorter::core::{Command, Event};
use photo_sorter::error::AppError;
use std::fs;
use std::path::{Path, PathBuf};

#[test]
fn favourite_copies_photo_into_session_folder_and_persists() {
    let core = test_core();
    let photos = photo_folder_with(&[("IMG_1.jpg", b"original bytes")]);
    create_session(&core, "Wedding Ana & Mihai");
    let source = photos.path().join("IMG_1.jpg");

    let event = toggle(&core, &source);

    assert!(matches!(event, Event::FavouriteToggled { favourited: true, .. }));
    let session_dir = core.favourites_root().join("Wedding Ana & Mihai");
    assert_eq!(files_in(&session_dir), ["IMG_1.jpg"]);
    assert_eq!(fs::read(session_dir.join("IMG_1.jpg")).unwrap(), b"original bytes");
    let state = core.state_on_disk();
    assert_eq!(
        state["sessions"][0]["favourites"][source.to_str().unwrap()]["file_name"],
        "IMG_1.jpg"
    );
}

#[test]
fn unfavourite_moves_only_our_copy_to_trash() {
    let core = test_core();
    let photos = photo_folder_with(&[("IMG_1.jpg", b"x")]);
    create_session(&core, "Party");
    let source = photos.path().join("IMG_1.jpg");
    toggle(&core, &source);

    let event = toggle(&core, &source);

    assert!(matches!(event, Event::FavouriteToggled { favourited: false, .. }));
    assert_eq!(files_in(&core.favourites_root().join("Party")), ["IMG_1.jpg.trashed"]);
    assert!(source.exists(), "the source photo is never touched");
    assert_eq!(core.state_on_disk()["sessions"][0]["favourites"], serde_json::json!({}));
}

#[test]
fn trash_failure_keeps_the_file_and_warns() {
    let core = test_core_with_trash(failing_trash);
    let photos = photo_folder_with(&[("IMG_1.jpg", b"x")]);
    create_session(&core, "Party");
    let source = photos.path().join("IMG_1.jpg");
    toggle(&core, &source);

    assert!(matches!(
        toggle(&core, &source),
        Event::FavouriteToggled { favourited: false, .. }
    ));
    assert!(matches!(core.expect_event(), Event::Error(AppError::Io(message)) if message.contains("kept")));
    assert_eq!(files_in(&core.favourites_root().join("Party")), ["IMG_1.jpg"]);
}

#[test]
fn same_file_name_from_two_cameras_gets_numbered_copy() {
    let core = test_core();
    let camera_a = photo_folder_with(&[("IMG_0001.JPG", b"camera a")]);
    let camera_b = photo_folder_with(&[("IMG_0001.JPG", b"camera b")]);
    create_session(&core, "Two shooters");
    toggle(&core, &camera_a.path().join("IMG_0001.JPG"));
    toggle(&core, &camera_b.path().join("IMG_0001.JPG"));

    let session_dir = core.favourites_root().join("Two shooters");
    assert_eq!(files_in(&session_dir), ["IMG_0001 (2).JPG", "IMG_0001.JPG"]);
    assert_eq!(fs::read(session_dir.join("IMG_0001 (2).JPG")).unwrap(), b"camera b");

    toggle(&core, &camera_b.path().join("IMG_0001.JPG"));
    assert_eq!(files_in(&session_dir), ["IMG_0001 (2).JPG.trashed", "IMG_0001.JPG"]);
}

#[test]
fn sessions_track_favourites_independently() {
    let core = test_core();
    let photos = photo_folder_with(&[("IMG_1.jpg", b"x")]);
    let source = photos.path().join("IMG_1.jpg");
    let first = create_session(&core, "First");
    toggle(&core, &source);
    create_session(&core, "Second"); // becomes active, nothing favourited yet

    toggle(&core, &source);
    core.send(Command::SetActiveSession(first));
    assert!(matches!(core.expect_event(), Event::SessionsChanged { active: Some(id), .. } if id == first));
    let Event::FavouritesOfActive { sources, .. } = core.expect_event() else {
        panic!()
    };

    assert_eq!(sources.into_iter().collect::<Vec<PathBuf>>(), [source]);
    assert_eq!(files_in(&core.favourites_root().join("First")), ["IMG_1.jpg"]);
    assert_eq!(files_in(&core.favourites_root().join("Second")), ["IMG_1.jpg"]);
}

#[test]
fn favourite_without_session_fails_without_side_effects() {
    let core = test_core();
    let photos = photo_folder_with(&[("IMG_1.jpg", b"x")]);

    let event = toggle(&core, &photos.path().join("IMG_1.jpg"));

    assert!(matches!(
        event,
        Event::FavouriteFailed {
            error: AppError::Invalid(_),
            ..
        }
    ));
    assert!(!core.favourites_root().exists());
}

#[test]
fn forgetting_a_session_keeps_its_files() {
    let core = test_core();
    let photos = photo_folder_with(&[("IMG_1.jpg", b"x")]);
    let id = create_session(&core, "Keep me");
    toggle(&core, &photos.path().join("IMG_1.jpg"));

    core.send(Command::ForgetSession(id));
    assert!(matches!(core.expect_event(), Event::SessionsChanged { sessions, active: None } if sessions.is_empty()));

    assert_eq!(files_in(&core.favourites_root().join("Keep me")), ["IMG_1.jpg"]);
}

#[test]
fn session_names_are_validated_and_sanitized_for_every_os() {
    let core = test_core();
    for bad_name in ["CON", "   ", "lpt1.txt", "..."] {
        core.send(Command::CreateSession {
            name: bad_name.into(),
            dir: None,
        });
        assert!(
            matches!(core.expect_event(), Event::SessionCreateFailed(AppError::Invalid(_))),
            "{bad_name:?} must be rejected"
        );
    }
    create_session(&core, "My/Event: 2?");
    assert!(core.favourites_root().join("My_Event_ 2_").is_dir());

    core.send(Command::CreateSession {
        name: "my/event: 2?".into(),
        dir: None,
    });
    assert!(
        matches!(core.expect_event(), Event::SessionCreateFailed(AppError::Invalid(message)) if message.contains("already exists"))
    );
}

#[test]
fn deleted_session_folder_is_recreated_on_next_favourite() {
    let core = test_core();
    let photos = photo_folder_with(&[("IMG_1.jpg", b"x")]);
    create_session(&core, "Gone");
    fs::remove_dir(core.favourites_root().join("Gone")).unwrap();

    toggle(&core, &photos.path().join("IMG_1.jpg"));

    assert_eq!(files_in(&core.favourites_root().join("Gone")), ["IMG_1.jpg"]);
}

#[test]
fn externally_deleted_copy_is_no_longer_favourited() {
    let core = test_core();
    let photos = photo_folder_with(&[("IMG_1.jpg", b"x")]);
    let id = create_session(&core, "Pruned");
    toggle(&core, &photos.path().join("IMG_1.jpg"));
    fs::remove_file(core.favourites_root().join("Pruned/IMG_1.jpg")).unwrap();

    core.send(Command::SetActiveSession(id));
    core.expect_event();
    let Event::FavouritesOfActive { sources, .. } = core.expect_event() else {
        panic!()
    };

    assert!(sources.is_empty());
    assert_eq!(core.state_on_disk()["sessions"][0]["favourites"], serde_json::json!({}));
}

#[test]
fn failed_copy_leaves_no_partial_file_and_no_state() {
    let core = test_core();
    create_session(&core, "Failing");

    let event = toggle(&core, Path::new("/no/such/photo.jpg"));

    assert!(matches!(event, Event::FavouriteFailed { .. }));
    assert!(files_in(&core.favourites_root().join("Failing")).is_empty());
    assert_eq!(core.state_on_disk()["sessions"][0]["favourites"], serde_json::json!({}));
}

#[test]
fn sessions_and_favourites_survive_restart() {
    let core = test_core();
    let photos = photo_folder_with(&[("IMG_1.jpg", b"x")]);
    let id = create_session(&core, "Persisted");
    toggle(&core, &photos.path().join("IMG_1.jpg"));

    let core = restart(core);

    assert!(matches!(core.expect_event(), Event::Snapshot(_)));
    assert!(
        matches!(core.expect_event(), Event::SessionsChanged { active: Some(active), sessions } if active == id && sessions[0].favourite_count == 1)
    );
    assert!(matches!(core.expect_event(), Event::FavouritesOfActive { sources, .. } if sources.len() == 1));
}

#[test]
fn inspect_reports_existing_files_in_a_folder() {
    let core = test_core();
    let existing = photo_folder_with(&[("a.jpg", b"x"), ("b.jpg", b"y")]);

    core.send(Command::InspectSessionDir(existing.path().to_path_buf()));

    assert!(
        matches!(core.expect_event(), Event::SessionDirInspected { info, .. } if info.exists && info.file_count == 2)
    );
}
