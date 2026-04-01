//! Network management (Task 9.1).

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use tokio::sync::RwLock;

use crate::chains::evm::networks::builtin_networks;
use crate::error::WalletError;
use std::time::Instant;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NetworkConfig {
    pub id: String,
    pub name: String,
    pub rpc_url: String,
    pub chain_id: u64,
    pub explorer_url: Option<String>,
    pub explorer_api_url: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NetworkInfo {
    pub chain_id: u64,
    pub name: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NetworkHealth {
    pub ok: bool,
    pub latency_ms: u128,
    pub latest_block: Option<u64>,
    pub error: Option<String>,
}

/// Network service managing known networks and the current selection.
pub struct NetworkService {
    networks: RwLock<HashMap<String, NetworkConfig>>,
    active: RwLock<Option<String>>,
}

impl NetworkService {
    pub fn new() -> Self {
        let mut map = HashMap::new();
        for n in builtin_networks() {
            map.insert(
                n.id.clone(),
                NetworkConfig {
                    id: n.id,
                    name: n.name,
                    rpc_url: n.rpc_url,
                    chain_id: n.chain_id,
                    explorer_url: n.explorer_url,
                    explorer_api_url: n.explorer_api_url,
                },
            );
        }

        Self {
            networks: RwLock::new(map),
            // Default so RPC/UI (balance, history, tokens) agree before user opens Settings.
            active: RwLock::new(Some("ethereum".into())),
        }
    }

    pub async fn list_networks(&self) -> Vec<NetworkConfig> {
        self.networks.read().await.values().cloned().collect()
    }

    pub async fn get_network(&self, id: &str) -> Option<NetworkConfig> {
        self.networks.read().await.get(id).cloned()
    }

    pub async fn add_custom_network(&self, cfg: NetworkConfig) -> Result<(), WalletError> {
        if cfg.id.trim().is_empty() {
            return Err(WalletError::Other("Network id cannot be empty".into()));
        }
        self.networks.write().await.insert(cfg.id.clone(), cfg);
        Ok(())
    }

    pub async fn set_active_network(&self, id: &str) -> Result<(), WalletError> {
        let networks = self.networks.read().await;
        if !networks.contains_key(id) {
            return Err(WalletError::Other(format!("Unknown network id: {}", id)));
        }
        drop(networks);
        *self.active.write().await = Some(id.to_string());
        Ok(())
    }

    pub async fn active_network(&self) -> Option<NetworkConfig> {
        let id = self.active.read().await.clone()?;
        self.networks.read().await.get(&id).cloned()
    }

    pub async fn active_network_info(&self) -> Option<NetworkInfo> {
        let net = self.active_network().await?;
        Some(NetworkInfo {
            chain_id: net.chain_id,
            name: net.name,
        })
    }

    /// Check RPC health by calling `eth_blockNumber`.
    pub async fn check_health(&self, id: &str) -> Result<NetworkHealth, WalletError> {
        let net = self
            .get_network(id)
            .await
            .ok_or_else(|| WalletError::Other(format!("Unknown network id: {}", id)))?;

        let started = Instant::now();
        let client = reqwest::Client::new();
        let payload = serde_json::json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "eth_blockNumber",
            "params": []
        });

        let resp = client.post(&net.rpc_url).json(&payload).send().await;

        let latency_ms = started.elapsed().as_millis();

        match resp {
            Ok(r) => {
                let v: serde_json::Value = r
                    .json()
                    .await
                    .map_err(|e| WalletError::RpcError(e.to_string()))?;
                let block_hex = v.get("result").and_then(|x| x.as_str()).unwrap_or("0x0");
                let latest_block = u64::from_str_radix(block_hex.trim_start_matches("0x"), 16).ok();
                Ok(NetworkHealth {
                    ok: true,
                    latency_ms,
                    latest_block,
                    error: None,
                })
            }
            Err(e) => Ok(NetworkHealth {
                ok: false,
                latency_ms,
                latest_block: None,
                error: Some(e.to_string()),
            }),
        }
    }
}

impl Default for NetworkService {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn can_add_and_select_custom_network() {
        let svc = NetworkService::new();
        let custom = NetworkConfig {
            id: "local-dev".into(),
            name: "Local Dev".into(),
            rpc_url: "http://localhost:8545".into(),
            chain_id: 1337,
            explorer_url: None,
            explorer_api_url: None,
        };
        svc.add_custom_network(custom.clone()).await.unwrap();
        svc.set_active_network(&custom.id).await.unwrap();

        let active = svc.active_network().await.unwrap();
        assert_eq!(active.id, custom.id);
        assert_eq!(active.chain_id, 1337);

        let info = svc.active_network_info().await.unwrap();
        assert_eq!(info.chain_id, 1337);
        assert_eq!(info.name, "Local Dev");
    }

    #[tokio::test]
    async fn rejects_empty_network_id() {
        let svc = NetworkService::new();
        let bad = NetworkConfig {
            id: "".into(),
            name: "Bad".into(),
            rpc_url: "http://localhost:8545".into(),
            chain_id: 1,
            explorer_url: None,
            explorer_api_url: None,
        };
        let err = svc
            .add_custom_network(bad)
            .await
            .expect_err("empty id must error");
        matches!(err, WalletError::Other(_));
    }

    #[tokio::test]
    async fn set_active_unknown_network_errors() {
        let svc = NetworkService::new();
        let err = svc
            .set_active_network("does-not-exist")
            .await
            .expect_err("unknown id must error");
        matches!(err, WalletError::Other(_));
    }
}
