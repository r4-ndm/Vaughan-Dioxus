//! Centralized error types and recovery helpers for wallet operations.

use std::time::Duration;

use tokio::time::sleep;

/// Central error type for all wallet operations.
#[derive(Debug, thiserror::Error)]
pub enum WalletError {
    // Network
    #[error("Network error: {0}")]
    NetworkError(String),
    #[error("RPC error: {0}")]
    RpcError(String),
    #[error("Connection timeout: {0}")]
    ConnectionTimeout(String),

    // Address
    #[error("Invalid address: {0}")]
    InvalidAddress(String),

    // Transaction
    #[error("Insufficient balance: need {need}, have {have}")]
    InsufficientBalance { need: String, have: String },
    #[error("Transaction failed: {0}")]
    TransactionFailed(String),
    #[error("Invalid transaction: {0}")]
    InvalidTransaction(String),
    #[error("Gas estimation failed: {0}")]
    GasEstimationFailed(String),
    #[error("Invalid amount: {0}")]
    InvalidAmount(String),

    // Account
    #[error("Account not found: {0}")]
    AccountNotFound(String),
    #[error("Invalid private key: {0}")]
    InvalidPrivateKey(String),
    #[error("Invalid mnemonic: {0}")]
    InvalidMnemonic(String),
    #[error("Invalid derivation path: {0}")]
    InvalidDerivationPath(String),

    // Security
    #[error("Unauthorized")]
    Unauthorized,
    #[error("Wallet is locked")]
    WalletLocked,
    #[error("Invalid password")]
    InvalidPassword,
    #[error("Encryption failed: {0}")]
    EncryptionFailed(String),
    #[error("Decryption failed: {0}")]
    DecryptionFailed(String),
    #[error("Signing failed: {0}")]
    SigningFailed(String),
    #[error("Key derivation failed: {0}")]
    KeyDerivationFailed(String),
    #[error("Keyring error: {0}")]
    KeyringError(String),

    // Chain / adapter
    #[error("Unsupported chain: {0}")]
    UnsupportedChain(String),
    #[error("Chain error: {0}")]
    ChainError(String),

    // Persistence
    #[error("Storage error: {0}")]
    StorageError(String),
    #[error("Invalid data: {0}")]
    InvalidData(String),

    // Other
    #[error("{0}")]
    Other(String),
}

impl WalletError {
    /// Short, user-facing message suitable for UI display.
    pub fn user_message(&self) -> String {
        match self {
            Self::NetworkError(_) | Self::RpcError(_) | Self::ConnectionTimeout(_) => {
                "Network error. Please check your connection and try again.".into()
            }
            Self::InvalidAddress(_) => "That address looks invalid.".into(),
            Self::InvalidAmount(_) => "Please enter a valid amount.".into(),
            Self::InvalidPrivateKey(_) | Self::InvalidMnemonic(_) | Self::InvalidDerivationPath(_) => {
                "The provided secret material is invalid.".into()
            }
            Self::WalletLocked => "Wallet is locked. Unlock it to continue.".into(),
            Self::InvalidPassword => "Incorrect password.".into(),
            Self::InsufficientBalance { .. } => "Insufficient balance for this transaction.".into(),
            Self::TransactionFailed(_) => "Transaction failed.".into(),
            Self::GasEstimationFailed(_) => "Failed to estimate gas.".into(),
            Self::EncryptionFailed(_) | Self::DecryptionFailed(_) | Self::SigningFailed(_) | Self::KeyDerivationFailed(_) => {
                "Security operation failed.".into()
            }
            Self::KeyringError(_) | Self::StorageError(_) => "Could not access secure storage.".into(),
            Self::AccountNotFound(_) | Self::UnsupportedChain(_) | Self::ChainError(_) => {
                "Requested wallet data is unavailable.".into()
            }
            Self::InvalidTransaction(_) | Self::InvalidData(_) | Self::Unauthorized => {
                "The requested operation could not be completed.".into()
            }
            Self::Other(msg) => msg.clone(),
        }
    }
}

/// Retry an async operation with exponential backoff.
pub async fn retry_async<F, Fut, T>(mut op: F, attempts: usize, base_delay: Duration) -> Result<T, WalletError>
where
    F: FnMut() -> Fut,
    Fut: std::future::Future<Output = Result<T, WalletError>>,
{
    let attempts = attempts.max(1);
    let mut delay = base_delay;

    for idx in 0..attempts {
        match op().await {
            Ok(v) => return Ok(v),
            Err(e) if idx + 1 == attempts => return Err(e),
            Err(_) => {
                sleep(delay).await;
                delay = delay.saturating_mul(2);
            }
        }
    }

    unreachable!("attempts is clamped to >= 1");
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Arc, atomic::{AtomicUsize, Ordering}};

    #[test]
    fn user_message_is_friendly() {
        assert_eq!(
            WalletError::InvalidAddress("0xdead".into()).user_message(),
            "That address looks invalid."
        );
        assert_eq!(
            WalletError::WalletLocked.user_message(),
            "Wallet is locked. Unlock it to continue."
        );
    }

    #[tokio::test]
    async fn retry_async_eventually_succeeds() {
        let attempts = Arc::new(AtomicUsize::new(0));
        let attempts2 = attempts.clone();
        let out = retry_async(
            move || {
                let attempts2 = attempts2.clone();
                async move {
                    let n = attempts2.fetch_add(1, Ordering::SeqCst);
                    if n < 2 {
                        Err(WalletError::NetworkError("transient".into()))
                    } else {
                        Ok(42)
                    }
                }
            },
            5,
            Duration::from_millis(1),
        )
        .await
        .unwrap();
        assert_eq!(out, 42);
        assert_eq!(attempts.load(Ordering::SeqCst), 3);
    }
}
