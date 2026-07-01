//! Shared signer loading and transaction field parsing.

use std::str::FromStr;

use alloy::primitives::Address;
use alloy::signers::local::PrivateKeySigner;

use crate::core::account::{Account, AccountManager, AccountType};
use crate::error::WalletError;
use crate::security::{derive_account, mnemonic_to_seed};

/// Parse an optional decimal `u64` (e.g. nonce, gas limit). Empty/whitespace → `None`.
pub fn parse_optional_u64_decimal(value: Option<&str>) -> Result<Option<u64>, WalletError> {
    match value {
        None => Ok(None),
        Some(v) => {
            let t = v.trim();
            if t.is_empty() {
                return Ok(None);
            }
            t.parse::<u64>()
                .map(Some)
                .map_err(|_| WalletError::InvalidTransaction(format!("Invalid u64 value: {t}")))
        }
    }
}

/// Load a [`PrivateKeySigner`] for the given persisted account (HD or imported).
pub fn load_signer_for_account(
    mgr: &AccountManager,
    password: &str,
    account: &Account,
) -> Result<PrivateKeySigner, WalletError> {
    match account.account_type {
        AccountType::Imported => {
            let pk = mgr.export_private_key(password, account.address)?;
            PrivateKeySigner::from_str(pk.trim_start_matches("0x"))
                .map_err(|e| WalletError::InvalidPrivateKey(e.to_string()))
        }
        AccountType::Hd => {
            let idx = account.index.unwrap_or(0);
            let mnemonic = mgr.export_wallet_mnemonic(password)?;
            let seed = mnemonic_to_seed(&mnemonic, None)?;
            derive_account(&seed, idx)
        }
        AccountType::SmartAccount => {
            let info = account.smart_account.as_ref().ok_or_else(|| {
                WalletError::Other("SmartAccount missing SmartAccountInfo".into())
            })?;
            let accounts = mgr.list_accounts_sync();
            let owner_account = accounts
                .iter()
                .find(|a| {
                    a.address == info.owner_address
                        && matches!(a.account_type, AccountType::Hd | AccountType::Imported)
                })
                .ok_or_else(|| {
                    WalletError::AccountNotFound(format!(
                        "Owner EOA {} not found for smart account",
                        info.owner_address
                    ))
                })?;
            load_signer_for_account(mgr, password, owner_account)
        }
    }
}

/// Load a signer for the active account in [`AccountManager`].
pub async fn load_active_signer(
    mgr: &AccountManager,
    password: &str,
) -> Result<PrivateKeySigner, WalletError> {
    let account = mgr
        .active_account()
        .await
        .ok_or_else(|| WalletError::AccountNotFound("No active account".into()))?;
    load_signer_for_account(mgr, password, &account)
}

/// Load a signer for an account matching `address` (case-insensitive `0x` hex).
pub async fn load_signer_for_address(
    mgr: &AccountManager,
    password: &str,
    address: &str,
) -> Result<PrivateKeySigner, WalletError> {
    let needle = address.trim();
    let account = mgr
        .list_accounts()
        .await
        .into_iter()
        .find(|a| address_to_hex(a.address).eq_ignore_ascii_case(needle))
        .ok_or_else(|| {
            WalletError::AccountNotFound("Requested address not found in wallet accounts".into())
        })?;
    load_signer_for_account(mgr, password, &account)
}

/// Canonical `0x`-prefixed checksummed hex for UI and IPC.
pub fn address_to_hex(address: Address) -> String {
    format!("{address:?}")
}
