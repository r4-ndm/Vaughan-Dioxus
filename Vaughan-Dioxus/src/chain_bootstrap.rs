//! Default EVM RPC adapter and wallet address resolution shared by dashboard, history, and similar views.

use std::sync::Arc;

use vaughan_core::chains::evm::networks::get_network_by_id;
use vaughan_core::chains::evm::utils::parse_address;
use vaughan_core::chains::evm::EvmAdapter;
use vaughan_core::chains::{ChainAdapter, ChainType};
use vaughan_core::core::account::AccountManager;
use vaughan_core::core::{Account, AccountId, AccountType, NetworkConfig, NetworkService, WalletState};
use vaughan_core::error::WalletError;

/// Only used when there is no account in [`AccountManager`] yet (e.g. edge cases before onboarding completes).
const EXPLORER_FALLBACK_ADDRESS: &str = "0xd8dA6BF26964aF9D7eEd9e03E53415D37aA96045";

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

/// If [`WalletState`] has no active account, copy the primary persisted account (or a safe fallback).
pub async fn ensure_wallet_state_active_account(wallet_state: &WalletState, mgr: &AccountManager) {
    if wallet_state.active_account().await.is_some() {
        return;
    }

    let from_manager = if let Some(a) = mgr.active_account().await {
        Some(a)
    } else {
        mgr.list_accounts().await.into_iter().next()
    };

    if let Some(a) = from_manager {
        wallet_state.add_account(a).await;
    } else if let Ok(addr) = parse_address(EXPLORER_FALLBACK_ADDRESS) {
        wallet_state
            .add_account(Account {
                id: AccountId::new(),
                name: "Example (add an account)".into(),
                address: addr,
                account_type: AccountType::Imported,
                index: None,
            })
            .await;
    }
}
