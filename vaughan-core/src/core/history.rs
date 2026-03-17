//! Transaction history retrieval and caching (Task 7.11).

use std::collections::HashMap;
use std::time::{Duration, Instant};

use tokio::sync::RwLock;

use crate::chains::{ChainAdapter, TxRecord};
use crate::error::WalletError;

#[derive(Clone)]
struct CacheEntry {
    inserted_at: Instant,
    value: Vec<TxRecord>,
}

/// Simple in-memory cache for transaction history.
pub struct HistoryService {
    ttl: Duration,
    cache: RwLock<HashMap<String, CacheEntry>>,
}

impl HistoryService {
    pub fn new(ttl: Duration) -> Self {
        Self {
            ttl,
            cache: RwLock::new(HashMap::new()),
        }
    }

    fn cache_key(&self, kind: &str, chain: &str, address: &str, limit: u32) -> String {
        format!("{kind}:{chain}:{address}:{limit}")
    }

    pub async fn get_transactions(
        &self,
        adapter: &dyn ChainAdapter,
        address: &str,
        limit: u32,
    ) -> Result<Vec<TxRecord>, WalletError> {
        let chain = adapter.chain_info().chain_id.to_string();
        let key = self.cache_key("txlist", &chain, address, limit);

        if let Some(entry) = self.cache.read().await.get(&key).cloned() {
            if entry.inserted_at.elapsed() < self.ttl {
                return Ok(entry.value);
            }
        }

        let fresh = adapter.get_transaction_history(address, limit).await?;
        self.cache.write().await.insert(
            key,
            CacheEntry {
                inserted_at: Instant::now(),
                value: fresh.clone(),
            },
        );
        Ok(fresh)
    }

    pub async fn get_token_transfers(
        &self,
        adapter: &dyn ChainAdapter,
        address: &str,
        limit: u32,
    ) -> Result<Vec<TxRecord>, WalletError> {
        let chain = adapter.chain_info().chain_id.to_string();
        let key = self.cache_key("tokentx", &chain, address, limit);

        if let Some(entry) = self.cache.read().await.get(&key).cloned() {
            if entry.inserted_at.elapsed() < self.ttl {
                return Ok(entry.value);
            }
        }

        let fresh = adapter.get_token_transfer_history(address, limit).await?;
        self.cache.write().await.insert(
            key,
            CacheEntry {
                inserted_at: Instant::now(),
                value: fresh.clone(),
            },
        );
        Ok(fresh)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::chains::{Balance, ChainInfo, ChainType, Fee, TxHash, TxStatus};
    use async_trait::async_trait;

    struct FakeAdapter {
        chain_id: u64,
        tx_calls: RwLock<u32>,
        token_calls: RwLock<u32>,
        fail: bool,
    }

    #[async_trait]
    impl ChainAdapter for FakeAdapter {
        fn chain_info(&self) -> ChainInfo {
            ChainInfo {
                chain_type: ChainType::Evm,
                chain_id: self.chain_id,
                name: "Fake".into(),
                rpc_url: "http://localhost".into(),
            }
        }

        async fn get_balance(&self, _address: &str) -> Result<Balance, WalletError> {
            Err(WalletError::Other("not used".into()))
        }

        async fn get_token_balance(
            &self,
            _address: &str,
            _token_address: &str,
        ) -> Result<Balance, WalletError> {
            Err(WalletError::Other("not used".into()))
        }

        async fn estimate_fee(&self, _tx: &crate::chains::ChainTransaction) -> Result<Fee, WalletError> {
            Err(WalletError::Other("not used".into()))
        }

        async fn send_transaction(
            &self,
            _tx: crate::chains::ChainTransaction,
        ) -> Result<TxHash, WalletError> {
            Err(WalletError::Other("not used".into()))
        }

        async fn get_transaction_history(
            &self,
            _address: &str,
            _limit: u32,
        ) -> Result<Vec<TxRecord>, WalletError> {
            if self.fail {
                return Err(WalletError::NetworkError("boom".into()));
            }
            let mut calls = self.tx_calls.write().await;
            *calls += 1;
                Ok(vec![TxRecord {
                    hash: "0x1".into(),
                    from: "0xfrom".into(),
                    to: "0xto".into(),
                    value: "1".into(),
                    status: TxStatus::Confirmed,
                    block_number: None,
                    timestamp: Some(0),
                    gas_used: None,
                    token_symbol: None,
                    token_address: None,
                    is_token_transfer: false,
                }])
        }

        async fn get_token_transfer_history(
            &self,
            _address: &str,
            _limit: u32,
        ) -> Result<Vec<TxRecord>, WalletError> {
            let mut calls = self.token_calls.write().await;
            *calls += 1;
            Ok(vec![TxRecord {
                hash: "0x2".into(),
                from: "0xfrom".into(),
                to: "0xto".into(),
                value: "2".into(),
                status: TxStatus::Confirmed,
                block_number: None,
                timestamp: Some(0),
                gas_used: None,
                token_symbol: Some("FAKE".into()),
                token_address: Some("0xtoken".into()),
                is_token_transfer: true,
            }])
        }

        async fn get_nonce(&self, _address: &str) -> Result<u64, WalletError> {
            Err(WalletError::Other("not used".into()))
        }

        async fn get_tx_status(&self, _tx_hash: &str) -> Result<TxStatus, WalletError> {
            Err(WalletError::Other("not used".into()))
        }

        fn validate_address(&self, _address: &str) -> Result<(), WalletError> {
            Ok(())
        }

        fn chain_type(&self) -> ChainType {
            ChainType::Evm
        }
    }

    #[tokio::test]
    async fn history_service_caches_transactions() {
        let adapter = FakeAdapter {
            chain_id: 1,
            tx_calls: RwLock::new(0),
            token_calls: RwLock::new(0),
            fail: false,
        };
        let svc = HistoryService::new(Duration::from_secs(60));

        let first = svc.get_transactions(&adapter, "0xaddr", 10).await.unwrap();
        let second = svc.get_transactions(&adapter, "0xaddr", 10).await.unwrap();
        assert_eq!(first.len(), 1);
        assert_eq!(second.len(), 1);

        let calls = adapter.tx_calls.read().await;
        assert_eq!(*calls, 1, "under TTL, should hit adapter only once");
    }

    #[tokio::test]
    async fn history_service_caches_token_transfers() {
        let adapter = FakeAdapter {
            chain_id: 1,
            tx_calls: RwLock::new(0),
            token_calls: RwLock::new(0),
            fail: false,
        };
        let svc = HistoryService::new(Duration::from_secs(60));

        let first = svc.get_token_transfers(&adapter, "0xaddr", 10).await.unwrap();
        let second = svc.get_token_transfers(&adapter, "0xaddr", 10).await.unwrap();
        assert_eq!(first.len(), 1);
        assert_eq!(second.len(), 1);

        let calls = adapter.token_calls.read().await;
        assert_eq!(*calls, 1, "under TTL, should hit adapter only once");
    }

    #[tokio::test]
    async fn history_service_propagates_errors() {
        let adapter = FakeAdapter {
            chain_id: 1,
            tx_calls: RwLock::new(0),
            token_calls: RwLock::new(0),
            fail: true,
        };
        let svc = HistoryService::new(Duration::from_secs(60));
        let err = svc
            .get_transactions(&adapter, "0xaddr", 10)
            .await
            .expect_err("network error should propagate");
        matches!(err, WalletError::NetworkError(_));
    }
}

