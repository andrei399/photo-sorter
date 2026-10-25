//! Dropbox API v2 as an "App folder" app: it sees only `Dropbox/Apps/Photo Sorter`, which is `/` to it.
//! Layout: `Apps/Photo Sorter/<session>/<file>`.

use super::{CloudError, ProviderClient, ProviderEndpoints, check, classify_failure, file_body};
use reqwest::StatusCode;
use reqwest::header::{CONTENT_LENGTH, CONTENT_TYPE};
use serde::Deserialize;
use serde_json::json;
use std::path::Path;

/// `files/upload` takes at most 150 MB in one request.
// ponytail: single-request uploads only; add upload sessions if medium-format RAWs over 150 MB show up.
const MAX_SINGLE_UPLOAD_BYTES: u64 = 150 * 1024 * 1024;

#[derive(Deserialize)]
struct FileMetadata {
    id: String,
}

#[derive(Deserialize)]
struct Account {
    email: String,
}

pub(super) async fn upload(
    client: &ProviderClient,
    token: &str,
    remote_folder: &str,
    local: &Path,
    file_name: &str,
) -> Result<String, CloudError> {
    let (body, length) = file_body(local).await?;
    if length > MAX_SINGLE_UPLOAD_BYTES {
        return Err(CloudError::Rejected(format!(
            "{file_name} is over 150 MB, too large for Dropbox uploads"
        )));
    }
    let arguments = json!({
        "path": format!("/{remote_folder}/{file_name}"),
        "mode": "add",
        "autorename": true,
        "mute": true,
    });
    let uploaded: FileMetadata = check(
        client
            .http
            .post(format!("{}/2/files/upload", client.endpoints().content))
            .bearer_auth(token)
            .header("Dropbox-API-Arg", ascii_json(&arguments))
            .header(CONTENT_TYPE, "application/octet-stream")
            .header(CONTENT_LENGTH, length)
            .body(body)
            .send()
            .await?,
    )
    .await?
    .json()
    .await?;
    Ok(uploaded.id)
}

/// Deleted files stay restorable from Dropbox's "Deleted files" view.
pub(super) async fn delete(client: &ProviderClient, token: &str, file_id: &str) -> Result<(), CloudError> {
    let response = client
        .http
        .post(format!("{}/2/files/delete_v2", client.endpoints().api))
        .bearer_auth(token)
        .json(&json!({ "path": file_id }))
        .send()
        .await?;
    if response.status() != StatusCode::CONFLICT {
        return check(response).await.map(drop);
    }
    let body = response.text().await.unwrap_or_default();
    if body.contains("not_found") {
        return Ok(());
    }
    Err(classify_failure(StatusCode::CONFLICT, &body))
}

pub(super) async fn account_email(
    http: &reqwest::Client,
    endpoints: &ProviderEndpoints,
    token: &str,
) -> Result<String, CloudError> {
    let account: Account = check(
        http.post(format!("{}/2/users/get_current_account", endpoints.api))
            .bearer_auth(token)
            .send()
            .await?,
    )
    .await?
    .json()
    .await?;
    Ok(account.email)
}

/// HTTP headers carry ASCII only, so Dropbox wants everything else escaped: "Nuntă" → "Nunt\u0103".
fn ascii_json(value: &serde_json::Value) -> String {
    let mut escaped = String::new();
    for c in value.to_string().chars() {
        if c.is_ascii() {
            escaped.push(c);
        } else {
            for unit in c.encode_utf16(&mut [0; 2]) {
                escaped.push_str(&format!("\\u{unit:04x}"));
            }
        }
    }
    escaped
}
