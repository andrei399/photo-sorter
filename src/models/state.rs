use crate::models::session::{Session, SessionId};
use crate::models::settings::Settings;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// Root persisted document. Every field must be `#[serde(default)]` so older files keep loading.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct PersistedState {
    #[serde(default)]
    pub schema_version: u32,
    #[serde(default)]
    pub last_opened_folder: Option<PathBuf>,
    #[serde(default)]
    pub sessions: Vec<Session>,
    #[serde(default)]
    pub active_session_id: Option<SessionId>,
    #[serde(default)]
    pub settings: Settings,
}

impl PersistedState {
    pub fn active_session(&self) -> Option<&Session> {
        self.sessions
            .iter()
            .find(|session| Some(session.id) == self.active_session_id)
    }

    pub fn active_session_mut(&mut self) -> Option<&mut Session> {
        let active = self.active_session_id?;
        self.sessions.iter_mut().find(|session| session.id == active)
    }
}
