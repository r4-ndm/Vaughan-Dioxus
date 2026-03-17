use dioxus::prelude::*;

use std::sync::Arc;
use std::time::Duration;

use futures_util::StreamExt;

use vaughan_core::chains::{ChainAdapter, ChainType};
use vaughan_core::chains::evm::EvmAdapter;
use vaughan_core::chains::evm::networks::get_network_by_id;
use vaughan_core::chains::evm::utils::parse_address;
use vaughan_core::core::{Account, AccountId, AccountType, WalletState};
use vaughan_core::error::WalletError;
use vaughan_core::monitoring::BalanceEvent;
use vaughan_core::monitoring::BalanceWatcher;

use crate::app::AppRuntime;
use crate::components::{AddressDisplay, BalanceDisplay};

#[derive(Debug, Clone)]
pub enum DashboardCmd {
    Start,
    RefreshOnce,
}

fn format_balance(b: &vaughan_core::chains::Balance) -> String {
    // `formatted` is produced by the adapter; keep it as the UI display value.
    let s = b.formatted.trim();
    if s.is_empty() { "0.00".into() } else { s.to_string() }
}

#[component]
pub fn DashboardView(cmd_tx: Coroutine<DashboardCmd>) -> Element {
    let _wallet_state: Arc<WalletState> = use_context();
    let runtime: AppRuntime = use_context();

    let display_balance = runtime
        .balance
        .read()
        .as_ref()
        .map(format_balance)
        .unwrap_or_else(|| "—".into());

    let display_symbol = runtime
        .balance
        .read()
        .as_ref()
        .map(|b| b.token.symbol.clone())
        .unwrap_or_else(|| "ETH".into());

    rsx! {
        div { style: "display: flex; flex-direction: column; gap: 12px;",
            BalanceDisplay { amount: display_balance, symbol: display_symbol }

            AddressDisplay { address: "0xd8dA6BF26964aF9D7eEd9e03E53415D37aA96045".to_string() }

            div { style: "display: flex; gap: 8px;",
                button { class: "btn", onclick: move |_| cmd_tx.send(DashboardCmd::RefreshOnce), "Refresh" }
                button { class: "btn", onclick: move |_| cmd_tx.send(DashboardCmd::Start), "Start watcher" }
            }

            div { style: "border: 1px solid var(--border); background: var(--card); padding: 14px;",
                p { class: "muted", style: "margin: 0; font-size: 12px;", "Balance events (latest first)" }
                div { style: "display: flex; flex-direction: column; gap: 6px; margin-top: 8px;" ,
                    for (idx, ev) in runtime.balance_events.read().iter().rev().take(5).enumerate() {
                        div { key: "{idx}", style: "font-family: var(--font-mono); font-size: 12px; opacity: 0.9;",
                            "{ev.balance.token.symbol} {ev.balance.formatted}"
                        }
                    }
                }
            }
        }
    }
}

pub fn use_dashboard_coroutine() -> Coroutine<DashboardCmd> {
    let wallet_state: Arc<WalletState> = use_context();
    let runtime: AppRuntime = use_context();

    use_coroutine(move |mut rx: UnboundedReceiver<DashboardCmd>| {
        let wallet_state = wallet_state.clone();
        let mut runtime = runtime.clone();

        async move {
            // Demo address: Vitalik. Replace with real AccountManager wiring later.
            let demo_addr = parse_address("0xd8dA6BF26964aF9D7eEd9e03E53415D37aA96045")
                .expect("Hardcoded demo address should parse");

            // Default network: ethereum mainnet from our built-in list.
            let net = get_network_by_id("ethereum").expect("built-in ethereum network");

            let adapter: Arc<dyn ChainAdapter> = Arc::new(
                EvmAdapter::new(&net.rpc_url, net.chain_id, net.name.clone())
                    .await
                    .expect("EvmAdapter should construct"),
            );

            let mut watcher: Option<BalanceWatcher> = None;
            let mut updates_rx: Option<tokio::sync::mpsc::UnboundedReceiver<BalanceEvent>> = None;

            loop {
                tokio::select! {
                    cmd = rx.next() => {
                        let Some(cmd) = cmd else { break; };
                        match cmd {
                            DashboardCmd::Start => {
                                // Seed minimal WalletState.
                                wallet_state.register_adapter(ChainType::Evm, adapter.clone()).await;
                                wallet_state.set_locked(false).await;

                                // Add a demo account if none exists.
                                if wallet_state.active_account().await.is_none() {
                                    wallet_state
                                        .add_account(Account {
                                            id: AccountId::new(),
                                            name: "Demo Account".into(),
                                            address: demo_addr,
                                            account_type: AccountType::Imported,
                                            index: None,
                                        })
                                        .await;
                                }

                                // Start the real BalanceWatcher and route updates into signals (Task 16.3).
                                if watcher.is_none() {
                                    let (tx, rx2) = tokio::sync::mpsc::unbounded_channel::<BalanceEvent>();
                                    watcher = Some(BalanceWatcher::start(
                                        adapter.clone(),
                                        format!("{:?}", demo_addr),
                                        Duration::from_secs(10),
                                        tx,
                                    ));
                                    updates_rx = Some(rx2);
                                }
                            }
                            DashboardCmd::RefreshOnce => {
                                let bal = match wallet_state.get_active_balance().await {
                                    Ok(b) => Some(b),
                                    Err(WalletError::WalletLocked) => None,
                                    Err(_) => None,
                                };
                                if let Some(b) = bal {
                                    runtime.balance.set(Some(b.clone()));
                                    runtime
                                        .balance_events
                                        .with_mut(|v: &mut Vec<BalanceEvent>| v.push(BalanceEvent { balance: b }));
                                }
                            }
                        }
                    }
                    ev = async {
                        match updates_rx.as_mut() {
                            Some(r) => r.recv().await,
                            None => None,
                        }
                    }, if updates_rx.is_some() => {
                        if let Some(ev) = ev {
                            runtime.balance.set(Some(ev.balance.clone()));
                            runtime
                                .balance_events
                                .with_mut(|v: &mut Vec<BalanceEvent>| v.push(ev));
                        }
                    }
                }
            }

            if let Some(w) = watcher.take() {
                w.stop().await;
            }
        }
    })
}

