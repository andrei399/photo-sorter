//! Self-update from GitHub Releases via cargo-packager-updater. Checks at most once a day in the background and
//! downloads silently; installing waits for the user to click Restart, so a cull is never interrupted.
//! Only builds made by the release workflow embed the public key, so dev builds never replace themselves.

use crate::error::{AppError, AppResult};
use cargo_packager_updater::{Config, Update, UpdaterBuilder, semver::Version};
use std::path::PathBuf;
use std::time::{Duration, SystemTime};

/// `releases/latest` skips drafts: updaters only see a release once a human publishes it after the smoke test.
const LATEST_RELEASE_MANIFEST: &str = "https://github.com/andrei399/photo-sorter/releases/latest/download/latest.json";
pub const RELEASES_PAGE: &str = "https://github.com/andrei399/photo-sorter/releases/latest";
const UPDATER_PUBLIC_KEY: Option<&str> = option_env!("PHOTO_SORTER_UPDATER_PUBKEY");
const CHECK_INTERVAL: Duration = Duration::from_secs(24 * 60 * 60);
const REQUEST_TIMEOUT: Duration = Duration::from_secs(60);

pub struct AvailableUpdate {
    pub version: String,
    pub install: UpdateInstall,
}

pub enum UpdateInstall {
    /// Downloaded and signature-verified, ready to replace the installed app.
    Ready(Box<(Update, Vec<u8>)>),
    /// Packages the updater can't replace (Linux .deb): the user downloads the new one themselves.
    FromReleasePage,
}

/// `check_marker`'s modification time records the last successful check.
pub fn spawn_background_update(check_marker: PathBuf, on_available: impl FnOnce(AvailableUpdate) + Send + 'static) {
    let Some(public_key) = UPDATER_PUBLIC_KEY else { return };
    let checked_recently = std::fs::metadata(&check_marker)
        .and_then(|metadata| metadata.modified())
        .is_ok_and(|checked_at| SystemTime::now().duration_since(checked_at).unwrap_or_default() < CHECK_INTERVAL);
    if checked_recently {
        return;
    }
    let spawned = std::thread::Builder::new().name("updater".into()).spawn(move || {
        match find_update(public_key) {
            Ok(update) => {
                let _ = std::fs::write(&check_marker, b"");
                update.map(on_available);
            }
            // Offline at a venue is normal; the next launch tries again.
            Err(error) => log::info!("update check failed: {error}"),
        }
    });
    if let Err(error) = spawned {
        log::warn!("couldn't start the update check: {error}");
    }
}

fn find_update(public_key: &str) -> AppResult<Option<AvailableUpdate>> {
    let current_version: Version = env!("CARGO_PKG_VERSION").parse().map_err(updater_error)?;
    let config = Config {
        endpoints: vec![LATEST_RELEASE_MANIFEST.parse().map_err(updater_error)?],
        pubkey: public_key.to_owned(),
        ..Default::default()
    };
    let updater = UpdaterBuilder::new(current_version, config)
        .timeout(REQUEST_TIMEOUT)
        .build()
        .map_err(updater_error)?;
    let Some(update) = updater.check().map_err(updater_error)? else {
        return Ok(None);
    };
    let version = update.version.clone();
    let install = if self_updating_package() {
        // Verifies the signature; a tampered download never gets this far.
        let bytes = update.download().map_err(updater_error)?;
        UpdateInstall::Ready(Box::new((update, bytes)))
    } else {
        UpdateInstall::FromReleasePage
    };
    Ok(Some(AvailableUpdate { version, install }))
}

/// On Linux only the AppImage can replace itself; the AppImage runtime sets `APPIMAGE`.
fn self_updating_package() -> bool {
    !cfg!(target_os = "linux") || std::env::var_os("APPIMAGE").is_some()
}

/// Replaces the installed app and starts the new version; the caller then closes this one.
/// On Windows the installer takes over: it exits this process and relaunches the app itself.
pub fn install_and_relaunch(update: &Update, bytes: Vec<u8>) -> AppResult<()> {
    update.install(bytes).map_err(updater_error)?;
    let new_executable = std::env::var_os("APPIMAGE")
        .map(PathBuf::from)
        .map_or_else(std::env::current_exe, Ok)?;
    std::process::Command::new(new_executable).spawn()?;
    Ok(())
}

fn updater_error(error: impl std::fmt::Display) -> AppError {
    AppError::Io(format!("Update failed: {error}"))
}
