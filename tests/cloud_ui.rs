//! The cloud UI end to end: the bottom panel's upload chip and the Settings › Cloud uploads cards, against a fake
//! WebDAV server.

mod common;

use common::{MemorySecrets, app_harness_with, cloud_config_at, jpeg_folder, run_until, run_until_label};
use egui::{Key, Role};
use egui_kittest::kittest::Queryable;
use photo_sorter::app::SettingsPage;
use photo_sorter::core::Command;
use photo_sorter::models::cloud::{CloudAccount, CloudProvider, WebDavServer};
use photo_sorter::models::state::PersistedState;
use serde_json::{Value, json};
use std::path::Path;
use wiremock::matchers::method;
use wiremock::{Mock, MockServer, ResponseTemplate};

const WEBDAV_SWITCH: &str = "Upload to Nextcloud / WebDAV";

fn state_on_disk(state_dir: &Path) -> Value {
    serde_json::from_slice(&std::fs::read(state_dir.join("state.json")).unwrap()).unwrap()
}

fn click(harness: &mut common::AppHarness, role: Role, label: &str) {
    harness.get_by_role_and_label(role, label).click();
    harness.run_steps(2);
}

#[tokio::test(flavor = "multi_thread")]
async fn upload_chip_follows_the_upload_and_its_switch_pauses_the_service() {
    let server = MockServer::start().await;
    Mock::given(method("MKCOL"))
        .respond_with(ResponseTemplate::new(201))
        .mount(&server)
        .await;
    Mock::given(method("PUT"))
        .respond_with(ResponseTemplate::new(201))
        .expect(1)
        .mount(&server)
        .await;
    let photos = jpeg_folder(2);
    let state_dir = tempfile::tempdir().unwrap();
    let webdav = WebDavServer {
        url: format!("{}/dav/ana/", server.uri()),
        username: "ana".into(),
    };
    let mut harness = app_harness_with(
        state_dir.path(),
        |state: &mut PersistedState| {
            state.cloud_accounts = vec![CloudAccount {
                provider: CloudProvider::WebDav,
                account_label: "ana @ nas".into(),
                enabled: true,
                webdav: Some(webdav),
            }];
        },
        cloud_config_at(&server.uri(), MemorySecrets::with(&[(CloudProvider::WebDav, "pw")])),
    );
    harness.state_mut().send(Command::CreateSession {
        name: "Party".into(),
        dir: None,
    });
    harness
        .state_mut()
        .send(Command::OpenFolder(photos.path().to_path_buf()));
    run_until_label(&mut harness, "IMG_1.jpg · 1 / 2");
    run_until_label(&mut harness, "Uploaded");

    harness.key_press(Key::Enter);
    run_until(&mut harness, "uploaded to WebDAV", |_| {
        state_on_disk(state_dir.path())["sessions"][0]["remote_file_ids"]["WebDav"]
            == json!({ "IMG_1.jpg": "Photo Sorter/Party/IMG_1.jpg" })
    });
    run_until_label(&mut harness, "Uploaded");

    click(&mut harness, Role::Button, "Cloud uploads: Uploaded");
    click(&mut harness, Role::Switch, WEBDAV_SWITCH);
    run_until(&mut harness, "WebDAV paused on disk", |_| {
        state_on_disk(state_dir.path())["cloud_accounts"][0]["enabled"] == json!(false)
    });
    run_until(&mut harness, "chip says uploads are off", |h| {
        h.query_by_role_and_label(Role::Button, "Cloud uploads: Uploads off")
            .is_some()
    });
}

#[tokio::test(flavor = "multi_thread")]
async fn settings_card_connects_a_webdav_server() {
    let server = MockServer::start().await;
    Mock::given(method("PROPFIND"))
        .respond_with(ResponseTemplate::new(207))
        .expect(1)
        .mount(&server)
        .await;
    let state_dir = tempfile::tempdir().unwrap();
    let secrets = MemorySecrets::with(&[]);
    let mut harness = app_harness_with(
        state_dir.path(),
        |_| {},
        cloud_config_at(&server.uri(), secrets.clone()),
    );
    run_until(&mut harness, "core started", |h| h.state().state.is_some());
    harness.state_mut().open_settings_at(SettingsPage::Cloud);
    run_until(&mut harness, "four provider cards", |h| {
        CloudProvider::ALL.iter().all(|provider| {
            h.query_by_role_and_label(Role::Button, &format!("Connect… {}", provider.label()))
                .is_some()
        })
    });

    click(&mut harness, Role::Button, "Connect… Nextcloud / WebDAV");
    for (role, field, text) in [
        (Role::TextInput, "Server address", format!("{}/dav/ana/", server.uri())),
        (Role::TextInput, "Username", "ana".to_owned()),
        (Role::PasswordInput, "App password", "app-password".to_owned()),
    ] {
        harness.get_by_role_and_label(role, field).focus();
        harness.run_steps(1);
        harness.get_by_role_and_label(role, field).type_text(&text);
        harness.run_steps(1);
    }
    click(&mut harness, Role::Button, "Connect");

    let host = reqwest::Url::parse(&server.uri())
        .unwrap()
        .host_str()
        .unwrap()
        .to_owned();
    run_until_label(&mut harness, &format!("ana @ {host}"));
    assert!(harness.query_by_role_and_label(Role::Switch, WEBDAV_SWITCH).is_some());
    assert_eq!(secrets.get(CloudProvider::WebDav).as_deref(), Some("app-password"));
}
