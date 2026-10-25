//! OAuth 2.0 for installed apps: PKCE, the provider's page in the system browser, and a one-shot loopback server
//! that receives the code. Nothing of ours sits in between: tokens go straight from the provider to this machine.

use super::{CloudError, ProviderEndpoints, check};
use crate::models::cloud::CloudProvider;
use base64::Engine;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use reqwest::Url;
use serde::Deserialize;
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::time::Duration;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;
use uuid::Uuid;

const SIGN_IN_TIMEOUT: Duration = Duration::from_secs(5 * 60);
/// Dropbox only accepts redirect URIs registered verbatim, port included; the others take any loopback port.
const DROPBOX_REDIRECT_PORT: u16 = 53682;
const SIGNED_IN_PAGE: &str = "<!doctype html><meta charset=utf-8><title>Photo Sorter</title>\
    <body style=\"font:16px system-ui;background:#121212;color:#ececec;display:grid;place-items:center;height:90vh\">\
    <p>Connected. You can close this tab and return to Photo Sorter.</p>";
const SIGN_IN_FAILED_PAGE: &str = "<!doctype html><meta charset=utf-8><title>Photo Sorter</title>\
    <body style=\"font:16px system-ui;background:#121212;color:#ececec;display:grid;place-items:center;height:90vh\">\
    <p>Sign-in didn't complete. Return to Photo Sorter to try again.</p>";

#[derive(Debug, Deserialize)]
pub(super) struct Tokens {
    pub access_token: String,
    #[serde(default = "one_hour")]
    pub expires_in: u64,
    pub refresh_token: Option<String>,
    id_token: Option<String>,
}

fn one_hour() -> u64 {
    3600
}

impl Tokens {
    /// Google's `email` or Microsoft's `preferred_username`. The ID token came over TLS straight from the
    /// provider's token endpoint, so a display-only value needs no signature check.
    pub fn account_label(&self) -> Option<String> {
        let payload = self.id_token.as_deref()?.split('.').nth(1)?;
        let claims: serde_json::Value = serde_json::from_slice(&URL_SAFE_NO_PAD.decode(payload).ok()?).ok()?;
        ["email", "preferred_username"]
            .iter()
            .find_map(|claim| claims.get(claim)?.as_str().map(str::to_owned))
    }
}

/// Least access each provider allows: Google sees only files this app created, Microsoft and Dropbox only the
/// app's own folder (`Apps/Photo Sorter`). Dropbox's are set on the app in its console instead.
fn scopes(provider: CloudProvider) -> Option<&'static str> {
    match provider {
        CloudProvider::GoogleDrive => Some("https://www.googleapis.com/auth/drive.file openid email"),
        CloudProvider::OneDrive => Some("Files.ReadWrite.AppFolder offline_access openid profile"),
        CloudProvider::Dropbox | CloudProvider::WebDav => None,
    }
}

/// What each provider needs to hand out a refresh token.
fn offline_access_params(provider: CloudProvider) -> &'static [(&'static str, &'static str)] {
    match provider {
        CloudProvider::GoogleDrive => &[("access_type", "offline"), ("prompt", "consent")],
        CloudProvider::Dropbox => &[("token_access_type", "offline")],
        CloudProvider::OneDrive | CloudProvider::WebDav => &[],
    }
}

