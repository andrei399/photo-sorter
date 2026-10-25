//! Any WebDAV server: Nextcloud, ownCloud, a Synology or QNAP NAS. Signs in with a username and (app) password.
//! Layout: `<server url>/Photo Sorter/<session>/<file>`; the remote id is that path relative to the server URL.

use super::{CloudError, ProviderClient, check, file_body};
use crate::models::cloud::WebDavServer;
use reqwest::header::CONTENT_LENGTH;
use reqwest::{Method, StatusCode, Url};
use std::path::Path;

const APP_FOLDER: &str = "Photo Sorter";

pub(super) async fn upload(
    client: &ProviderClient,
    password: &str,
    remote_folder: &str,
    local: &Path,
    file_name: &str,
) -> Result<String, CloudError> {
    let server = server_of(client)?;
    ensure_collections(client, server, password, remote_folder).await?;
    let remote_path = format!("{APP_FOLDER}/{remote_folder}/{file_name}");
    let (body, length) = file_body(local).await?;
    check(
        client
            .http
            .put(url_of(server, &remote_path)?)
            .basic_auth(&server.username, Some(password))
            .header(CONTENT_LENGTH, length)
            .body(body)
            .send()
            .await?,
    )
    .await?;
    Ok(remote_path)
}

/// Nextcloud and ownCloud keep deleted files in their trash bin.
pub(super) async fn delete(client: &ProviderClient, password: &str, remote_path: &str) -> Result<(), CloudError> {
    let server = server_of(client)?;
    let response = client
        .http
        .delete(url_of(server, remote_path)?)
        .basic_auth(&server.username, Some(password))
        .send()
        .await?;
    if response.status() == StatusCode::NOT_FOUND {
        return Ok(());
    }
    check(response).await.map(drop)
}

/// Run before saving the password, so a typo is caught in the dialog instead of on the first upload.
pub(super) async fn check_login(http: &reqwest::Client, server: &WebDavServer, password: &str) -> Result<(), String> {
    let url = url_of(server, "").map_err(|error| error.to_string())?;
    let response = http
        .request(Method::from_bytes(b"PROPFIND").expect("valid method"), url)
        .basic_auth(&server.username, Some(password))
        .header("Depth", "0")
        .send()
        .await
        .map_err(|error| format!("Couldn't reach {}: {error}", server.url))?;
    match response.status() {
        status if status.is_success() => Ok(()),
        StatusCode::UNAUTHORIZED => Err("Wrong username or password".into()),
        StatusCode::NOT_FOUND | StatusCode::METHOD_NOT_ALLOWED => Err(format!("{} isn't a WebDAV folder", server.url)),
        status => Err(format!("{} answered {status}", server.url)),
    }
}

/// WebDAV PUT needs the parent folder to exist. 405 means it already does.
async fn ensure_collections(
    client: &ProviderClient,
    server: &WebDavServer,
    password: &str,
    remote_folder: &str,
) -> Result<(), CloudError> {
    let mut known_folders = client.known_folders.lock().await;
    for path in [APP_FOLDER.to_owned(), format!("{APP_FOLDER}/{remote_folder}")] {
        if known_folders.contains_key(&path) {
            continue;
        }
        let response = client
            .http
            .request(
                Method::from_bytes(b"MKCOL").expect("valid method"),
                url_of(server, &path)?,
            )
            .basic_auth(&server.username, Some(password))
            .send()
            .await?;
        if response.status() != StatusCode::METHOD_NOT_ALLOWED {
            check(response).await?;
        }
        known_folders.insert(path, String::new());
    }
    Ok(())
}

fn server_of(client: &ProviderClient) -> Result<&WebDavServer, CloudError> {
    client.account.webdav.as_ref().ok_or(CloudError::AuthExpired)
}

/// Each `/`-separated part of `relative_path` becomes one percent-encoded segment under the server URL.
fn url_of(server: &WebDavServer, relative_path: &str) -> Result<Url, CloudError> {
    let mut url = Url::parse(&server.url).map_err(|error| CloudError::Rejected(format!("{}: {error}", server.url)))?;
    url.path_segments_mut()
        .map_err(|()| CloudError::Rejected(format!("{} can't hold files", server.url)))?
        .pop_if_empty()
        .extend(relative_path.split('/').filter(|segment| !segment.is_empty()));
    Ok(url)
}
