use dioxus::prelude::*;

use std::sync::Arc;
use std::time::Duration;

use futures_util::StreamExt;

use vaughan_core::chains::ChainAdapter;
use vaughan_core::core::WalletState;
use vaughan_core::error::WalletError;
use vaughan_core::monitoring::BalanceEvent;
use vaughan_core::monitoring::BalanceWatcher;

use crate::app::AppRuntime;
use crate::components::BalanceDisplay;
use crate::services::AppServices;

#[derive(Debug, Clone)]
pub enum DashboardCmd {
    Start,
    RefreshOnce,
}

fn format_balance(b: &vaughan_core::chains::Balance) -> String {
    // `formatted` is produced by the adapter; keep it as the UI display value.
    let s = b.formatted.trim();
    if s.is_empty() {
        "0.00".into()
    } else {
        s.to_string()
    }
}

#[component]
pub fn DashboardView(cmd_tx: Coroutine<DashboardCmd>, on_go_send: Callback<()>) -> Element {
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
        div { style: "display: flex; flex-direction: column; gap: 16px;",
            BalanceDisplay { amount: display_balance, symbol: display_symbol }

            div { style: "display: flex; gap: 8px; flex-wrap: wrap;",
                button {
                    class: "vaughan-btn",
                    style: "flex: 1; min-width: 120px;",
                    onclick: move |_| cmd_tx.send(DashboardCmd::RefreshOnce),
                    "Refresh balance"
                }
                button {
                    class: "vaughan-btn",
                    style: "flex: 1; min-width: 120px;",
                    onclick: move |_| cmd_tx.send(DashboardCmd::Start),
                    "Start watcher"
                }
            }

            div { class: "card-panel",
                div { style: "display: flex; flex-direction: column; gap: 12px;",
                    div {
                        label { class: "field-label", "To address" }
                        input {
                            class: "input-std input-mono",
                            r#type: "text",
                            placeholder: "Recipient address (0x...)",
                            disabled: true,
                        }
                    }
                    div {
                        label { class: "field-label", "Amount" }
                        input {
                            class: "input-std input-mono",
                            r#type: "text",
                            placeholder: "0.0",
                            disabled: true,
                        }
                    }
                    p { class: "muted", style: "margin: 0; font-size: 11px;",
                        "Opens the Send screen for estimation and broadcast (same flow as Vaughan/web)."
                    }
                    button {
                        r#type: "button",
                        class: "vaughan-btn",
                        style: "margin-top: 8px;",
                        onclick: move |_| on_go_send.call(()),
                        "Send"
                    }
                }
            }

            div { class: "card-panel",
                p { class: "section-label", "Balance events (latest first)" }
                div { style: "display: flex; flex-direction: column; gap: 8px;",
                    for (idx, ev) in runtime.balance_events.read().iter().rev().take(5).enumerate() {
                        div { key: "{idx}",
                            style: "font-family: var(--font-mono); font-size: 12px; color: var(--muted-foreground);",
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
    let services: AppServices = use_context();

    use_coroutine(move |mut rx: UnboundedReceiver<DashboardCmd>| {
        let wallet_state = wallet_state.clone();
        let mut runtime = runtime.clone();
        let services = services.clone();

        async move {
            let adapter: Arc<dyn ChainAdapter> =
                match crate::chain_bootstrap::evm_adapter_for_network_service(
                    services.network_service.as_ref(),
                )
                .await
                {
                    Ok(a) => a,
                    Err(e) => {
                        tracing::error!(target: "vaughan_gui", error = %e, "default EVM adapter failed");
                        return;
                    }
                };

            let mut watcher: Option<BalanceWatcher> = None;
            let mut updates_rx: Option<tokio::sync::mpsc::UnboundedReceiver<BalanceEvent>> = None;

            loop {
                tokio::select! {
                    cmd = rx.next() => {
                        let Some(cmd) = cmd else { break; };
                        match cmd {
                            DashboardCmd::Start => {
                                crate::chain_bootstrap::register_default_evm_adapter(
                                    &wallet_state,
                                    adapter.clone(),
                                )
                                .await;
                                crate::chain_bootstrap::ensure_wallet_state_active_account(
                                    &wallet_state,
                                    services.account_manager.as_ref(),
                                )
                                .await;

                                // Start the real BalanceWatcher and route updates into signals (Task 16.3).
                                if watcher.is_none() {
                                    let watch_addr = crate::chain_bootstrap::primary_wallet_address_hex(
                                        services.account_manager.as_ref(),
                                    )
                                    .await;
                                    let (tx, rx2) = tokio::sync::mpsc::unbounded_channel::<BalanceEvent>();
                                    watcher = Some(BalanceWatcher::start(
                                        adapter.clone(),
                                        watch_addr,
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
