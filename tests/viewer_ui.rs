mod common;

use common::{app_harness, jpeg_bytes, jpeg_folder, photo_folder_with, run_until, run_until_label, with_capture_time};
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

#[test]
fn update_notice_waits_for_the_user_and_can_be_dismissed() {
    use egui_kittest::kittest::Queryable;
    use photo_sorter::integrations::updater::{AvailableUpdate, UpdateInstall};
    let state_dir = tempfile::tempdir().unwrap();
    let mut harness = app_harness(state_dir.path());

    harness.state_mut().available_update = Some(AvailableUpdate {
        version: "9.9.9".into(),
        install: UpdateInstall::FromReleasePage,
    });
    run_until_label(&mut harness, "Photo Sorter v9.9.9 is out");

    harness.get_by_label("×").click();
    run_until(&mut harness, "notice dismissed", |h| {
        h.query_by_label_contains("v9.9.9").is_none()
    });
    assert!(harness.state().available_update.is_none());
}

#[test]
fn typing_a_number_jumps_to_that_photo_and_escape_cancels() {
    use egui_kittest::kittest::Queryable;
    let photos = jpeg_folder(5);
    let state_dir = tempfile::tempdir().unwrap();
    let mut harness = app_harness(state_dir.path());
    harness
        .state_mut()
        .send(Command::OpenFolder(photos.path().to_path_buf()));
    run_until_label(&mut harness, "IMG_1.jpg · 1 / 5");

    harness.key_press(Key::G);
    harness.run();
    harness.get_by_role(egui::accesskit::Role::TextInput).type_text("4");
    harness.key_press(Key::Enter);
    run_until_label(&mut harness, "IMG_4.jpg · 4 / 5");
    assert!(
        harness.state().modal.is_none(),
        "Enter confirms the jump, it must not also favourite"
    );

    // Out-of-range numbers land on the last photo; Escape leaves the index alone.
    harness.get_by_label("IMG_4.jpg · 4 / 5").click();
    harness.run();
    harness.get_by_role(egui::accesskit::Role::TextInput).type_text("99");
    harness.key_press(Key::Enter);
    run_until_label(&mut harness, "IMG_5.jpg · 5 / 5");

    harness.key_press(Key::G);
    harness.run();
    harness.get_by_role(egui::accesskit::Role::TextInput).type_text("2");
    harness.key_press(Key::Escape);
    run_until_label(&mut harness, "IMG_5.jpg · 5 / 5");
    assert!(harness.state().go_to_number_draft.is_none());
}

#[test]
fn clicking_the_scrub_bar_jumps_to_that_position_in_the_folder() {
    use egui_kittest::kittest::Queryable;
    let photos = jpeg_folder(5);
    let state_dir = tempfile::tempdir().unwrap();
    let mut harness = app_harness(state_dir.path());
    harness
        .state_mut()
        .send(Command::OpenFolder(photos.path().to_path_buf()));
    run_until_label(&mut harness, "IMG_1.jpg · 1 / 5");

    harness.get_by_label("Photo position").click();
    run_until_label(&mut harness, "IMG_3.jpg · 3 / 5");
}

#[test]
fn bottom_bar_shows_when_the_photo_was_taken() {
    let photo = with_capture_time(
        &jpeg_bytes(300, 200, |x, y| [x as u8, y as u8, 60]),
        "2024:06:15 14:32:07",
    );
    let photos = photo_folder_with(&[("IMG_1.jpg", &photo)]);
    let state_dir = tempfile::tempdir().unwrap();
    let mut harness = app_harness(state_dir.path());

    harness
        .state_mut()
        .send(Command::OpenFolder(photos.path().to_path_buf()));
    run_until_label(&mut harness, "IMG_1.jpg · 1 / 1 · 2024-06-15 14:32");
}
