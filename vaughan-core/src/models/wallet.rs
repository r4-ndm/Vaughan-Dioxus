//! Wallet and account models.

use alloy::primitives::Address;
use serde::{Deserialize, Serialize};

/// Account type
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum AccountType {
    /// HD account (derived from seed)
    Hd,
    /// Imported account (from private key)
    Imported,
}

/// Account information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Account {
    /// Account address
    pub address: Address,
    /// User-defined name
    pub name: String,
    /// Account type
    pub account_type: AccountType,
    /// Derivation index (for HD accounts)
    pub index: Option<u32>,
}
