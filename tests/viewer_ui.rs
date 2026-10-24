mod common;

use common::{app_harness, jpeg_folder, run_until, run_until_label};
use egui::Key;
use photo_sorter::core::Command;

#[test]
fn keyboard_navigation_moves_through_folder_and_stops_at_ends() {
    let photos = jpeg_folder(5);
    let state_dir = tempfile::tempdir().unwrap();
    let mut harness = app_harness(state_dir.path());

    harness
        .state_mut()
        .send(Command::OpenFolder(photos.path().to_path_buf()));
    run_until_label(&mut harness, "IMG_1.jpg · 1 / 5");

    harness.key_press(Key::ArrowRight);
    harness.key_press(Key::ArrowRight);
    run_until_label(&mut harness, "IMG_3.jpg · 3 / 5");

    harness.key_press(Key::End);
    run_until_label(&mut harness, "IMG_5.jpg · 5 / 5");
    harness.key_press(Key::ArrowRight);
    run_until_label(&mut harness, "IMG_5.jpg · 5 / 5");

    harness.key_press(Key::Home);
    harness.key_press(Key::ArrowLeft);
    run_until_label(&mut harness, "IMG_1.jpg · 1 / 5");
}

#[test]
fn enter_favourites_current_photo_into_active_session() {
    let photos = jpeg_folder(3);
    let state_dir = tempfile::tempdir().unwrap();
    let session_dir = state_dir.path().join("Party");
    let mut harness = app_harness(state_dir.path());
    harness.state_mut().send(Command::CreateSession {
        name: "Party".into(),
        dir: Some(session_dir.clone()),
    });
    harness
        .state_mut()
        .send(Command::OpenFolder(photos.path().to_path_buf()));
    run_until_label(&mut harness, "IMG_1.jpg · 1 / 3");

    harness.key_press(Key::ArrowRight);
    harness.key_press(Key::Enter);
    run_until(&mut harness, "copy in session folder", |_| {
        session_dir.join("IMG_2.jpg").exists()
    });

    run_until_label(&mut harness, "Favourited");
    assert!(
        harness
            .state()
            .sessions
            .favourited
            .contains(&photos.path().join("IMG_2.jpg"))
    );
}
