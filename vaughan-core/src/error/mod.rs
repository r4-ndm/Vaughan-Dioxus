//! Centralized error types and recovery helpers for wallet operations.
//!
//! ## UI vs logs
//! - **User-visible strings:** always use [`WalletError::user_message`] in Dioxus (and similar GUIs).
//!   [`WalletError`]'s `Display` / [`std::string::ToString::to_string`] may leak internal detail.
//! - **Logs / tracing:** prefer the `Display` implementation or structured fields for engineers.
//!
//! ## Recovery
//! Use [`WalletError::is_transient`] with [`retry_async_transient`] for RPC / network flakiness.
//! Do not retry validation, auth, or balance errors — those need user action.

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
    #[error("A master wallet already exists on this device")]
    WalletAlreadyExists,
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
                "Network error. Please check your connection. Waiting a few seconds and trying again often helps."
                    .into()
            }
            Self::InvalidAddress(_) => "That address looks invalid.".into(),
            Self::InvalidAmount(_) => "Please enter a valid amount.".into(),
            Self::InvalidPrivateKey(_) | Self::InvalidMnemonic(_) | Self::InvalidDerivationPath(_) => {
                "The provided secret material is invalid.".into()
            }
            Self::WalletLocked => "Wallet is locked. Unlock it to continue.".into(),
            Self::WalletAlreadyExists => "A master wallet already exists on this device.".into(),
            Self::InvalidPassword => "Incorrect password.".into(),
            Self::InsufficientBalance { .. } => "Insufficient balance for this transaction.".into(),
            Self::TransactionFailed(_) => "Transaction failed.".into(),
            Self::GasEstimationFailed(_) => {
                "Failed to estimate gas. The RPC may be busy — try again shortly.".into()
            }
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

    /// Whether this error may succeed if retried (typical RPC / transport flake).
    pub fn is_transient(&self) -> bool {
        matches!(
            self,
            Self::NetworkError(_)
                | Self::RpcError(_)
                | Self::ConnectionTimeout(_)
                | Self::ChainError(_)
                | Self::GasEstimationFailed(_)
        )
    }
}

/// Retry an async operation with exponential backoff (every failure is retried).
pub async fn retry_async<F, Fut, T>(
    mut op: F,
    attempts: usize,
    base_delay: Duration,
) -> Result<T, WalletError>
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

/// Retry only on [`WalletError::is_transient`]; return immediately on other errors.
pub async fn retry_async_transient<F, Fut, T>(
    mut op: F,
    attempts: usize,
    base_delay: Duration,
) -> Result<T, WalletError>
where
    F: FnMut() -> Fut,
    Fut: std::future::Future<Output = Result<T, WalletError>>,
{
    let attempts = attempts.max(1);
    let mut delay = base_delay;

    for idx in 0..attempts {
        match op().await {
            Ok(v) => return Ok(v),
            Err(e) if !e.is_transient() => return Err(e),
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
    use std::sync::{
        atomic::{AtomicUsize, Ordering},
        Arc,
    };

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
    async fn retry_async_transient_skips_non_transient() {
        let attempts = Arc::new(AtomicUsize::new(0));
        let attempts2 = attempts.clone();
        let err = retry_async_transient::<_, _, ()>(
            move || {
                let attempts2 = attempts2.clone();
                async move {
                    attempts2.fetch_add(1, Ordering::SeqCst);
                    Err(WalletError::InvalidPassword)
                }
            },
            5,
            Duration::from_millis(1),
        )
        .await
        .unwrap_err();
        assert!(matches!(err, WalletError::InvalidPassword));
        assert_eq!(attempts.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn retry_async_transient_retries_network() {
        let attempts = Arc::new(AtomicUsize::new(0));
        let attempts2 = attempts.clone();
        let out = retry_async_transient(
            move || {
                let attempts2 = attempts2.clone();
                async move {
                    let n = attempts2.fetch_add(1, Ordering::SeqCst);
                    if n < 1 {
                        Err(WalletError::RpcError("busy".into()))
                    } else {
                        Ok(7u8)
                    }
                }
            },
            5,
            Duration::from_millis(1),
        )
        .await
        .unwrap();
        assert_eq!(out, 7);
        assert_eq!(attempts.load(Ordering::SeqCst), 2);
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
