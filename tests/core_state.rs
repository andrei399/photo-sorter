use photo_sorter::core::{Event, spawn_core};
use photo_sorter::models::state::PersistedState;
use std::fs;

#[test]
fn fresh_install_emits_default_snapshot_without_writing() {
    let dir = tempfile::tempdir().unwrap();
    let state_path = dir.path().join("state.json");
    let handle = spawn_core(state_path.clone(), || {}).unwrap();
    let event = handle.events.recv_timeout(std::time::Duration::from_secs(5)).unwrap();
    assert!(matches!(event, Event::Snapshot(state) if state == PersistedState::default()));
    assert!(!state_path.exists());
}

#[test]
fn corrupt_state_is_quarantined_not_overwritten() {
    let dir = tempfile::tempdir().unwrap();
    let state_path = dir.path().join("state.json");
    fs::write(&state_path, b"{garbage").unwrap();

    let handle = spawn_core(state_path.clone(), || {}).unwrap();
    let event = handle.events.recv_timeout(std::time::Duration::from_secs(5)).unwrap();

    assert!(matches!(event, Event::Snapshot(state) if state == PersistedState::default()));
    let quarantined: Vec<_> = fs::read_dir(dir.path())
        .unwrap()
        .filter_map(Result::ok)
        .filter(|entry| entry.file_name().to_string_lossy().starts_with("state.json.corrupt-"))
        .collect();
    assert_eq!(quarantined.len(), 1);
    assert_eq!(fs::read(quarantined[0].path()).unwrap(), b"{garbage");
}

#[test]
fn unknown_fields_from_newer_versions_are_ignored() {
    let dir = tempfile::tempdir().unwrap();
    let state_path = dir.path().join("state.json");
    fs::write(
        &state_path,
        br#"{"schema_version": 0, "last_opened_folder": "/photos", "future_field": 1}"#,
    )
    .unwrap();

    let handle = spawn_core(state_path, || {}).unwrap();
    let event = handle.events.recv_timeout(std::time::Duration::from_secs(5)).unwrap();

    assert!(matches!(event, Event::Snapshot(state) if state.last_opened_folder.as_deref() == Some("/photos".as_ref())));
}
