//! Token management (Task 19.1).
//!
//! Tracks user-added tokens per chain (starting with ERC-20 on EVM).

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use tokio::sync::RwLock;

use crate::chains::TokenInfo;
use crate::error::WalletError;

/// A token entry tracked by the user for a specific chain.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TrackedToken {
    pub chain_id: u64,
    pub token: TokenInfo,
}

/// Manages user-tracked tokens (in-memory for now; persistence wired later).
pub struct TokenManager {
    // chain_id -> contract_address(lowercase) -> token
    tokens: RwLock<HashMap<u64, HashMap<String, TokenInfo>>>,
}

impl TokenManager {
    pub fn new() -> Self {
        Self {
            tokens: RwLock::new(HashMap::new()),
        }
    }

    pub async fn list(&self, chain_id: u64) -> Vec<TokenInfo> {
        self.tokens
            .read()
            .await
            .get(&chain_id)
            .map(|m| m.values().cloned().collect())
            .unwrap_or_default()
    }

    pub async fn add_erc20(
        &self,
        chain_id: u64,
        contract_address: &str,
        symbol: &str,
        name: &str,
        decimals: u8,
    ) -> Result<(), WalletError> {
        // Validate EVM address format.
        let _ = crate::chains::evm::utils::parse_address(contract_address)?;

        if symbol.trim().is_empty() || name.trim().is_empty() {
            return Err(WalletError::Other(
                "Token symbol/name cannot be empty".into(),
            ));
        }

        let addr_key = contract_address.trim().to_ascii_lowercase();
        let mut guard = self.tokens.write().await;
        let chain_map = guard.entry(chain_id).or_default();
        chain_map.insert(
            addr_key,
            TokenInfo {
                symbol: symbol.trim().to_string(),
                name: name.trim().to_string(),
                decimals,
                contract_address: Some(contract_address.trim().to_string()),
            },
        );
        Ok(())
    }

    pub async fn remove(&self, chain_id: u64, contract_address: &str) -> bool {
        let key = contract_address.trim().to_ascii_lowercase();
        let mut guard = self.tokens.write().await;
        guard
            .get_mut(&chain_id)
            .and_then(|m| m.remove(&key))
            .is_some()
    }

    pub async fn is_tracked(&self, chain_id: u64, contract_address: &str) -> bool {
        let key = contract_address.trim().to_ascii_lowercase();
        self.tokens
            .read()
            .await
            .get(&chain_id)
            .map(|m| m.contains_key(&key))
            .unwrap_or(false)
    }
}

impl Default for TokenManager {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn add_list_and_remove_token() {
        let mgr = TokenManager::new();
        let chain_id = 1;
        let addr = "0x0000000000000000000000000000000000000001";

        mgr.add_erc20(chain_id, addr, "TST", "Test Token", 18)
            .await
            .unwrap();

        assert!(mgr.is_tracked(chain_id, addr).await);
        let list = mgr.list(chain_id).await;
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].symbol, "TST");

        let removed = mgr.remove(chain_id, addr).await;
        assert!(removed);
        assert!(!mgr.is_tracked(chain_id, addr).await);
    }

    #[tokio::test]
    async fn rejects_empty_symbol_or_name() {
        let mgr = TokenManager::new();
        let chain_id = 1;
        let addr = "0x0000000000000000000000000000000000000001";

        let err = mgr
            .add_erc20(chain_id, addr, "", "Name", 18)
            .await
            .expect_err("empty symbol must error");
        matches!(err, WalletError::Other(_));

        let err = mgr
            .add_erc20(chain_id, addr, "SYM", "", 18)
            .await
            .expect_err("empty name must error");
        matches!(err, WalletError::Other(_));
    }
}
