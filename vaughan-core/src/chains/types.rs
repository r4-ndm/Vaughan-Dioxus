//! Chain-agnostic types.

use serde::{Deserialize, Serialize};
use std::fmt;

/// Supported blockchain types
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ChainType {
    Evm,
    Stellar,
    Aptos,
    Solana,
    Bitcoin,
}

impl fmt::Display for ChainType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Evm => write!(f, "EVM"),
            Self::Stellar => write!(f, "Stellar"),
            Self::Aptos => write!(f, "Aptos"),
            Self::Solana => write!(f, "Solana"),
            Self::Bitcoin => write!(f, "Bitcoin"),
        }
    }
}

/// Token information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TokenInfo {
    pub symbol: String,
    pub name: String,
    pub decimals: u8,
    pub contract_address: Option<String>,
}

/// Chain-agnostic balance
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Balance {
    pub token: TokenInfo,
    pub raw: String,
    pub formatted: String,
    pub usd_value: Option<f64>,
}

/// Chain info (name, chain id, etc.)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChainInfo {
    pub chain_type: ChainType,
    pub chain_id: u64,
    pub name: String,
    pub rpc_url: String,
}

/// Transaction hash
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TxHash(pub String);

impl fmt::Display for TxHash {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl From<String> for TxHash {
    fn from(value: String) -> Self {
        Self(value)
    }
}

// ----------------------------------------------------------------------------
// Transaction requests (Task 3.6)
// ----------------------------------------------------------------------------

/// Chain-agnostic transaction request.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "chain_type")]
pub enum ChainTransaction {
    Evm(EvmTransaction),
}

/// EVM transaction parameters (values are expected to be decimal strings in wei).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EvmTransaction {
    pub from: String,
    pub to: String,
    pub value: String,
    pub data: Option<String>,
    pub gas_limit: Option<u64>,
    pub gas_price: Option<String>,
    pub max_fee_per_gas: Option<String>,
    pub max_priority_fee_per_gas: Option<String>,
    pub nonce: Option<u64>,
    pub chain_id: u64,
}

/// Fee estimate
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Fee {
    pub gas_limit: u64,
    pub max_fee_per_gas: Option<String>,
    pub max_priority_fee_per_gas: Option<String>,
}

/// Transaction record for history
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TxRecord {
    pub hash: String,
    pub from: String,
    pub to: String,
    pub value: String,
    pub status: TxStatus,
    pub block_number: Option<u64>,
    pub timestamp: Option<u64>,
    pub gas_used: Option<u64>,
    pub token_symbol: Option<String>,
    pub token_address: Option<String>,
    pub is_token_transfer: bool,
    /// ERC-20 decimals from explorer (`tokenDecimal`) when `is_token_transfer` is true.
    #[serde(default)]
    pub token_decimals: Option<u8>,
}

/// Transaction status
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum TxStatus {
    Pending,
    Confirmed,
    Failed,
}

/// Signature (chain-agnostic)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Signature(pub Vec<u8>);
