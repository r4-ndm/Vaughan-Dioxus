//! WalletState: chain-agnostic core state (Task 6.3).

use std::collections::HashMap;
use std::sync::Arc;

use tokio::sync::RwLock;

use crate::chains::{ChainAdapter, ChainTransaction, ChainType, Fee, TxHash};
use crate::core::account::Account;
use crate::error::WalletError;

/// Chain-agnostic wallet state coordinating chain adapters.
pub struct WalletState {
    adapters: RwLock<HashMap<ChainType, Arc<dyn ChainAdapter>>>,
    active_chain: RwLock<ChainType>,
    active_account: RwLock<Option<Account>>,
    accounts: RwLock<Vec<Account>>,
    locked: RwLock<bool>,
}

impl WalletState {
    pub fn new() -> Self {
        Self {
            adapters: RwLock::new(HashMap::new()),
            active_chain: RwLock::new(ChainType::Evm),
            active_account: RwLock::new(None),
            accounts: RwLock::new(Vec::new()),
            locked: RwLock::new(true),
        }
    }

    pub async fn set_locked(&self, locked: bool) {
        *self.locked.write().await = locked;
    }

    pub async fn is_locked(&self) -> bool {
        *self.locked.read().await
    }

    pub async fn register_adapter(&self, chain: ChainType, adapter: Arc<dyn ChainAdapter>) {
        self.adapters.write().await.insert(chain, adapter);
    }

    pub async fn set_active_chain(&self, chain: ChainType) -> Result<(), WalletError> {
        let adapters = self.adapters.read().await;
        if !adapters.contains_key(&chain) {
            return Err(WalletError::UnsupportedChain(chain.to_string()));
        }
        drop(adapters);
        *self.active_chain.write().await = chain;
        Ok(())
    }

    pub async fn active_chain(&self) -> ChainType {
        *self.active_chain.read().await
    }

    pub async fn add_account(&self, account: Account) {
        self.accounts.write().await.push(account.clone());
        // If no active account is set, set it.
        if self.active_account.read().await.is_none() {
            *self.active_account.write().await = Some(account);
        }
    }

    pub async fn accounts(&self) -> Vec<Account> {
        self.accounts.read().await.clone()
    }

    pub async fn set_active_account_by_id(
        &self,
        id: crate::core::AccountId,
    ) -> Result<(), WalletError> {
        let accounts = self.accounts.read().await;
        let found = accounts.iter().find(|a| a.id == id).cloned();
        drop(accounts);
        match found {
            Some(a) => {
                *self.active_account.write().await = Some(a);
                Ok(())
            }
            None => Err(WalletError::AccountNotFound(id.0.to_string())),
        }
    }

    pub async fn active_account(&self) -> Option<Account> {
        self.active_account.read().await.clone()
    }

    /// Clear in-memory accounts and lock (after keychain wipe or full reset).
    pub async fn clear_ephemeral_state(&self) {
        self.accounts.write().await.clear();
        *self.active_account.write().await = None;
        *self.locked.write().await = true;
    }

    fn ensure_unlocked(&self, locked: bool) -> Result<(), WalletError> {
        if locked {
            Err(WalletError::WalletLocked)
        } else {
            Ok(())
        }
    }

    pub async fn get_active_balance(&self) -> Result<crate::chains::Balance, WalletError> {
        self.ensure_unlocked(*self.locked.read().await)?;
        let account = self
            .active_account()
            .await
            .ok_or_else(|| WalletError::AccountNotFound("No active account".into()))?;

        let chain = self.active_chain().await;
        let adapters = self.adapters.read().await;
        let adapter = adapters
            .get(&chain)
            .cloned()
            .ok_or_else(|| WalletError::UnsupportedChain(chain.to_string()))?;
        drop(adapters);

        adapter.get_balance(&format!("{:?}", account.address)).await
    }

    pub async fn estimate_fee(&self, tx: &ChainTransaction) -> Result<Fee, WalletError> {
        self.ensure_unlocked(*self.locked.read().await)?;
        let chain = self.active_chain().await;
        let adapters = self.adapters.read().await;
        let adapter = adapters
            .get(&chain)
            .cloned()
            .ok_or_else(|| WalletError::UnsupportedChain(chain.to_string()))?;
        drop(adapters);

        adapter.estimate_fee(tx).await
    }

