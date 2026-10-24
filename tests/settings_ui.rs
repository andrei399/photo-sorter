mod common;

use common::{app_harness, jpeg_folder, run_until, run_until_label};
use egui::{Key, Modifiers, Role};
use egui_kittest::kittest::Queryable;
use photo_sorter::app::Modal;
use photo_sorter::core::Command;
use photo_sorter::models::settings::{Action, KeyCombo};

/// Rebinds through the core and waits until the UI's key map has picked it up.
fn rebind(harness: &mut common::AppHarness, action: Action, combos: Vec<KeyCombo>) {
    let mut settings = harness.state().state.as_ref().unwrap().settings.clone();
    settings.keybindings.insert(action, combos.clone());
    harness.state_mut().send(Command::UpdateSettings(settings));
    run_until(harness, "rebinding applied", |h| {
        h.state().state.as_ref().unwrap().settings.keybindings[&action] == combos
    });
}

#[test]
fn rebound_key_favourites_and_the_old_key_no_longer_does() {
    let photos = jpeg_folder(2);
    let state_dir = tempfile::tempdir().unwrap();
    let mut harness = app_harness(state_dir.path());
    harness.state_mut().send(Command::CreateSession {
        name: "Party".into(),
        dir: None,
    });
    harness
        .state_mut()
        .send(Command::OpenFolder(photos.path().to_path_buf()));
    run_until_label(&mut harness, "IMG_1.jpg · 1 / 2");
    rebind(&mut harness, Action::ToggleFavourite, vec![KeyCombo::plain(Key::F)]);
    run_until_label(&mut harness, "F");

    harness.key_press(Key::Enter);
    harness.run_steps(5);
    assert!(
        harness.state().sessions.favourited.is_empty(),
        "Enter is no longer bound"
    );

    harness.key_press(Key::F);
    let copy = state_dir.path().join("favourites/Party/IMG_1.jpg");
    run_until(&mut harness, "photo copied by the rebound key", |_| copy.exists());
}

#[test]
fn letter_bindings_never_fire_while_typing_a_session_name() {
    let photos = jpeg_folder(3);
    let state_dir = tempfile::tempdir().unwrap();
    let mut harness = app_harness(state_dir.path());
    harness
        .state_mut()
        .send(Command::OpenFolder(photos.path().to_path_buf()));
    run_until_label(&mut harness, "IMG_1.jpg · 1 / 3");
    rebind(&mut harness, Action::Next, vec![KeyCombo::plain(Key::N)]);

    harness.key_press_modifiers(Modifiers::COMMAND, Key::N);
    run_until_label(&mut harness, "New session");
    harness
        .get_all_by_role(Role::TextInput)
        .next()
        .unwrap()
        .type_text("Nunta");
    harness.key_press(Key::N);
    harness.run_steps(2);

    assert_eq!(harness.state().index, 0, "N typed into the dialog must not navigate");
    assert!(matches!(&harness.state().modal, Some(Modal::NewSession(draft)) if draft.name == "Nunta"));
}
