//! Persistence and state manager (Task 10.1).

use crate::error::WalletError;
use crate::core::{Account, AccountId, NetworkConfig};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use tokio::task;

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct PersistedState {
    pub version: u32,
    pub accounts: Vec<Account>,
    pub active_account: Option<AccountId>,
    pub networks: Vec<NetworkConfig>,
    pub active_network_id: Option<String>,
    pub preferences: Option<UserPreferences>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UserPreferences {
    pub sound_enabled: bool,
    pub polling_interval_secs: u64,
}

/// State manager: load/save persisted state
pub struct StateManager {
    path: PathBuf,
}

impl StateManager {
    pub fn new() -> Self {
        let base = dirs::data_dir().unwrap_or_else(|| std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")));
        let dir = base.join("vaughan");
        let path = dir.join("state.json");
        Self { path }
    }

    /// Create a state manager with an explicit file path (useful for tests).
    pub fn new_with_path(path: PathBuf) -> Self {
        Self { path }
    }

    pub async fn load(&self) -> Result<PersistedState, WalletError> {
        let path = self.path.clone();
        task::spawn_blocking(move || load_state_file(&path))
            .await
            .map_err(|e| WalletError::Other(format!("Join error: {}", e)))?
    }

    pub async fn save(&self, _state: &PersistedState) -> Result<(), WalletError> {
        let path = self.path.clone();
        let state = _state.clone();
        task::spawn_blocking(move || save_state_file(&path, &state))
            .await
            .map_err(|e| WalletError::Other(format!("Join error: {}", e)))?
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn persistence_roundtrip() {
        let path = std::env::temp_dir().join(format!("vaughan_state_test_{}.json", uuid::Uuid::new_v4()));
        let mgr = StateManager::new_with_path(path.clone());

        let mut s = PersistedState::default();
        s.version = 1;
        s.active_network_id = Some("ethereum-mainnet".into());

        mgr.save(&s).await.unwrap();
        let loaded = mgr.load().await.unwrap();
        assert_eq!(loaded.version, 1);
        assert_eq!(loaded.active_network_id, Some("ethereum-mainnet".into()));

        let _ = std::fs::remove_file(path);
    }
}

fn load_state_file(path: &Path) -> Result<PersistedState, WalletError> {
    if !path.exists() {
        return Ok(PersistedState {
            version: 1,
            ..Default::default()
        });
    }

    let bytes = std::fs::read(path).map_err(|e| WalletError::Other(format!("Failed to read state: {}", e)))?;
    let mut state: PersistedState =
        serde_json::from_slice(&bytes).map_err(|e| WalletError::Other(format!("Invalid state JSON: {}", e)))?;
    if state.version == 0 {
        state.version = 1;
    }
    Ok(state)
}

fn save_state_file(path: &Path, state: &PersistedState) -> Result<(), WalletError> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|e| WalletError::Other(format!("Failed to create data dir: {}", e)))?;
    }

    let data = serde_json::to_vec_pretty(state)
        .map_err(|e| WalletError::Other(format!("Failed to serialize state: {}", e)))?;

    let tmp = path.with_extension("json.tmp");
    std::fs::write(&tmp, data).map_err(|e| WalletError::Other(format!("Failed to write state: {}", e)))?;
    std::fs::rename(&tmp, path).map_err(|e| WalletError::Other(format!("Failed to persist state: {}", e)))?;
    Ok(())
}
