//! Shared network/account selector polling and active-account handlers.

use std::time::Duration;

use dioxus::prelude::*;

use vaughan_core::core::WalletState;
use vaughan_core::core::address_to_hex;
use vaughan_core::monitoring::BalanceEvent;

use crate::app::AppRuntime;
use crate::components::{AccountOption, NetworkOption};
use crate::services::AppServices;

/// Latest network/account selector state from persistence services.
#[derive(Debug, Clone)]
pub struct WalletSelectorSnapshot {
    pub networks: Vec<NetworkOption>,
    pub active_network_id: Option<String>,
    pub active_chain_id: u64,
    pub accounts: Vec<AccountOption>,
    pub active_account_address: Option<String>,
}

/// Poll interval from user preferences (minimum 1 second).
pub async fn selector_poll_interval(services: &AppServices) -> Duration {
    let secs = services
        .persistence
        .snapshot()
        .preferences
        .map(|p| p.polling_interval_secs)
        .unwrap_or(2);
    Duration::from_secs(secs.max(1))
}

/// Fetch current network/account lists for selector widgets.
pub async fn poll_wallet_selectors(services: &AppServices) -> WalletSelectorSnapshot {
    let networks = services
        .network_service
        .list_networks()
        .await
        .into_iter()
        .map(|n| NetworkOption {
            id: n.id,
            name: n.name,
            chain_id: n.chain_id,
        })
        .collect::<Vec<_>>();
    let active_network_id = services
        .network_service
        .active_network()
        .await
        .map(|n| n.id);
    let active_chain_id = services
        .network_service
        .active_network()
        .await
        .map(|n| n.chain_id)
        .unwrap_or(1);
    let accounts = services
        .account_manager
        .list_accounts()
        .await
        .into_iter()
        .map(|a| AccountOption {
            name: a.name,
            address: address_to_hex(a.address),
            account_type: a.account_type,
        })
        .collect::<Vec<_>>();
    let active_account_address = services
        .account_manager
        .active_account()
        .await
        .map(|a| address_to_hex(a.address));

    WalletSelectorSnapshot {
        networks,
        active_network_id,
        active_chain_id,
        accounts,
        active_account_address,
    }
}

/// Apply a selector snapshot to Dioxus signals (optional chain id for dApps view).
pub fn apply_wallet_selector_snapshot(
    snap: WalletSelectorSnapshot,
    mut networks: Signal<Vec<NetworkOption>>,
    mut active_network_id: Signal<Option<String>>,
    mut accounts: Signal<Vec<AccountOption>>,
    mut active_account_address: Signal<Option<String>>,
    active_chain_id: Option<Signal<u64>>,
) {
    networks.set(snap.networks);
    active_network_id.set(snap.active_network_id);
    accounts.set(snap.accounts);
    active_account_address.set(snap.active_account_address);
    if let Some(mut chain) = active_chain_id {
        chain.set(snap.active_chain_id);
    }
}

/// Background poll loop for network/account selector widgets.
pub fn spawn_wallet_selector_poll(
    services: AppServices,
    networks: Signal<Vec<NetworkOption>>,
    active_network_id: Signal<Option<String>>,
    accounts: Signal<Vec<AccountOption>>,
    active_account_address: Signal<Option<String>>,
    active_chain_id: Option<Signal<u64>>,
) {
    spawn(async move {
        loop {
            let snap = poll_wallet_selectors(&services).await;
            apply_wallet_selector_snapshot(
                snap,
                networks,
                active_network_id,
                accounts,
                active_account_address,
                active_chain_id,
            );
            let interval = selector_poll_interval(&services).await;
            tokio::time::sleep(interval).await;
        }
    });
}

/// Switch active account in persistence + wallet state and refresh balance signals.
pub async fn set_active_account_and_refresh(
    services: &AppServices,
    wallet_state: &WalletState,
    runtime: &mut AppRuntime,
    address: &str,
) {
    let acc = services
        .account_manager
        .list_accounts()
        .await
        .into_iter()
        .find(|a| address_to_hex(a.address) == address);
    let Some(acc) = acc else {
        return;
    };
    if services.account_manager.set_active(acc.id).await.is_err() {
        return;
    }
    if wallet_state.set_active_account_by_id(acc.id).await.is_err() {
        wallet_state.add_account(acc.clone()).await;
        let _ = wallet_state.set_active_account_by_id(acc.id).await;
    }
    push_active_balance(runtime, wallet_state).await;
}

async fn push_active_balance(runtime: &mut AppRuntime, wallet_state: &WalletState) {
    if let Ok(b) = wallet_state.get_active_balance().await {
        runtime.balance.set(Some(b.clone()));
        runtime
            .balance_events
            .with_mut(|v: &mut Vec<BalanceEvent>| v.push(BalanceEvent { balance: b }));
    }
}

