use crate::{SwarmError, sessions::SessionStore};

pub struct PruneStore {
    sessions: SessionStore,
}

impl PruneStore {
    pub async fn open() -> Result<Self, SwarmError> {
        Ok(Self {
            sessions: SessionStore::open().await?,
        })
    }

    pub async fn sessions(&self) -> Result<usize, SwarmError> {
        self.sessions.prune_terminal_sessions().await
    }
}