pub(super) async fn sign_in_with_browser(
    http: &reqwest::Client,
    provider: CloudProvider,
    endpoints: &ProviderEndpoints,
    open_browser: fn(&str) -> std::io::Result<()>,
) -> Result<Tokens, String> {
    let client = endpoints
        .client
        .as_ref()
        .ok_or_else(|| format!("{} isn't available in this build", provider.label()))?;
    let port = if provider == CloudProvider::Dropbox {
        DROPBOX_REDIRECT_PORT
    } else {
        0
    };
    let listener = TcpListener::bind(("127.0.0.1", port))
        .await
        .map_err(|error| format!("Couldn't start the sign-in: {error}"))?;
    let port = listener.local_addr().map_err(|error| error.to_string())?.port();
    // Microsoft registers `http://localhost` (any port); the others are registered with the IP literal.
    let host = if provider == CloudProvider::OneDrive {
        "localhost"
    } else {
        "127.0.0.1"
    };
    let redirect_uri = format!("http://{host}:{port}/");
    let verifier = format!("{}{}", Uuid::new_v4().simple(), Uuid::new_v4().simple());
    let challenge = URL_SAFE_NO_PAD.encode(Sha256::digest(verifier.as_bytes()));
    let state = Uuid::new_v4().simple().to_string();

    let mut authorize_url = Url::parse(&endpoints.authorize).map_err(|error| error.to_string())?;
    authorize_url
        .query_pairs_mut()
        .extend_pairs([
            ("client_id", client.id.as_str()),
            ("redirect_uri", &redirect_uri),
            ("response_type", "code"),
            ("code_challenge", &challenge),
            ("code_challenge_method", "S256"),
            ("state", &state),
        ])
        .extend_pairs(scopes(provider).map(|scopes| ("scope", scopes)))
        .extend_pairs(offline_access_params(provider));
    open_browser(authorize_url.as_str()).map_err(|error| format!("Couldn't open the browser: {error}"))?;

    let code = tokio::time::timeout(SIGN_IN_TIMEOUT, receive_code(&listener, &state))
        .await
        .map_err(|_| "Sign-in timed out; please try again".to_owned())??;

    let mut form = vec![
        ("grant_type", "authorization_code"),
        ("code", code.as_str()),
        ("redirect_uri", redirect_uri.as_str()),
        ("client_id", client.id.as_str()),
        ("code_verifier", verifier.as_str()),
    ];
    form.extend(client.secret.as_deref().map(|secret| ("client_secret", secret)));
    let response = http
        .post(&endpoints.token)
        .form(&form)
        .send()
        .await
        .map_err(|error| error.to_string())?;
    check(response)
        .await
        .map_err(|error| format!("{} refused the sign-in: {error}", provider.label()))?
        .json()
        .await
        .map_err(|error| error.to_string())
}

/// Serves the browser's redirect. Requests without our `state` (favicon, stray tabs) are ignored, which also
/// shuts out other local programs guessing the port.
async fn receive_code(listener: &TcpListener, state: &str) -> Result<String, String> {
    loop {
        let (mut stream, _) = listener.accept().await.map_err(|error| error.to_string())?;
        let mut request = vec![0; 8192];
        let Ok(read) = stream.read(&mut request).await else {
            continue;
        };
        let request = String::from_utf8_lossy(&request[..read]);
        let params: HashMap<String, String> = request
            .lines()
            .next()
            .and_then(|request_line| request_line.split(' ').nth(1))
            .and_then(|target| Url::parse(&format!("http://loopback{target}")).ok())
            .map(|url| url.query_pairs().into_owned().collect())
            .unwrap_or_default();
        if params.get("state").map(String::as_str) != Some(state) {
            let _ = stream
                .write_all(b"HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\n\r\n")
                .await;
            continue;
        }
        let (page, outcome) = match params.get("code") {
            Some(code) => (SIGNED_IN_PAGE, Ok(code.clone())),
            None => (
                SIGN_IN_FAILED_PAGE,
                Err(match params.get("error").map(String::as_str) {
                    Some("access_denied") | None => "Sign-in was cancelled".to_owned(),
                    Some(error) => format!("Sign-in failed: {error}"),
                }),
            ),
        };
        let response = format!(
            "HTTP/1.1 200 OK\r\nContent-Type: text/html; charset=utf-8\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{page}",
            page.len()
        );
        let _ = stream.write_all(response.as_bytes()).await;
        return outcome;
    }
}

pub(super) async fn refresh(
    http: &reqwest::Client,
    provider: CloudProvider,
    endpoints: &ProviderEndpoints,
    refresh_token: &str,
) -> Result<Tokens, CloudError> {
    let client = endpoints.client.as_ref().ok_or(CloudError::AuthExpired)?;
    let mut form = vec![
        ("grant_type", "refresh_token"),
        ("refresh_token", refresh_token),
        ("client_id", client.id.as_str()),
    ];
    form.extend(client.secret.as_deref().map(|secret| ("client_secret", secret)));
    // Microsoft's v2 endpoint otherwise answers with a token for a different resource.
    if provider == CloudProvider::OneDrive {
        form.extend(scopes(provider).map(|scopes| ("scope", scopes)));
    }
    let response = http.post(&endpoints.token).form(&form).send().await?;
    // The token endpoint answers 400 `invalid_grant` once the user revoked access or the token expired.
    if matches!(response.status().as_u16(), 400 | 401) {
        return Err(CloudError::AuthExpired);
    }
    Ok(check(response).await?.json().await?)
}
