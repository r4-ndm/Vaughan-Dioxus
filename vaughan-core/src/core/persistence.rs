//! Persistence and state manager (Task 10.1).
//!
//! **Single writer model** (aligned with Vaughan-Tauri): one [`PersistedState`] document in
//! `state.json`, loaded at startup and saved as a whole after changes. Secrets stay in the OS keychain.

use crate::core::account::{Account, AccountId, AccountType};
use crate::core::NetworkConfig;
use crate::error::WalletError;
use alloy::primitives::Address;
use serde::{Deserialize, Serialize};
use std::ops::DerefMut;
use std::path::{Path, PathBuf};
use std::str::FromStr;
use std::sync::{Arc, RwLock};
use tokio::task;
use uuid::Uuid;

/// Same file path used by [`StateManager::new`].
pub fn vaughan_state_json_path() -> PathBuf {
    let base = dirs::data_dir()
        .unwrap_or_else(|| std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")));
    base.join("vaughan").join("state.json")
}

/// Owns the in-memory [`PersistedState`] and performs full-file atomic saves.
///
/// All subsystems (accounts, networks, tokens, …) should mutate the same struct and call
/// [`PersistenceHandle::save_disk`]; do not merge partial slices into JSON from scattered call sites.
#[derive(Debug)]
pub struct PersistenceHandle {
    path: PathBuf,
    /// In-process source of truth; mutate only with short-held locks, never across `.await`.
    pub(crate) state: RwLock<PersistedState>,
}

impl PersistenceHandle {
    /// Load `state.json` or defaults. Safe to call from sync init (e.g. `OnceLock`).
    pub fn open() -> Result<Arc<Self>, WalletError> {
        let path = vaughan_state_json_path();
        let state = match load_state_file(&path) {
            Ok(s) => s,
            Err(e) => {
                tracing::warn!(
                    target: "vaughan_core",
                    "State load failed (using defaults): {}",
                    e
                );
                PersistedState {
                    version: 1,
                    ..Default::default()
                }
            }
        };
        Ok(Arc::new(Self {
            path,
            state: RwLock::new(state),
        }))
    }

    #[cfg(test)]
    pub fn open_with_path(path: PathBuf, state: PersistedState) -> Arc<Self> {
        Arc::new(Self {
            path,
            state: RwLock::new(state),
        })
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Read-only snapshot (clone). Keep critical sections short; do not hold across `.await`.
    pub fn snapshot(&self) -> PersistedState {
        self.state.read().expect("persist state poisoned").clone()
    }

    /// Write entire `state.json` from current in-memory state (atomic tmp + rename).
    pub async fn save_disk(&self) -> Result<(), WalletError> {
        let path = self.path.clone();
        let snapshot = self.snapshot();
        task::spawn_blocking(move || save_state_file(&path, &snapshot))
            .await
            .map_err(|e| WalletError::Other(format!("Persist join: {}", e)))?
    }

    /// Mutate state under write lock; **do not** `.await` inside `f`. Then persist full file.
    pub async fn update_and_save(
        &self,
        f: impl FnOnce(&mut PersistedState),
    ) -> Result<(), WalletError> {
        {
            let mut g = self.state.write().expect("persist state poisoned");
            f(g.deref_mut());
            if g.version == 0 {
                g.version = 1;
            }
        }
        self.save_disk().await
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct PersistedState {
    pub version: u32,
    #[serde(default)]
    pub accounts: Vec<Account>,
    #[serde(default)]
    pub active_account: Option<AccountId>,
    #[serde(default)]
    pub networks: Vec<NetworkConfig>,
    #[serde(default)]
    pub active_network_id: Option<String>,
    pub preferences: Option<UserPreferences>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UserPreferences {
    pub sound_enabled: bool,
    pub polling_interval_secs: u64,
}

/// State manager: load/save persisted state (async API over the same file as [`PersistenceHandle`]).
pub struct StateManager {
    path: PathBuf,
}

impl StateManager {
    pub fn new() -> Self {
        Self {
            path: vaughan_state_json_path(),
        }
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

impl Default for StateManager {
    fn default() -> Self {
        Self::new()
    }
}

fn load_state_file(path: &Path) -> Result<PersistedState, WalletError> {
    if !path.exists() {
        return Ok(PersistedState {
            version: 1,
            ..Default::default()
        });
    }

    let bytes = std::fs::read(path)
        .map_err(|e| WalletError::Other(format!("Failed to read state: {}", e)))?;

    match serde_json::from_slice::<PersistedState>(&bytes) {
        Ok(mut state) => {
            if state.version == 0 {
                state.version = 1;
            }
            Ok(state)
        }
        Err(e_strict) => {
            let value: serde_json::Value = serde_json::from_slice(&bytes)
                .map_err(|e| WalletError::Other(format!("State file is not valid JSON: {}", e)))?;
            match migrate_persisted_state_from_json(&value) {
                Ok(migrated) => {
                    tracing::info!(
                        target: "vaughan_core",
                        "state.json used Vaughan-Tauri / legacy layout (strict parse failed: {}). Migrated and normalizing.",
                        e_strict
                    );
                    if let Err(e) = save_state_file(path, &migrated) {
                        tracing::error!(
                            target: "vaughan_core",
                            "Failed to write migrated state.json (accounts may re-migrate next launch): {}",
                            e
                        );
                    }
                    Ok(migrated)
                }
                Err(e_mig) => {
                    tracing::warn!(
                        target: "vaughan_core",
                        "Invalid state.json (strict: {}). Migration failed: {} — using empty state.",
                        e_strict,
                        e_mig
                    );
                    Ok(PersistedState {
                        version: 1,
                        ..Default::default()
                    })
                }
            }
        }
    }
}

/// Vaughan-Tauri used `active_account` as a `0x` address string and omitted per-account `id`.
/// Dioxus expects `active_account: Option<AccountId>` and `id` on each [`Account`].
#[derive(Debug, Deserialize)]
struct LooseAccountRow {
    #[serde(default)]
    id: Option<AccountId>,
    #[serde(default = "default_loose_account_name")]
    name: String,
    address: String,
    #[serde(default)]
    account_type: AccountType,
    #[serde(default)]
    index: Option<u32>,
}

fn default_loose_account_name() -> String {
    "Account".into()
}

fn resolve_active_account_field(
    raw: Option<&serde_json::Value>,
    accounts: &[Account],
) -> Option<AccountId> {
    let raw = raw?;
    match raw {
        serde_json::Value::Null => None,
        serde_json::Value::String(s) => {
            let s = s.trim();
            if s.is_empty() {
                return None;
            }
            if let Ok(u) = Uuid::parse_str(s) {
                return Some(AccountId(u));
            }
            if let Ok(addr) = Address::from_str(s) {
                return accounts.iter().find(|a| a.address == addr).map(|a| a.id);
            }
            let lower = s.to_ascii_lowercase();
            accounts
                .iter()
                .find(|a| format!("{:?}", a.address).to_ascii_lowercase() == lower)
                .map(|a| a.id)
        }
        v => serde_json::from_value::<Option<AccountId>>(v.clone())
            .ok()
            .flatten(),
    }
}

fn migrate_persisted_state_from_json(
    value: &serde_json::Value,
) -> Result<PersistedState, WalletError> {
    let version = value.get("version").and_then(|v| v.as_u64()).unwrap_or(1) as u32;

    let items = value
        .get("accounts")
        .and_then(|a| a.as_array())
        .cloned()
        .unwrap_or_default();

    let mut accounts = Vec::new();
    for item in items {
        if let Ok(a) = serde_json::from_value::<Account>(item.clone()) {
            accounts.push(a);
            continue;
        }
        let row: LooseAccountRow = serde_json::from_value(item)
            .map_err(|e| WalletError::Other(format!("migrate accounts: {}", e)))?;
        let address = Address::from_str(row.address.trim())
            .map_err(|e| WalletError::Other(format!("migrate accounts: bad address: {}", e)))?;
        accounts.push(Account {
            id: row.id.unwrap_or_else(AccountId::new),
            name: row.name,
            address,
            account_type: row.account_type,
            index: row.index,
        });
    }

    let mut active_account = resolve_active_account_field(value.get("active_account"), &accounts);

    if active_account.is_none() {
        active_account = accounts.first().map(|a| a.id);
    }

    let networks: Vec<NetworkConfig> = value
        .get("networks")
        .or_else(|| value.get("custom_networks"))
        .and_then(|n| serde_json::from_value(n.clone()).ok())
        .unwrap_or_default();

    let active_network_id = value
        .get("active_network_id")
        .and_then(|v| v.as_str())
        .map(std::string::ToString::to_string);

    let preferences = value
        .get("preferences")
        .and_then(|p| serde_json::from_value::<UserPreferences>(p.clone()).ok());

    Ok(PersistedState {
        version: version.max(1),
        accounts,
        active_account,
        networks,
        active_network_id,
        preferences,
    })
}

pub(crate) fn save_state_file(path: &Path, state: &PersistedState) -> Result<(), WalletError> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|e| WalletError::Other(format!("Failed to create data dir: {}", e)))?;
    }

    let data = serde_json::to_vec_pretty(state)
        .map_err(|e| WalletError::Other(format!("Failed to serialize state: {}", e)))?;

    let tmp = path.with_extension("json.tmp");
    std::fs::write(&tmp, data)
        .map_err(|e| WalletError::Other(format!("Failed to write state: {}", e)))?;
    std::fs::rename(&tmp, path)
        .map_err(|e| WalletError::Other(format!("Failed to persist state: {}", e)))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    /// Prefer workspace `target/` so tests stay reliable when OS `/tmp` is full (CI sandboxes, laptops).
    fn test_temp_dir() -> PathBuf {
        let manifest = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        let dir = manifest
            .parent()
            .map(|p| p.join("target").join("vaughan-core-test-tmp"))
            .unwrap_or_else(|| manifest.join("target").join("vaughan-core-test-tmp"));
        let _ = std::fs::create_dir_all(&dir);
        dir
    }

    #[tokio::test]
    async fn persistence_roundtrip() {
        let dir = tempfile::tempdir_in(test_temp_dir()).expect("test tempdir");
        let path = dir
            .path()
            .join(format!("state_{}.json", uuid::Uuid::new_v4()));
        let mgr = StateManager::new_with_path(path.clone());

        let s = PersistedState {
            version: 1,
            active_network_id: Some("ethereum-mainnet".into()),
            ..Default::default()
        };

        mgr.save(&s).await.unwrap();
        let loaded = mgr.load().await.unwrap();
        assert_eq!(loaded.version, 1);
        assert_eq!(loaded.active_network_id, Some("ethereum-mainnet".into()));
    }

    #[tokio::test]
    async fn persistence_handle_full_save() {
        let dir = tempfile::tempdir_in(test_temp_dir()).expect("test tempdir");
        let path = dir.path().join(format!("ph_{}.json", uuid::Uuid::new_v4()));
        let ph = PersistenceHandle::open_with_path(
            path.clone(),
            PersistedState {
                version: 1,
                active_network_id: Some("ethereum".into()),
                ..Default::default()
            },
        );
        ph.update_and_save(|s| {
            s.accounts.clear();
        })
        .await
        .unwrap();

        let disk = load_state_file(&path).unwrap();
        assert_eq!(disk.version, 1);
        assert_eq!(disk.active_network_id, Some("ethereum".into()));
    }

    #[test]
    fn load_migrates_tauri_style_state_and_rewrites_disk() {
        let dir = tempfile::tempdir_in(test_temp_dir()).expect("test tempdir");
        let path = dir.path().join("tauri_like_state.json");
        let addr = "0xd8da6bf26964af9d7eed9e03e53415d37aa96045";
        let json = format!(
            r#"{{
                "version": 1,
                "accounts": [
                    {{
                        "name": "Main",
                        "address": "{addr}",
                        "account_type": "hd",
                        "index": 0
                    }}
                ],
                "active_account": "{addr}"
            }}"#
        );
        std::fs::write(&path, json.as_bytes()).unwrap();

        let migrated = load_state_file(&path).expect("migrate");
        assert_eq!(migrated.accounts.len(), 1);
        assert_eq!(migrated.accounts[0].name, "Main");
        assert_eq!(
            migrated.active_account,
            Some(migrated.accounts[0].id),
            "active_account 0x string should resolve to the matching account id"
        );

        let strict: PersistedState =
            serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
        assert_eq!(strict.accounts.len(), 1);
        assert_eq!(strict.accounts[0].id, migrated.accounts[0].id);
        assert_eq!(strict.active_account, migrated.active_account);
    }
}
