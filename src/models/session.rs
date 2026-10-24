use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::PathBuf;
use uuid::Uuid;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct SessionId(pub Uuid);

impl SessionId {
    pub fn new_random() -> Self {
        Self(Uuid::new_v4())
    }
}

/// One favourites folder, typically one photographed event.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Session {
    pub id: SessionId,
    pub name: String,
    pub dir: PathBuf,
    /// Key = absolute source path; value = what we copied into `dir` for it.
    #[serde(default)]
    pub favourites: BTreeMap<PathBuf, Favourite>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Favourite {
    /// Name inside the session dir; differs from the source name after a collision (`IMG_0001 (2).JPG`).
    pub file_name: String,
}
