//! Default EVM RPC adapter and wallet address resolution shared by dashboard, history, and similar views.

use std::sync::Arc;

use vaughan_core::chains::evm::networks::get_network_by_id;
use vaughan_core::chains::evm::utils::parse_address;
use vaughan_core::chains::evm::EvmAdapter;
use vaughan_core::chains::{ChainAdapter, ChainType};
use vaughan_core::core::account::AccountManager;
use vaughan_core::core::{
    Account, AccountId, AccountType, NetworkConfig, NetworkService, WalletState,
};
use vaughan_core::error::WalletError;

/// Only used when there is no account in [`AccountManager`] yet (e.g. edge cases before onboarding completes).
const EXPLORER_FALLBACK_ADDRESS: &str =
    concat!("0xd8dA", "6BF26964", "aF9D7eEd", "9e03E534", "15D37aA9", "6045");

/// EVM adapter for the user's selected network in [`NetworkService`], or built-in Ethereum if none set.
pub async fn evm_adapter_for_network_service(
    network_service: &NetworkService,
) -> Result<Arc<dyn ChainAdapter>, WalletError> {
    let cfg = match network_service.active_network().await {
        Some(n) => n,
        None => {
            let net = get_network_by_id("ethereum").ok_or_else(|| {
                WalletError::Other("No active network and built-in \"ethereum\" is missing".into())
            })?;
            NetworkConfig {
                id: net.id,
                name: net.name,
                rpc_url: net.rpc_url,
                chain_id: net.chain_id,
                explorer_url: net.explorer_url,
                explorer_api_url: net.explorer_api_url,
            }
        }
    };
    let evm = EvmAdapter::new(&cfg.rpc_url, cfg.chain_id, cfg.name).await?;
    Ok(Arc::new(evm) as Arc<dyn ChainAdapter>)
}

/// `0x`-prefixed address for RPC calls: persisted active/first account, else a well-known mainnet address.
pub async fn primary_wallet_address_hex(mgr: &AccountManager) -> String {
    if let Some(a) = mgr.active_account().await {
        return format!("{:?}", a.address);
    }
    if let Some(a) = mgr.list_accounts().await.first() {
        return format!("{:?}", a.address);
    }
    EXPLORER_FALLBACK_ADDRESS.to_string()
}

/// Register default EVM adapter and unlock [`WalletState`] (shared dashboard/history setup).
pub async fn register_default_evm_adapter(
    wallet_state: &WalletState,
    adapter: Arc<dyn ChainAdapter>,
) {
    wallet_state.register_adapter(ChainType::Evm, adapter).await;
    wallet_state.set_locked(false).await;
}

/// Copy **all** persisted accounts and the active id from [`AccountManager`] into [`WalletState`].
///
/// Call after unlock and whenever adapters register so balance/signing match `state.json`.
pub async fn sync_wallet_state_with_account_manager(
    wallet_state: &WalletState,
    mgr: &AccountManager,
) {
    let mut accounts = mgr.list_accounts().await;
    let active = mgr.active_account().await;

    if accounts.is_empty() {
        if let Ok(addr) = parse_address(EXPLORER_FALLBACK_ADDRESS) {
            let dummy = Account {
                id: AccountId::new(),
                name: "Example (add an account)".into(),
                address: addr,
                account_type: AccountType::Imported,
                index: None,
            };
            accounts.push(dummy.clone());
            wallet_state
                .replace_accounts_and_active(accounts, Some(dummy))
                .await;
            return;
        }
    }

    wallet_state
        .replace_accounts_and_active(accounts, active)
        .await;
}

/// When `session_password` is set, align `state.json` with the mnemonic and keyring (stale Tauri rows,
/// orphaned imports), then mirror into [`WalletState`].
pub async fn reconcile_and_sync_wallet_state(
    wallet_state: &WalletState,
    mgr: &AccountManager,
    session_password: Option<&str>,
) {
    if let Some(pw) = session_password {
        if let Err(e) = mgr.reconcile_persisted_accounts_with_seed(pw).await {
            tracing::warn!(
                target: "vaughan_gui",
                "Account reconciliation failed (continuing): {}",
                e
            );
        }
    }
    sync_wallet_state_with_account_manager(wallet_state, mgr).await;
}

/// Rebuild the default EVM adapter after [`NetworkService::set_active_network`].
pub async fn refresh_evm_adapter_for_active_network(
    wallet_state: &WalletState,
    network_service: &NetworkService,
) {
    if let Ok(a) = evm_adapter_for_network_service(network_service).await {
        register_default_evm_adapter(wallet_state, a).await;
    }
}
