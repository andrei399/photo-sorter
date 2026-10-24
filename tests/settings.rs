mod common;

use common::{restart, test_core};
use egui::Key;
use photo_sorter::core::{Command, Event};
use photo_sorter::error::AppError;
use photo_sorter::models::settings::{Action, KeyCombo, Settings, default_keybindings};
use std::fs;

fn current_settings(core: &common::TestCore) -> Settings {
    serde_json::from_value(core.state_on_disk()["settings"].clone()).unwrap()
}

fn expect_rejected(core: &common::TestCore, settings: Settings, message: &str) {
    let disk_before = fs::read(core.state_path()).unwrap();
    core.send(Command::UpdateSettings(settings));
    match core.expect_event() {
        Event::SettingsUpdateFailed(AppError::Invalid(error)) => {
            assert!(error.contains(message), "{error:?} lacks {message:?}")
        }
        other => panic!("expected rejection containing {message:?}, got {other:?}"),
    }
    assert_eq!(
        fs::read(core.state_path()).unwrap(),
        disk_before,
        "rejected settings must not touch disk"
    );
}

#[test]
fn fresh_state_has_every_action_bound_to_its_default() {
    let settings = Settings::default();
    for action in Action::ALL {
        assert!(!settings.keybindings[&action].is_empty(), "{action:?} unbound");
    }
    assert_eq!(
        settings.keybindings[&Action::ToggleFavourite],
        [KeyCombo::plain(Key::Enter)]
    );
    assert_eq!(settings.keybindings[&Action::NewSession], [KeyCombo::command(Key::N)]);
    assert_eq!(
        settings.keybindings[&Action::QuickSwitcher],
        [KeyCombo::command(Key::K)]
    );
    assert_eq!(settings.keybindings[&Action::OpenFolder], [KeyCombo::command(Key::O)]);
}

#[test]
fn rebinding_is_persisted_readably_and_survives_restart() {
    let core = test_core();
    let mut settings = current_settings(&core);
    settings
        .keybindings
        .insert(Action::ToggleFavourite, vec![KeyCombo::plain(Key::F)]);

    core.send(Command::UpdateSettings(settings.clone()));
    assert!(matches!(core.expect_event(), Event::SettingsChanged(saved) if saved == settings));
    assert_eq!(
        core.state_on_disk()["settings"]["keybindings"]["ToggleFavourite"],
        serde_json::json!([{"command": false, "alt": false, "shift": false, "key": "F"}]),
        "state.json stays hand-editable"
    );

    let core = restart(core);
    assert!(matches!(core.expect_event(), Event::Snapshot(state) if state.settings == settings));
}

#[test]
fn combo_bound_to_two_actions_is_rejected() {
    let core = test_core();
    let mut settings = current_settings(&core);
    settings
        .keybindings
        .insert(Action::ToggleFavourite, vec![KeyCombo::plain(Key::ArrowRight)]);
    expect_rejected(&core, settings, "→ is bound to both Toggle favourite and Next photo");
}

#[test]
fn action_without_shortcut_is_rejected() {
    let core = test_core();
    let mut settings = current_settings(&core);
    settings.keybindings.insert(Action::Next, vec![]);
    expect_rejected(&core, settings, "Next photo needs at least one shortcut");
}

#[test]
fn reserved_combo_is_rejected() {
    let core = test_core();
    for reserved in [
        KeyCombo::command(Key::Q),
        KeyCombo::command(Key::W),
        KeyCombo::alt(Key::F4),
        KeyCombo::command(Key::Comma),
    ] {
        let mut settings = current_settings(&core);
        settings.keybindings.insert(Action::First, vec![reserved]);
        expect_rejected(&core, settings, "reserved");
    }
}

#[test]
fn stored_bindings_from_older_and_newer_versions_load() {
    let core = test_core();
    let mut state = core.state_on_disk();
    // Older version: no OpenFolder entry. Newer version: a "Zoom" action this build doesn't know.
    state["settings"]["keybindings"] = serde_json::json!({
        "Next": [{"command": false, "alt": false, "shift": false, "key": "D"}],
        "Zoom": [{"command": false, "alt": false, "shift": false, "key": "Z"}],
    });
    fs::write(core.state_path(), serde_json::to_vec(&state).unwrap()).unwrap();

    let core = restart(core);
    let Event::Snapshot(snapshot) = core.expect_event() else {
        panic!("expected snapshot")
    };
    let keybindings = snapshot.settings.keybindings;
    assert_eq!(keybindings[&Action::Next], [KeyCombo::plain(Key::D)]);
    assert_eq!(
        keybindings[&Action::OpenFolder],
        default_keybindings()[&Action::OpenFolder]
    );
    assert_eq!(keybindings.len(), Action::ALL.len());
}

#[test]
fn favourites_root_change_affects_only_new_sessions() {
    let core = test_core();
    core.send(Command::CreateSession {
        name: "Old".into(),
        dir: None,
    });
    assert!(matches!(core.expect_event(), Event::SessionsChanged { .. }));
    assert!(matches!(core.expect_event(), Event::FavouritesOfActive { .. }));

    let new_root = core.dir.path().join("drive");
    let mut settings = current_settings(&core);
    settings.favourites_root = new_root.clone();
    core.send(Command::UpdateSettings(settings));
    assert!(matches!(core.expect_event(), Event::SettingsChanged(_)));
    assert!(new_root.is_dir(), "a new root is created up front");
    assert_eq!(
        fs::read_dir(&new_root).unwrap().count(),
        0,
        "the write probe is cleaned up"
    );

    core.send(Command::CreateSession {
        name: "New".into(),
        dir: None,
    });
    let Event::SessionsChanged { sessions, .. } = core.expect_event() else {
        panic!("expected sessions")
    };
    let dir_of = |name: &str| {
        sessions
            .iter()
            .find(|session| session.name == name)
            .unwrap()
            .dir
            .clone()
    };
    assert_eq!(dir_of("Old"), core.favourites_root().join("Old"));
    assert_eq!(dir_of("New"), new_root.join("New"));
}

#[test]
fn relative_favourites_root_is_rejected() {
    let core = test_core();
    let mut settings = current_settings(&core);
    settings.favourites_root = "Pictures/Sorted".into();
    expect_rejected(&core, settings, "not a full path");
}

#[cfg(unix)]
#[test]
fn unwritable_favourites_root_is_rejected() {
    use std::os::unix::fs::PermissionsExt;
    let core = test_core();
    let locked = core.dir.path().join("locked");
    fs::create_dir(&locked).unwrap();
    fs::set_permissions(&locked, fs::Permissions::from_mode(0o555)).unwrap();

    let mut settings = current_settings(&core);
    settings.favourites_root = locked.clone();
    expect_rejected(&core, settings, "Can't save into");
    fs::set_permissions(&locked, fs::Permissions::from_mode(0o755)).unwrap();
}
