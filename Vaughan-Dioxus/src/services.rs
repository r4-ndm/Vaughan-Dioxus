use std::sync::{Arc, OnceLock};

use tokio::sync::RwLock;
use vaughan_core::core::account::AccountManager;
use vaughan_core::core::persistence::PersistenceHandle;
use vaughan_core::core::{HistoryService, NetworkService, TokenManager, WalletState};
use vaughan_core::error::WalletError;
use vaughan_core::security::AuthRateLimiter;

#[derive(Clone)]
pub struct AppServices {
    pub wallet_state: Arc<WalletState>,
    pub network_service: Arc<NetworkService>,
    pub history_service: Arc<HistoryService>,
    /// Full `state.json` in memory; single source of truth for persisted app state.
    pub persistence: Arc<PersistenceHandle>,
    pub account_manager: Arc<AccountManager>,
    pub token_manager: Arc<TokenManager>,
    /// Limits repeated failed password attempts (import/export unlock and similar).
    pub password_attempt_limiter: Arc<AuthRateLimiter>,
    session_password: Arc<RwLock<Option<String>>>,
}

impl AppServices {
    /// Construct services, returning a typed error so callers can present a friendly message
    /// instead of crashing when (e.g.) the OS keychain is missing or the state file is unreadable.
    pub fn try_new() -> Result<Self, WalletError> {
        let persistence = PersistenceHandle::open().map_err(|e| {
            WalletError::StorageError(format!("Failed to open wallet state: {e}"))
        })?;
        let account_manager = AccountManager::new("vaughan-wallet", persistence.clone())
            .map_err(|e| {
                WalletError::Other(format!(
                    "Failed to initialise OS keychain (gnome-keyring / KWallet / Keychain): {e}"
                ))
            })?;
        Ok(Self {
            wallet_state: Arc::new(WalletState::new()),
            network_service: Arc::new(NetworkService::new()),
            history_service: Arc::new(HistoryService::new(std::time::Duration::from_secs(10))),
            persistence,
            account_manager: Arc::new(account_manager),
            token_manager: Arc::new(TokenManager::new()),
            password_attempt_limiter: Arc::new(AuthRateLimiter::new()),
            session_password: Arc::new(RwLock::new(None)),
        })
    }

    pub async fn set_session_password(&self, password: String) {
        *self.session_password.write().await = Some(password);
    }

    pub async fn session_password(&self) -> Option<String> {
        self.session_password.read().await.clone()
    }

    pub async fn clear_session_password(&self) {
        *self.session_password.write().await = None;
    }

    /// Delete all keychain secrets and in-memory wallet state. User must complete onboarding again.
    pub async fn wipe_wallet(&self) -> Result<(), vaughan_core::error::WalletError> {
        self.account_manager.wipe_all_wallet_data().await?;
        self.wallet_state.clear_ephemeral_state().await;
        self.clear_session_password().await;
        Ok(())
    }
}

static SERVICES: OnceLock<AppServices> = OnceLock::new();

/// Stash the constructed services so [`shared_services`] can hand them out app-wide.
/// Call once during startup, after `AppServices::try_new()` succeeds.
pub fn install_shared_services(services: AppServices) {
    let _ = SERVICES.set(services);
}

/// Get a clone of the app services. **Panics** if [`install_shared_services`] has not run —
/// that is a programming error (the wallet failed to initialise and should have exited).
pub fn shared_services() -> AppServices {
    SERVICES
        .get()
        .cloned()
        .expect("shared_services() called before install_shared_services()")
}
