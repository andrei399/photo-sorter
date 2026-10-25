//! Whole flows against fake provider APIs: favourite in the core → bytes arrive at every enabled provider →
//! remote ids land in state.json. No keychain, real browser or internet.

mod common;

use base64::Engine;
use base64::engine::general_purpose::{STANDARD, URL_SAFE_NO_PAD};
use common::{
    MemorySecrets, cloud_config_at, create_session, photo_folder_with, rename_to_trashed, test_core_with, toggle,
};
use photo_sorter::core::{Command, Event};
use photo_sorter::integrations::cloud::CloudProblem;
use photo_sorter::models::cloud::{CloudAccount, CloudProvider, WebDavServer};
use photo_sorter::models::state::PersistedState;
use photo_sorter::processor::cloud_sync::CloudStatus;
use serde_json::{Value, json};
use wiremock::matchers::{body_partial_json, body_string_contains, header, method, path, path_regex};
use wiremock::{Mock, MockServer, Request, ResponseTemplate};

fn account(provider: CloudProvider, enabled: bool, webdav: Option<WebDavServer>) -> CloudAccount {
    CloudAccount {
        provider,
        account_label: format!("{} account", provider.label()),
        enabled,
        webdav,
    }
}

fn webdav_server(server: &MockServer) -> WebDavServer {
    WebDavServer {
        url: format!("{}/dav/ana/", server.uri()),
        username: "ana".into(),
    }
}

fn status_of(event: &Event, provider: CloudProvider) -> Option<&CloudStatus> {
    match event {
        Event::CloudStatusChanged(statuses) => statuses.iter().find(|status| status.provider == provider),
        _ => None,
    }
}

fn remote_ids(state: &Value, provider: &str) -> Value {
    state["sessions"][0]["remote_file_ids"][provider].clone()
}

async fn requests_to(server: &MockServer, http_method: &str, path_prefix: &str) -> Vec<Request> {
    server
        .received_requests()
        .await
        .unwrap()
        .into_iter()
        .filter(|request| request.method.as_str() == http_method && request.url.path().starts_with(path_prefix))
        .collect()
}

#[tokio::test(flavor = "multi_thread")]
async fn google_drive_sign_in_uploads_favourites_and_trashes_unfavourited_ones() {
    let server = MockServer::start().await;
    let id_token = format!(
        "header.{}.signature",
        URL_SAFE_NO_PAD.encode(json!({ "email": "ana@gmail.com" }).to_string())
    );
    Mock::given(method("POST"))
        .and(path("/google/token"))
        .and(body_string_contains("grant_type=authorization_code"))
        .and(body_string_contains("code=test-code"))
        .and(body_string_contains("code_verifier="))
        .and(body_string_contains("client_secret=google-secret"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "access_token": "google-access", "expires_in": 3600, "refresh_token": "google-refresh", "id_token": id_token,
        })))
        .expect(1)
        .mount(&server)
        .await;
    Mock::given(method("POST"))
        .and(path("/google/token"))
        .and(body_string_contains("grant_type=refresh_token"))
        .and(body_string_contains("refresh_token=google-refresh"))
        .respond_with(
            ResponseTemplate::new(200).set_body_json(json!({ "access_token": "google-access", "expires_in": 3600 })),
        )
        .expect(1)
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/google/drive/v3/files"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({ "files": [] })))
        .mount(&server)
        .await;
    for (name, id) in [("Photo Sorter", "app-folder"), ("Wedding", "session-folder")] {
        Mock::given(method("POST"))
            .and(path("/google/drive/v3/files"))
            .and(body_partial_json(
                json!({ "name": name, "mimeType": "application/vnd.google-apps.folder" }),
            ))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({ "id": id })))
            .expect(1)
            .mount(&server)
            .await;
    }
    Mock::given(method("POST"))
        .and(path("/google/upload/drive/v3/files"))
        .and(header("authorization", "Bearer google-access"))
        .and(body_partial_json(
            json!({ "name": "IMG_1.jpg", "parents": ["session-folder"] }),
        ))
        .respond_with(
            ResponseTemplate::new(200).insert_header("Location", format!("{}/google/upload-session/1", server.uri())),
        )
        .expect(1)
        .mount(&server)
        .await;
    Mock::given(method("PUT"))
        .and(path("/google/upload-session/1"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({ "id": "drive-file-1" })))
        .expect(1)
        .mount(&server)
        .await;
    Mock::given(method("PATCH"))
        .and(path("/google/drive/v3/files/drive-file-1"))
        .and(body_partial_json(json!({ "trashed": true })))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({})))
        .expect(1)
        .mount(&server)
        .await;
    let secrets = MemorySecrets::with(&[]);
    let core = test_core_with(
        rename_to_trashed,
        |_| {},
        cloud_config_at(&server.uri(), secrets.clone()),
    );

    core.send(Command::ConnectCloud(CloudProvider::GoogleDrive));
    core.wait_for_event("Google Drive connected", |event| {
        status_of(event, CloudProvider::GoogleDrive)
            .and_then(|status| status.account.as_ref())
            .is_some_and(|account| account.account_label == "ana@gmail.com" && account.enabled)
    });
    assert_eq!(
        secrets.get(CloudProvider::GoogleDrive).as_deref(),
        Some("google-refresh")
    );

    let photo_bytes = b"jpeg bytes of the first dance".to_vec();
    let photos = photo_folder_with(&[("IMG_1.jpg", &photo_bytes)]);
    create_session(&core, "Wedding");
    toggle(&core, &photos.path().join("IMG_1.jpg"));
    core.wait_for_state("uploaded to Drive", |state| {
        remote_ids(state, "GoogleDrive") == json!({ "IMG_1.jpg": "drive-file-1" })
    });
    let uploads = requests_to(&server, "PUT", "/google/upload-session").await;
    assert_eq!(uploads[0].body, photo_bytes);

    toggle(&core, &photos.path().join("IMG_1.jpg"));
    core.wait_for_state("trashed in Drive", |state| {
        remote_ids(state, "GoogleDrive") == json!({})
    });

    core.send(Command::DisconnectCloud(CloudProvider::GoogleDrive));
    core.wait_for_event("Google Drive disconnected", |event| {
        status_of(event, CloudProvider::GoogleDrive).is_some_and(|status| status.account.is_none())
    });
    assert_eq!(core.state_on_disk()["cloud_accounts"], json!([]));
    // The keychain entry is removed off the core thread.
    std::thread::sleep(std::time::Duration::from_millis(200));
    assert_eq!(secrets.get(CloudProvider::GoogleDrive), None);
}

