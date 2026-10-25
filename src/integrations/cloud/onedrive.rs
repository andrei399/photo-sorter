//! Microsoft Graph with `Files.ReadWrite.AppFolder`: the app sees only `OneDrive/Apps/Photo Sorter`, which Graph
//! calls `approot`. Layout: `Apps/Photo Sorter/<session>/<file>`.

use super::{CloudError, ProviderClient, check};
use reqwest::header::{CONTENT_LENGTH, CONTENT_RANGE};
use reqwest::{StatusCode, Url};
use serde::Deserialize;
use serde_json::json;
use std::path::Path;
use tokio::io::AsyncReadExt;

/// Graph wants upload chunks in multiples of 320 KiB, at most 60 MiB; 10 MiB keeps memory small with 3 in flight.
const CHUNK_BYTES: u64 = 32 * 320 * 1024;

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct UploadSession {
    upload_url: String,
}

#[derive(Deserialize)]
struct DriveItem {
    id: String,
}

pub(super) async fn upload(
    client: &ProviderClient,
    token: &str,
    remote_folder: &str,
    local: &Path,
    file_name: &str,
) -> Result<String, CloudError> {
    let mut file = tokio::fs::File::open(local).await?;
    let length = file.metadata().await?.len();
    if length == 0 {
        return Err(CloudError::Rejected(format!("{file_name} is empty")));
    }
    // Upload sessions create missing folders on the way and have no size limit.
    let mut session_url = graph_url(client, &["me", "drive", "special", "approot:", remote_folder])?;
    session_url
        .path_segments_mut()
        .expect("Graph URLs have a path")
        .extend([format!("{file_name}:").as_str(), "createUploadSession"]);
    let session: UploadSession = check(
        client
            .http
            .post(session_url)
            .bearer_auth(token)
            .json(&json!({ "item": { "@microsoft.graph.conflictBehavior": "rename" } }))
            .send()
            .await?,
    )
    .await?
    .json()
    .await?;

    let mut offset = 0;
    loop {
        let chunk_length = CHUNK_BYTES.min(length - offset);
        let mut chunk = vec![0; chunk_length as usize];
        file.read_exact(&mut chunk).await?;
        // The upload URL is pre-authorized; Graph rejects it with an Authorization header.
        let response = check(
            client
                .http
                .put(&session.upload_url)
                .header(CONTENT_LENGTH, chunk_length)
                .header(
                    CONTENT_RANGE,
                    format!("bytes {offset}-{}/{length}", offset + chunk_length - 1),
                )
                .body(chunk)
                .send()
                .await?,
        )
        .await?;
        offset += chunk_length;
        if offset == length {
            return Ok(response.json::<DriveItem>().await?.id);
        }
    }
}

/// Goes to the OneDrive recycle bin.
pub(super) async fn delete(client: &ProviderClient, token: &str, item_id: &str) -> Result<(), CloudError> {
    let response = client
        .http
        .delete(graph_url(client, &["me", "drive", "items", item_id])?)
        .bearer_auth(token)
        .send()
        .await?;
    if response.status() == StatusCode::NOT_FOUND {
        return Ok(());
    }
    check(response).await.map(drop)
}

/// Segments are percent-encoded, so session names like "Nuntă #2" stay one path segment.
fn graph_url(client: &ProviderClient, segments: &[&str]) -> Result<Url, CloudError> {
    let mut url = Url::parse(&client.endpoints().api).map_err(|error| CloudError::Rejected(error.to_string()))?;
    url.path_segments_mut()
        .map_err(|()| CloudError::Rejected("Graph URL has no path".into()))?
        .pop_if_empty()
        .extend(segments);
    Ok(url)
}
