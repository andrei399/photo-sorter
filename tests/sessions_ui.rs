mod common;

use common::{app_harness, jpeg_folder, run_until, run_until_label};
use egui::{Key, Modifiers, Role};
use egui_kittest::kittest::Queryable;
use photo_sorter::core::Command;
use photo_sorter::models::settings::KeyCombo;

fn active_session_name(harness: &common::AppHarness) -> Option<String> {
    harness
        .state()
        .sessions
        .active_summary()
        .map(|session| session.name.clone())
}

#[test]
fn first_enter_opens_dialog_then_creates_session_and_favourites_the_photo() {
    let photos = jpeg_folder(2);
    let state_dir = tempfile::tempdir().unwrap();
    let mut harness = app_harness(state_dir.path());
    harness
        .state_mut()
        .send(Command::OpenFolder(photos.path().to_path_buf()));
    run_until_label(&mut harness, "IMG_1.jpg · 1 / 2");

    harness.key_press(Key::Enter);
    run_until_label(&mut harness, "IMG_1.jpg will be favourited into it right away.");
    harness
        .get_all_by_role(Role::TextInput)
        .next()
        .unwrap()
        .type_text("Nunta Ioana");
    harness.run_steps(2);
    harness.key_press(Key::Enter);

    let copy = state_dir.path().join("favourites/Nunta Ioana/IMG_1.jpg");
    run_until(&mut harness, "photo copied into the new session", |_| copy.exists());
    run_until(&mut harness, "dialog closed", |h| h.state().modal.is_none());
    run_until_label(&mut harness, "Favourited");
    assert_eq!(active_session_name(&harness).as_deref(), Some("Nunta Ioana"));
}

#[test]
fn ctrl_n_dialog_swallows_typing_and_esc_cancels_without_creating() {
    let photos = jpeg_folder(3);
    let state_dir = tempfile::tempdir().unwrap();
    let mut harness = app_harness(state_dir.path());
    harness
        .state_mut()
        .send(Command::OpenFolder(photos.path().to_path_buf()));
    run_until_label(&mut harness, "IMG_1.jpg · 1 / 3");

    harness.key_press_modifiers(Modifiers::COMMAND, Key::N);
    run_until_label(&mut harness, "New session");
    harness
        .get_all_by_role(Role::TextInput)
        .next()
        .unwrap()
        .type_text("abc");
    harness.key_press(Key::ArrowRight);
    harness.key_press(Key::End);
    harness.run_steps(2);
    assert_eq!(harness.state().index, 0, "keys typed into the dialog must not navigate");

    harness.key_press(Key::Escape);
    run_until(&mut harness, "dialog closed", |h| h.state().modal.is_none());
    harness.run_steps(5);
    assert!(harness.state().sessions.sessions.is_empty());
    assert!(!state_dir.path().join("favourites/abc").exists());
}

#[test]
fn duplicate_name_is_rejected_inline_and_dialog_stays_open() {
    let state_dir = tempfile::tempdir().unwrap();
    let mut harness = app_harness(state_dir.path());
    harness.state_mut().send(Command::CreateSession {
        name: "Party".into(),
        dir: None,
    });
    run_until(&mut harness, "session exists", |h| {
        !h.state().sessions.sessions.is_empty()
    });

    harness.key_press_modifiers(Modifiers::COMMAND, Key::N);
    run_until_label(&mut harness, "New session");
    harness
        .get_all_by_role(Role::TextInput)
        .next()
        .unwrap()
        .type_text("party");
    run_until_label(&mut harness, "already exists");
    harness.key_press(Key::Enter);
    harness.run_steps(5);

    assert!(harness.state().modal.is_some());
    assert_eq!(harness.state().sessions.sessions.len(), 1);
}

#[test]
fn ctrl_k_filters_sessions_and_enter_switches() {
    let state_dir = tempfile::tempdir().unwrap();
    let mut harness = app_harness(state_dir.path());
    for name in ["Bucharest Marathon", "Wedding Ana & Mihai"] {
        harness.state_mut().send(Command::CreateSession {
            name: name.into(),
            dir: None,
        });
    }
    run_until(&mut harness, "wedding active", |h| {
        active_session_name(h).as_deref() == Some("Wedding Ana & Mihai")
    });

    harness.key_press_modifiers(Modifiers::COMMAND, Key::K);
    run_until_label(&mut harness, "Enter switch");
    harness.get_by_role(Role::TextInput).type_text("mar");
    run_until(&mut harness, "filtered", |h| {
        h.query_by_label_contains("Wedding").is_none()
    });
    harness.key_press(Key::Enter);

    run_until(&mut harness, "marathon active", |h| {
        active_session_name(h).as_deref() == Some("Bucharest Marathon")
    });
    assert!(harness.state().modal.is_none());
}

#[test]
fn ctrl_k_creates_a_session_from_the_query_with_ctrl_enter() {
    let state_dir = tempfile::tempdir().unwrap();
    let mut harness = app_harness(state_dir.path());
    harness.step();

    harness.key_press_modifiers(Modifiers::COMMAND, Key::K);
    run_until_label(&mut harness, "Enter switch");
    harness.get_by_role(Role::TextInput).type_text("Gala 2026");
    harness.run_steps(2);
    harness.key_press_modifiers(Modifiers::COMMAND, Key::Enter);

    run_until(&mut harness, "session created", |h| {
        active_session_name(h).as_deref() == Some("Gala 2026")
    });
    assert!(state_dir.path().join("favourites/Gala 2026").is_dir());
}

#[test]
fn session_menu_lists_cards_and_switches_on_click() {
    let state_dir = tempfile::tempdir().unwrap();
    let photos = jpeg_folder(1);
    let mut harness = app_harness(state_dir.path());
    for name in ["Alpha", "Beta"] {
        harness.state_mut().send(Command::CreateSession {
            name: name.into(),
            dir: None,
        });
    }
    harness
        .state_mut()
        .send(Command::OpenFolder(photos.path().to_path_buf()));
    run_until_label(&mut harness, "Beta · 0 ★");

    harness.get_by_label_contains("Beta · 0 ★").click();
    run_until_label(&mut harness, &format!("+ New session ({})", KeyCombo::command(Key::N)));
    harness.get_by_label("Alpha").click();

    run_until(&mut harness, "alpha active", |h| {
        active_session_name(h).as_deref() == Some("Alpha")
    });
}

#[test]
fn forget_from_menu_asks_first_and_keeps_files() {
    let state_dir = tempfile::tempdir().unwrap();
    let photos = jpeg_folder(1);
    let mut harness = app_harness(state_dir.path());
    harness.state_mut().send(Command::CreateSession {
        name: "Old event".into(),
        dir: None,
    });
    harness
        .state_mut()
        .send(Command::OpenFolder(photos.path().to_path_buf()));
    run_until_label(&mut harness, "Old event · 0 ★");
    harness.key_press(Key::Enter);
    let copy = state_dir.path().join("favourites/Old event/IMG_1.jpg");
    run_until(&mut harness, "favourited", |_| copy.exists());
    run_until_label(&mut harness, "Old event · 1 ★");

    harness.get_by_label_contains("Old event · 1 ★").click();
    run_until_label(&mut harness, "Forget this session");
    harness.get_by_label_contains("Forget this session").click();
    run_until_label(&mut harness, "Forget session");
    harness.get_by_label("Forget session").click();

    run_until(&mut harness, "session forgotten", |h| {
        h.state().sessions.sessions.is_empty()
    });
    assert!(copy.exists(), "forgetting must never delete photos");
}
