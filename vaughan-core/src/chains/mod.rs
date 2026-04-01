//! Chain adapters: trait and implementations (EVM first).

pub mod evm;
pub mod types;

pub use evm::EvmAdapter;
pub use types::{
    Balance, ChainInfo, ChainTransaction, ChainType, EvmTransaction, Fee, Signature, TokenInfo,
    TxHash, TxRecord, TxStatus,
};

// ChainAdapter trait and EVM adapter will go here; stub for now.
use crate::error::WalletError;
use async_trait::async_trait;

/// Network identifier (e.g. "ethereum-mainnet")
pub type NetworkId = String;

/// Chain adapter trait for blockchain operations.
#[async_trait]
pub trait ChainAdapter: Send + Sync {
    async fn get_balance(&self, address: &str) -> Result<Balance, WalletError>;
    async fn get_token_balance(
        &self,
        token_address: &str,
        wallet_address: &str,
    ) -> Result<Balance, WalletError>;
    async fn estimate_fee(&self, tx: &ChainTransaction) -> Result<Fee, WalletError>;
    async fn get_nonce(&self, address: &str) -> Result<u64, WalletError>;
    async fn send_transaction(&self, tx: ChainTransaction) -> Result<TxHash, WalletError>;
    async fn get_tx_status(&self, tx_hash: &str) -> Result<TxStatus, WalletError>;
    async fn get_transaction_history(
        &self,
        address: &str,
        limit: u32,
    ) -> Result<Vec<TxRecord>, WalletError>;
    async fn get_token_transfer_history(
        &self,
        address: &str,
        limit: u32,
    ) -> Result<Vec<TxRecord>, WalletError>;
    fn validate_address(&self, address: &str) -> Result<(), WalletError>;
    fn chain_info(&self) -> ChainInfo;
    fn chain_type(&self) -> ChainType;
}
