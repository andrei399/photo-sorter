use crate::error::AppResult;
use crate::models::state::PersistedState;
use std::fs;
use std::io::Write;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

/// Owned exclusively by the core actor thread, so no locking.
pub struct Store {
    path: PathBuf,
    pub state: PersistedState,
}

impl Store {
    /// Missing file → defaults. Corrupt file → quarantined next to the original (never overwritten) and defaults.
    pub fn load_or_default(path: PathBuf) -> AppResult<Self> {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        let state = match fs::read(&path) {
            Ok(bytes) => serde_json::from_slice(&bytes).unwrap_or_else(|error| {
                let quarantine_path = path.with_extension(format!("json.corrupt-{}", unix_seconds_now()));
                log::warn!(
                    "state file is corrupt ({error}); moving it to {}",
                    quarantine_path.display()
                );
                if let Err(rename_error) = fs::rename(&path, &quarantine_path) {
                    log::error!("could not quarantine corrupt state file: {rename_error}");
                }
                PersistedState::default()
            }),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => PersistedState::default(),
            Err(error) => return Err(error.into()),
        };
        Ok(Self { path, state })
    }

    /// tmp → fsync → rename, so a crash leaves either the old or the new file, never half of one.
    pub fn save(&self) -> AppResult<()> {
        let tmp_path = self.path.with_extension("json.tmp");
        let mut tmp_file = fs::File::create(&tmp_path)?;
        tmp_file.write_all(&serde_json::to_vec_pretty(&self.state)?)?;
        tmp_file.sync_all()?;
        fs::rename(&tmp_path, &self.path)?;
        Ok(())
    }
}

fn unix_seconds_now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or_default()
}