#[tokio::test(flavor = "multi_thread")]
async fn every_enabled_provider_gets_every_file_of_a_favourite() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/onedrive/token"))
        .and(body_string_contains("refresh_token=onedrive-refresh"))
        .and(body_string_contains("scope=Files.ReadWrite.AppFolder"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "access_token": "onedrive-access", "expires_in": 3600, "refresh_token": "onedrive-refresh-rotated",
        })))
        .expect(1)
        .mount(&server)
        .await;
    Mock::given(method("POST"))
        .and(path_regex(
            r"^/onedrive/me/drive/special/approot:/Nunt%C4%83%20%C8%98tefan/IMG_1\.(jpg|CR3):/createUploadSession$",
        ))
        .and(header("authorization", "Bearer onedrive-access"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_json(json!({ "uploadUrl": format!("{}/onedrive/upload-session", server.uri()) })),
        )
        .expect(2)
        .mount(&server)
        .await;
    Mock::given(method("PUT"))
        .and(path("/onedrive/upload-session"))
        .respond_with(ResponseTemplate::new(201).set_body_json(json!({ "id": "onedrive-item" })))
        .expect(3)
        .mount(&server)
        .await;
    Mock::given(method("POST"))
        .and(path("/dropbox/token"))
        .respond_with(
            ResponseTemplate::new(200).set_body_json(json!({ "access_token": "dropbox-access", "expires_in": 14400 })),
        )
        .expect(1)
        .mount(&server)
        .await;
    Mock::given(method("POST"))
        .and(path("/dropbox/2/files/upload"))
        .and(header("authorization", "Bearer dropbox-access"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({ "id": "id:dropbox-file" })))
        .expect(2)
        .mount(&server)
        .await;
    Mock::given(method("MKCOL"))
        .and(path_regex("^/dav/ana/Photo%20Sorter"))
        .respond_with(ResponseTemplate::new(201))
        .expect(2)
        .mount(&server)
        .await;
    Mock::given(method("PUT"))
        .and(path_regex("^/dav/ana/Photo%20Sorter/Nunt%C4%83%20%C8%98tefan/IMG_1"))
        .and(header(
            "authorization",
            format!("Basic {}", STANDARD.encode("ana:webdav-password")).as_str(),
        ))
        .respond_with(ResponseTemplate::new(201))
        .expect(2)
        .mount(&server)
        .await;
    // Connected but switched off: it must not receive anything.
    Mock::given(path_regex("^/google/"))
        .respond_with(ResponseTemplate::new(500))
        .expect(0)
        .mount(&server)
        .await;
    let secrets = MemorySecrets::with(&[
        (CloudProvider::GoogleDrive, "google-refresh"),
        (CloudProvider::OneDrive, "onedrive-refresh"),
        (CloudProvider::Dropbox, "dropbox-refresh"),
        (CloudProvider::WebDav, "webdav-password"),
    ]);
    let webdav = webdav_server(&server);
    let core = test_core_with(
        rename_to_trashed,
        |state: &mut PersistedState| {
            state.cloud_accounts = vec![
                account(CloudProvider::GoogleDrive, false, None),
                account(CloudProvider::OneDrive, true, None),
                account(CloudProvider::Dropbox, true, None),
                account(CloudProvider::WebDav, true, Some(webdav)),
            ];
        },
        cloud_config_at(&server.uri(), secrets.clone()),
    );
    let jpeg = b"jpeg".to_vec();
    let raw = vec![7u8; 11 * 1024 * 1024];
    let photos = photo_folder_with(&[("IMG_1.jpg", &jpeg), ("IMG_1.CR3", &raw)]);

    create_session(&core, "Nuntă Ștefan");
    toggle(&core, &photos.path().join("IMG_1.jpg"));
    let state = core.wait_for_state("both files on all three providers", |state| {
        ["OneDrive", "Dropbox", "WebDav"].iter().all(|provider| {
            remote_ids(state, provider)
                .as_object()
                .is_some_and(|ids| ids.len() == 2)
        })
    });

    assert_eq!(remote_ids(&state, "Dropbox")["IMG_1.CR3"], "id:dropbox-file");
    assert_eq!(
        remote_ids(&state, "WebDav")["IMG_1.CR3"],
        "Photo Sorter/Nuntă Ștefan/IMG_1.CR3"
    );
    assert_eq!(remote_ids(&state, "GoogleDrive"), Value::Null);
    assert_eq!(
        secrets.get(CloudProvider::OneDrive).as_deref(),
        Some("onedrive-refresh-rotated")
    );
    // The 11 MiB RAW goes to OneDrive in two chunks; the JPEG in one.
    let mut onedrive_ranges: Vec<String> = requests_to(&server, "PUT", "/onedrive/upload-session")
        .await
        .iter()
        .map(|request| request.headers["content-range"].to_str().unwrap().to_owned())
        .collect();
    onedrive_ranges.sort();
    assert_eq!(
        onedrive_ranges,
        [
            "bytes 0-10485759/11534336",
            "bytes 0-3/4",
            "bytes 10485760-11534335/11534336"
        ]
    );
    let dropbox_arguments: Vec<String> = requests_to(&server, "POST", "/dropbox/2/files/upload")
        .await
        .iter()
        .map(|request| request.headers["dropbox-api-arg"].to_str().unwrap().to_owned())
        .collect();
    assert!(
        dropbox_arguments
            .iter()
            .any(|arguments| arguments.contains(r#""path":"/Nunt\u0103 \u0218tefan/IMG_1.CR3""#)),
        "{dropbox_arguments:?}"
    );
    let webdav_raw = requests_to(
        &server,
        "PUT",
        "/dav/ana/Photo%20Sorter/Nunt%C4%83%20%C8%98tefan/IMG_1.CR3",
    )
    .await;
    assert_eq!(webdav_raw[0].body, raw);
}

#[tokio::test(flavor = "multi_thread")]
async fn server_errors_are_retried_until_the_upload_lands() {
    let server = MockServer::start().await;
    Mock::given(method("MKCOL"))
        .respond_with(ResponseTemplate::new(405))
        .mount(&server)
        .await;
    Mock::given(method("PUT"))
        .respond_with(ResponseTemplate::new(503))
        .up_to_n_times(2)
        .with_priority(1)
        .expect(2)
        .mount(&server)
        .await;
    Mock::given(method("PUT"))
        .respond_with(ResponseTemplate::new(201))
        .expect(1)
        .mount(&server)
        .await;
    let webdav = webdav_server(&server);
    let core = test_core_with(
        rename_to_trashed,
        |state: &mut PersistedState| state.cloud_accounts = vec![account(CloudProvider::WebDav, true, Some(webdav))],
        cloud_config_at(&server.uri(), MemorySecrets::with(&[(CloudProvider::WebDav, "pw")])),
    );
    let photos = photo_folder_with(&[("IMG_1.jpg", b"x")]);

    create_session(&core, "Marathon");
    toggle(&core, &photos.path().join("IMG_1.jpg"));

    core.wait_for_event("retrying shown", |event| {
        status_of(event, CloudProvider::WebDav)
            .is_some_and(|status| matches!(status.problem, Some(CloudProblem::Retrying(_))) && status.pending == 1)
    });
    core.wait_for_event("recovered", |event| {
        status_of(event, CloudProvider::WebDav).is_some_and(|status| status.problem.is_none() && status.pending == 0)
    });
    assert_eq!(
        remote_ids(&core.state_on_disk(), "WebDav"),
        json!({ "IMG_1.jpg": "Photo Sorter/Marathon/IMG_1.jpg" })
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn revoked_access_asks_to_reconnect_and_uploads_nothing() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/dropbox/token"))
        .respond_with(ResponseTemplate::new(400).set_body_json(json!({ "error": "invalid_grant" })))
        .mount(&server)
        .await;
    Mock::given(path("/dropbox/2/files/upload"))
        .respond_with(ResponseTemplate::new(200))
        .expect(0)
        .mount(&server)
        .await;
    let core = test_core_with(
        rename_to_trashed,
        |state: &mut PersistedState| state.cloud_accounts = vec![account(CloudProvider::Dropbox, true, None)],
        cloud_config_at(
            &server.uri(),
            MemorySecrets::with(&[(CloudProvider::Dropbox, "revoked")]),
        ),
    );
    let photos = photo_folder_with(&[("IMG_1.jpg", b"x"), ("IMG_2.jpg", b"y")]);

    create_session(&core, "Gala");
    toggle(&core, &photos.path().join("IMG_1.jpg"));
    toggle(&core, &photos.path().join("IMG_2.jpg"));

    core.wait_for_event("reconnect asked", |event| {
        status_of(event, CloudProvider::Dropbox)
            .is_some_and(|status| status.problem == Some(CloudProblem::NeedsReconnect) && status.pending == 2)
    });
}

#[tokio::test(flavor = "multi_thread")]
async fn webdav_connect_rejects_a_wrong_password_and_keeps_the_right_one() {
    let server = MockServer::start().await;
    let basic = |password: &str| format!("Basic {}", STANDARD.encode(format!("ana:{password}")));
    Mock::given(method("PROPFIND"))
        .and(path("/dav/ana"))
        .and(header("authorization", basic("right").as_str()))
        .and(header("depth", "0"))
        .respond_with(ResponseTemplate::new(207))
        .mount(&server)
        .await;
    Mock::given(method("PROPFIND"))
        .respond_with(ResponseTemplate::new(401))
        .mount(&server)
        .await;
    let secrets = MemorySecrets::with(&[]);
    let core = test_core_with(
        rename_to_trashed,
        |_| {},
        cloud_config_at(&server.uri(), secrets.clone()),
    );
    let connect = |password: &str| Command::ConnectWebDav {
        server: webdav_server(&server),
        password: password.into(),
    };

    core.send(connect("wrong"));
    let failed = core.wait_for_event("wrong password refused", |event| {
        matches!(event, Event::CloudConnectFailed { .. })
    });
    assert!(
        matches!(failed, Event::CloudConnectFailed { provider: CloudProvider::WebDav, error } if error == "Wrong username or password")
    );
    assert_eq!(secrets.get(CloudProvider::WebDav), None);

    core.send(connect("right"));
    let host = reqwest::Url::parse(&server.uri())
        .unwrap()
        .host_str()
        .unwrap()
        .to_owned();
    core.wait_for_event("connected", |event| {
        status_of(event, CloudProvider::WebDav)
            .and_then(|status| status.account.as_ref())
            .is_some_and(|account| account.account_label == format!("ana @ {host}"))
    });
    assert_eq!(secrets.get(CloudProvider::WebDav).as_deref(), Some("right"));
}

#[tokio::test(flavor = "multi_thread")]
async fn unfavourited_during_upload_is_deleted_once_the_upload_lands() {
    let server = MockServer::start().await;
    Mock::given(method("MKCOL"))
        .respond_with(ResponseTemplate::new(201))
        .mount(&server)
        .await;
    Mock::given(method("PUT"))
        .respond_with(ResponseTemplate::new(201).set_delay(std::time::Duration::from_millis(500)))
        .expect(1)
        .mount(&server)
        .await;
    Mock::given(method("DELETE"))
        .and(path("/dav/ana/Photo%20Sorter/Concert/IMG_1.jpg"))
        .respond_with(ResponseTemplate::new(204))
        .expect(1)
        .mount(&server)
        .await;
    let webdav = webdav_server(&server);
    let core = test_core_with(
        rename_to_trashed,
        |state: &mut PersistedState| state.cloud_accounts = vec![account(CloudProvider::WebDav, true, Some(webdav))],
        cloud_config_at(&server.uri(), MemorySecrets::with(&[(CloudProvider::WebDav, "pw")])),
    );
    let photos = photo_folder_with(&[("IMG_1.jpg", b"x")]);
    create_session(&core, "Concert");

    toggle(&core, &photos.path().join("IMG_1.jpg"));
    while requests_to(&server, "PUT", "/dav/").await.is_empty() {
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;
    }
    toggle(&core, &photos.path().join("IMG_1.jpg"));

    core.wait_for_state("late upload deleted", |state| {
        state["sessions"][0]["favourites"] == json!({}) && remote_ids(state, "WebDav") == json!({})
    });
}
