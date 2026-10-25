//! Drive API v3 with the `drive.file` scope: the app sees only the files and folders it created.
//! Layout: `My Drive/Photo Sorter/<session>/<file>`.

use super::{CloudError, ProviderClient, check, file_body};
use reqwest::StatusCode;
use reqwest::header::{CONTENT_LENGTH, LOCATION};
use serde::Deserialize;
use serde_json::json;
use std::path::Path;

const APP_FOLDER: &str = "Photo Sorter";
const FOLDER_MIME_TYPE: &str = "application/vnd.google-apps.folder";

#[derive(Deserialize)]
struct DriveFile {
    id: String,
}

#[derive(Deserialize)]
struct FileList {
    files: Vec<DriveFile>,
}

pub(super) async fn upload(
    client: &ProviderClient,
    token: &str,
    remote_folder: &str,
    local: &Path,
    file_name: &str,
) -> Result<String, CloudError> {
    let folder_id = ensure_session_folder(client, token, remote_folder).await?;
    let (body, length) = file_body(local).await?;
    // Resumable even for one-shot uploads: it's the only upload type without a size limit.
    let started = client
        .http
        .post(format!(
            "{}/upload/drive/v3/files?uploadType=resumable&fields=id",
            client.endpoints().content
        ))
        .bearer_auth(token)
        .header("X-Upload-Content-Length", length)
        .json(&json!({ "name": file_name, "parents": [folder_id] }))
        .send()
        .await?;
    if started.status() == StatusCode::NOT_FOUND {
        // The user deleted the folder in Drive: look it up (or recreate it) on the retry.
        client.known_folders.lock().await.clear();
        return Err(CloudError::Retryable(
            "The Drive folder was deleted; recreating it".into(),
        ));
    }
    let started = check(started).await?;
    let upload_url = started
        .headers()
        .get(LOCATION)
        .and_then(|location| location.to_str().ok())
        .ok_or_else(|| CloudError::Retryable("Drive didn't start the upload".into()))?
        .to_owned();
    let uploaded: DriveFile = check(
        client
            .http
            .put(upload_url)
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

/// Moves the file to Drive's trash, where the user can still restore it for 30 days.
pub(super) async fn delete(client: &ProviderClient, token: &str, file_id: &str) -> Result<(), CloudError> {
    let response = client
        .http
        .patch(format!("{}/drive/v3/files/{file_id}", client.endpoints().api))
        .bearer_auth(token)
        .json(&json!({ "trashed": true }))
        .send()
        .await?;
    if response.status() == StatusCode::NOT_FOUND {
        return Ok(());
    }
    check(response).await.map(drop)
}

/// Drive allows duplicate names, so folders are found by name before creating them; ids are cached per run.
async fn ensure_session_folder(
    client: &ProviderClient,
    token: &str,
    remote_folder: &str,
) -> Result<String, CloudError> {
    let mut known_folders = client.known_folders.lock().await;
    let mut parent_id = "root".to_owned();
    for (name, path) in [
        (APP_FOLDER, APP_FOLDER.to_owned()),
        (remote_folder, format!("{APP_FOLDER}/{remote_folder}")),
    ] {
        parent_id = match known_folders.get(&path) {
            Some(id) => id.clone(),
            None => {
                let id = match find_folder(client, token, name, &parent_id).await? {
                    Some(id) => id,
                    None => create_folder(client, token, name, &parent_id).await?,
                };
                known_folders.insert(path, id.clone());
                id
            }
        };
    }
    Ok(parent_id)
}

async fn find_folder(
    client: &ProviderClient,
    token: &str,
    name: &str,
    parent_id: &str,
) -> Result<Option<String>, CloudError> {
    let escaped_name = name.replace('\\', "\\\\").replace('\'', "\\'");
    let query = format!(
        "name = '{escaped_name}' and '{parent_id}' in parents and mimeType = '{FOLDER_MIME_TYPE}' and trashed = false"
    );
    let found: FileList = check(
        client
            .http
            .get(format!("{}/drive/v3/files", client.endpoints().api))
            .bearer_auth(token)
            .query(&[("q", query.as_str()), ("fields", "files(id)"), ("spaces", "drive")])
            .send()
            .await?,
    )
    .await?
    .json()
    .await?;
    Ok(found.files.into_iter().next().map(|folder| folder.id))
}

async fn create_folder(
    client: &ProviderClient,
    token: &str,
    name: &str,
    parent_id: &str,
) -> Result<String, CloudError> {
    let created: DriveFile = check(
        client
            .http
            .post(format!("{}/drive/v3/files?fields=id", client.endpoints().api))
            .bearer_auth(token)
            .json(&json!({ "name": name, "mimeType": FOLDER_MIME_TYPE, "parents": [parent_id] }))
            .send()
            .await?,
    )
    .await?
    .json()
    .await?;
    Ok(created.id)
}
