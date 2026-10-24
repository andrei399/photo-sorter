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

fn settings_open(harness: &common::AppHarness) -> bool {
    matches!(harness.state().modal, Some(Modal::Settings(_)))
}

fn draft_keybindings(harness: &common::AppHarness, action: Action) -> Vec<KeyCombo> {
    match &harness.state().modal {
        Some(Modal::Settings(draft)) => draft.settings.keybindings[&action].clone(),
        _ => panic!("settings dialog is closed"),
    }
}

fn click_button(harness: &mut common::AppHarness, label: &str) {
    harness.get_by_role_and_label(Role::Button, label).click();
    harness.run_steps(2);
}

#[test]
fn key_replaced_in_the_dialog_favourites_and_the_old_key_no_longer_does() {
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

    harness.key_press_modifiers(Modifiers::COMMAND, Key::Comma);
    run_until(&mut harness, "settings opened by Cmd/Ctrl+,", settings_open);
    click_button(&mut harness, "Toggle favourite: Enter");
    harness.key_press(Key::F);
    harness.run_steps(2);
    assert_eq!(
        draft_keybindings(&harness, Action::ToggleFavourite),
        [KeyCombo::plain(Key::F)]
    );
    click_button(&mut harness, "Save");
    run_until(&mut harness, "dialog closed after save", |h| h.state().modal.is_none());

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
fn conflicts_block_saving_and_closing_without_saving_discards_the_draft() {
    let photos = jpeg_folder(2);
    let state_dir = tempfile::tempdir().unwrap();
    let mut harness = app_harness(state_dir.path());
    harness
        .state_mut()
        .send(Command::OpenFolder(photos.path().to_path_buf()));
    run_until_label(&mut harness, "IMG_1.jpg · 1 / 2");
    let state_before = std::fs::read(state_dir.path().join("state.json")).unwrap();

    click_button(&mut harness, "Settings");
    run_until(&mut harness, "settings opened by the gear", settings_open);
    click_button(&mut harness, "Add shortcut for Next photo");
    harness.key_press(Key::Enter);
    run_until_label(&mut harness, "Enter is bound to both Toggle favourite and Next photo");
    click_button(&mut harness, "Save");
    harness.run_steps(5);
    assert!(settings_open(&harness), "Save is disabled while a conflict remains");

    click_button(&mut harness, "Next photo: Enter");
    harness.key_press(Key::Delete);
    harness.run_steps(2);
    assert_eq!(
        draft_keybindings(&harness, Action::Next),
        [KeyCombo::plain(Key::ArrowRight)]
    );

    click_button(&mut harness, "Add shortcut for First photo");
    harness.key_press(Key::Escape);
    harness.run_steps(2);
    assert!(settings_open(&harness), "Esc while capturing only cancels the capture");
    assert_eq!(draft_keybindings(&harness, Action::First), [KeyCombo::plain(Key::Home)]);

    click_button(&mut harness, "First photo: Home");
    harness.key_press(Key::S);
    harness.run_steps(2);
    harness.key_press(Key::Escape);
    run_until(&mut harness, "dialog closed by Esc", |h| h.state().modal.is_none());
    harness.key_press(Key::S);
    harness.run_steps(2);
    assert_eq!(harness.state().index, 0, "the unsaved S binding was discarded");
    assert_eq!(
        std::fs::read(state_dir.path().join("state.json")).unwrap(),
        state_before
    );
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