    pub async fn send_transaction(&self, tx: ChainTransaction) -> Result<TxHash, WalletError> {
        self.ensure_unlocked(*self.locked.read().await)?;
        let chain = self.active_chain().await;
        let adapters = self.adapters.read().await;
        let adapter = adapters
            .get(&chain)
            .cloned()
            .ok_or_else(|| WalletError::UnsupportedChain(chain.to_string()))?;
        drop(adapters);

        adapter.send_transaction(tx).await
    }
}

impl Default for WalletState {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::chains::{
        Balance, ChainInfo, ChainTransaction, ChainType, Fee, TokenInfo, TxHash, TxRecord, TxStatus,
    };
    use async_trait::async_trait;

    struct DummyAdapter;

    #[async_trait]
    impl ChainAdapter for DummyAdapter {
        async fn get_balance(&self, _address: &str) -> Result<Balance, WalletError> {
            Ok(Balance {
                token: TokenInfo {
                    symbol: "ETH".into(),
                    name: "Ether".into(),
                    decimals: 18,
                    contract_address: None,
                },
                raw: "1".into(),
                formatted: "1".into(),
                usd_value: None,
            })
        }

        async fn get_token_balance(
            &self,
            _token_address: &str,
            _wallet_address: &str,
        ) -> Result<Balance, WalletError> {
            self.get_balance(_wallet_address).await
        }

        async fn estimate_fee(&self, _tx: &ChainTransaction) -> Result<Fee, WalletError> {
            Ok(Fee {
                gas_limit: 21_000,
                max_fee_per_gas: Some("1".into()),
                max_priority_fee_per_gas: Some("1".into()),
            })
        }

        async fn get_nonce(&self, _address: &str) -> Result<u64, WalletError> {
            Ok(0)
        }

        async fn send_transaction(&self, _tx: ChainTransaction) -> Result<TxHash, WalletError> {
            Ok(TxHash("0xdead".into()))
        }

        async fn get_tx_status(&self, _tx_hash: &str) -> Result<TxStatus, WalletError> {
            Ok(TxStatus::Confirmed)
        }

        async fn get_transaction_history(
            &self,
            _address: &str,
            _limit: u32,
        ) -> Result<Vec<TxRecord>, WalletError> {
            Ok(vec![])
        }

        async fn get_token_transfer_history(
            &self,
            _address: &str,
            _limit: u32,
        ) -> Result<Vec<TxRecord>, WalletError> {
            Ok(vec![])
        }

        fn validate_address(&self, _address: &str) -> Result<(), WalletError> {
            Ok(())
        }

        fn chain_info(&self) -> ChainInfo {
            ChainInfo {
                chain_type: ChainType::Evm,
                chain_id: 1,
                name: "Ethereum".into(),
                rpc_url: "http://localhost".into(),
            }
        }

        fn chain_type(&self) -> ChainType {
            ChainType::Evm
        }
    }

    fn dummy_account() -> Account {
        use crate::core::{AccountId, AccountType};
        use alloy::primitives::Address;

        Account {
            id: AccountId::new(),
            name: "Test".into(),
            address: Address::ZERO,
            account_type: AccountType::Hd,
            index: Some(0),
        }
    }

    #[tokio::test]
    async fn wallet_lock_unlock_and_active_account() {
        let state = WalletState::new();
        assert!(state.is_locked().await);

        state.add_account(dummy_account()).await;
        state.set_locked(false).await;
        assert!(!state.is_locked().await);
        assert!(state.active_account().await.is_some());
    }

    #[tokio::test]
    async fn wallet_registers_adapter_and_sets_active_chain() {
        let state = WalletState::new();
        let adapter: Arc<dyn ChainAdapter> = Arc::new(DummyAdapter);
        state.register_adapter(ChainType::Evm, adapter).await;
        state.set_active_chain(ChainType::Evm).await.unwrap();
        assert_eq!(state.active_chain().await, ChainType::Evm);
    }

    #[tokio::test]
    async fn wallet_enforces_lock_for_balance() {
        let state = WalletState::new();
        let adapter: Arc<dyn ChainAdapter> = Arc::new(DummyAdapter);
        state.register_adapter(ChainType::Evm, adapter).await;
        state.add_account(dummy_account()).await;

        // Locked: should error
        let err = state
            .get_active_balance()
            .await
            .expect_err("locked wallet must error");
        matches!(err, WalletError::WalletLocked);

        // Unlocked: should succeed
        state.set_locked(false).await;
        let bal = state.get_active_balance().await.unwrap();
        assert_eq!(bal.raw, "1");
    }
}
