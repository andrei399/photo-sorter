use serde::{Deserialize, Serialize};

/// Where favourites can be uploaded. Every connected and enabled one receives every favourite.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub enum CloudProvider {
    GoogleDrive,
    OneDrive,
    Dropbox,
    /// Nextcloud, ownCloud, Synology/QNAP NAS or any WebDAV server: the self-hosted, privacy-first option.
    WebDav,
}

impl CloudProvider {
    pub const ALL: [Self; 4] = [Self::GoogleDrive, Self::OneDrive, Self::Dropbox, Self::WebDav];

    pub fn label(self) -> &'static str {
        match self {
            Self::GoogleDrive => "Google Drive",
            Self::OneDrive => "OneDrive",
            Self::Dropbox => "Dropbox",
            Self::WebDav => "Nextcloud / WebDAV",
        }
    }

    /// Account name in the OS keychain.
    pub fn keychain_key(self) -> &'static str {
        match self {
            Self::GoogleDrive => "google-drive",
            Self::OneDrive => "onedrive",
            Self::Dropbox => "dropbox",
            Self::WebDav => "webdav",
        }
    }
}

/// A connected account. Its secret (OAuth refresh token or WebDAV password) lives in the OS keychain, never here.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CloudAccount {
    pub provider: CloudProvider,
    /// Shown in Settings: "ana@gmail.com", "ana @ cloud.example.com".
    pub account_label: String,
    /// Off keeps the account connected but uploads nothing.
    pub enabled: bool,
    #[serde(default)]
    pub webdav: Option<WebDavServer>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WebDavServer {
    /// The user's files root, e.g. `https://cloud.example.com/remote.php/dav/files/ana/`.
    pub url: String,
    pub username: String,
}
