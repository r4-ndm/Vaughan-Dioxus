use dioxus::prelude::*;

use futures_util::StreamExt;
use std::sync::Arc;
use std::time::Duration;

use crate::components::{SubpageToolbar, TxStatusBadge};
use crate::services::AppServices;
use vaughan_core::chains::{ChainAdapter, TxRecord};
use vaughan_core::core::WalletState;
use vaughan_core::error::retry_async_transient;

#[derive(Debug, Clone)]
pub enum HistoryCmd {
    Refresh,
}

#[derive(Clone)]
pub struct HistoryRuntime {
    pub query: Signal<String>,
    pub loading: Signal<bool>,
    pub error: Signal<Option<String>>,
    pub items: Signal<Vec<TxRecord>>,
}

pub fn provide_history_runtime() -> HistoryRuntime {
    HistoryRuntime {
        query: use_signal(|| "".into()),
        loading: use_signal(|| false),
        error: use_signal(|| None),
        items: use_signal(|| Vec::new()),
    }
}

fn matches_query(tx: &TxRecord, q: &str) -> bool {
    if q.is_empty() {
        return true;
    }
    let q = q.to_ascii_lowercase();
    tx.hash.to_ascii_lowercase().contains(&q)
        || tx.from.to_ascii_lowercase().contains(&q)
        || tx.to.to_ascii_lowercase().contains(&q)
        || tx.value.to_ascii_lowercase().contains(&q)
        || tx
            .token_symbol
            .as_deref()
            .unwrap_or("")
            .to_ascii_lowercase()
            .contains(&q)
        || tx
            .token_address
            .as_deref()
            .unwrap_or("")
            .to_ascii_lowercase()
            .contains(&q)
}

#[component]
pub fn HistoryView(cmd_tx: Coroutine<HistoryCmd>, on_back: Callback<()>) -> Element {
    let mut rt: HistoryRuntime = use_context();

    use_effect(move || {
        cmd_tx.send(HistoryCmd::Refresh);
    });

    let q = rt.query.read().clone();
    let filtered: Vec<TxRecord> = rt
        .items
        .read()
        .iter()
        .cloned()
        .filter(|t| matches_query(t, &q))
        .collect();

    rsx! {
        div { style: "display: flex; flex-direction: column; gap: 16px;",
            div { class: "history-toolbar",
                SubpageToolbar { title: "Transaction History", on_back: on_back.clone() }
                button {
                    class: "vaughan-btn",
                    style: "width: auto; min-width: 100px;",
                    disabled: *rt.loading.read(),
                    onclick: move |_| cmd_tx.send(HistoryCmd::Refresh),
                    "Refresh"
                }
            }

            input {
                class: "input-std input-mono",
                value: "{rt.query.read()}",
                oninput: move |e| *rt.query.write() = e.value(),
                placeholder: "Search hash / from / to / token…"
            }

            if *rt.loading.read() {
                p { class: "muted", style: "text-align: center;", "Fetching transaction history…" }
            }

            if let Some(err) = rt.error.read().as_ref() {
                div { style: "border: 1px solid rgba(239,68,68,0.3); background: var(--error-bg); padding: 12px; border-radius: 8px;",
                    p { style: "margin: 0; color: var(--error-text); font-size: 13px;", "{err}" }
                }
            }

            div { class: "history-shell", style: "padding: 16px;",
                if filtered.is_empty() && !*rt.loading.read() {
                    div { style: "display: flex; flex-direction: column; align-items: center; justify-content: center; min-height: 240px; gap: 12px; text-align: center;",
                        div { class: "tx-icon-wrap", style: "background: var(--secondary); font-size: 22px;", "🕐" }
                        h3 { style: "margin: 0; font-size: 1rem;", "No transactions yet" }
                        p { class: "muted", style: "margin: 0; font-size: 13px;", "This address has no transaction history." }
                    }
                } else {
                    div {
                        p { class: "section-label", style: "margin-bottom: 12px;",
                            "{filtered.len()} transactions"
                        }
                        for (idx, tx) in filtered.iter().take(50).enumerate() {
                            div { key: "{idx}", class: "tx-row",
                                div { style: "display: flex; align-items: flex-start; gap: 12px;",
                                    div { class: "tx-icon-wrap tx-icon-out", "↗" }
                                    div { style: "flex: 1; min-width: 0;",
                                        div { style: "display: flex; justify-content: space-between; gap: 8px; align-items: baseline;",
                                            span { style: "font-family: var(--font-mono); font-size: 11px; word-break: break-all;",
                                                "{tx.hash}"
                                            }
                                            TxStatusBadge { status: tx.status }
                                        }
                                        p { class: "muted", style: "margin: 6px 0 0 0; font-size: 11px; font-family: var(--font-mono);",
                                            "from {tx.from} → to {tx.to}"
                                        }
                                        div { style: "margin-top: 8px; display: flex; justify-content: space-between; gap: 8px;",
                                            span { style: "font-size: 14px; font-weight: 600; color: #eab308;",
                                                if tx.is_token_transfer {
                                                    "{tx.value} {tx.token_symbol.as_deref().unwrap_or(\"TOKEN\")}"
                                                } else {
                                                    "{tx.value}"
                                                }
                                            }
                                            span { class: "muted", style: "font-size: 11px;",
                                                if tx.is_token_transfer { "ERC-20" } else { "Native" }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}

pub fn use_history_coroutine() -> Coroutine<HistoryCmd> {
    let wallet_state: Arc<WalletState> = use_context();
    let services: AppServices = use_context();
    let rt: HistoryRuntime = use_context();

    use_coroutine(move |mut rx: UnboundedReceiver<HistoryCmd>| {
        let wallet_state = wallet_state.clone();
        let services = services.clone();
        let mut rt2 = rt.clone();

        async move {
            let adapter: Arc<dyn ChainAdapter> =
                match crate::chain_bootstrap::evm_adapter_for_network_service(
                    services.network_service.as_ref(),
                )
                .await
                {
                    Ok(a) => a,
                    Err(e) => {
                        let msg = e.user_message();
                        while let Some(HistoryCmd::Refresh) = rx.next().await {
                            rt2.loading.set(true);
                            rt2.error.set(Some(msg.clone()));
                            rt2.items.set(Vec::new());
                            rt2.loading.set(false);
                        }
                        return;
                    }
                };

            crate::chain_bootstrap::register_default_evm_adapter(&wallet_state, adapter.clone())
                .await;

            let history_svc = services.history_service.clone();
            let mut ticker = tokio::time::interval(Duration::from_secs(10));
            let mut poll_status = false;

            loop {
                tokio::select! {
                    cmd = rx.next() => {
                        let Some(cmd) = cmd else { break; };
                        match cmd {
                            HistoryCmd::Refresh => {
                                rt2.loading.set(true);
                                rt2.error.set(None);

                                let address = crate::chain_bootstrap::primary_wallet_address_hex(
                                    services.account_manager.as_ref(),
                                )
                                .await;

                                let limit = 100u32;
                                let history_h = history_svc.clone();
                                let adapter_h = adapter.clone();
                                let address_h = address.clone();
                                let fetch_result = retry_async_transient(
                                    move || {
                                        let history_h = history_h.clone();
                                        let adapter_h = adapter_h.clone();
                                        let address_h = address_h.clone();
                                        async move {
                                            let (native, token) = tokio::join!(
                                                history_h.get_transactions(
                                                    adapter_h.as_ref(),
                                                    address_h.as_str(),
                                                    limit,
                                                ),
                                                history_h.get_token_transfers(
                                                    adapter_h.as_ref(),
                                                    address_h.as_str(),
                                                    limit,
                                                ),
                                            );
                                            match (native, token) {
                                                (Ok(mut a), Ok(mut b)) => {
                                                    a.append(&mut b);
                                                    a.sort_by(|x, y| y.timestamp.cmp(&x.timestamp));
                                                    Ok(a)
                                                }
                                                (Err(e), _) | (_, Err(e)) => Err(e),
                                            }
                                        }
                                    },
                                    4,
                                    Duration::from_millis(400),
                                )
                                .await;

                                match fetch_result {
                                    Ok(a) => {
                                        poll_status = a.iter().any(|t| {
                                            matches!(t.status, vaughan_core::chains::TxStatus::Pending)
                                        });
                                        rt2.items.set(a);
                                    }
                                    Err(e) => {
                                        rt2.error.set(Some(e.user_message()));
                                    }
                                }

                                rt2.loading.set(false);
                            }
                        }
                    }
                    _ = ticker.tick(), if poll_status => {
                        // Update status for up to 10 pending txs to avoid spamming RPC.
                        let mut items = rt2.items.read().clone();
                        let mut changed = false;
                        let mut pending_count = 0usize;

                        for tx in items.iter_mut() {
                            if matches!(tx.status, vaughan_core::chains::TxStatus::Pending) {
                                pending_count += 1;
                                if pending_count > 10 { break; }
                                if let Ok(s) = retry_async_transient(
                                    || {
                                        let hash = tx.hash.clone();
                                        let adapter = adapter.clone();
                                        async move { adapter.get_tx_status(&hash).await }
                                    },
                                    3,
                                    Duration::from_millis(150),
                                )
                                .await
                                {
                                    if s != tx.status {
                                        tx.status = s;
                                        changed = true;
                                    }
                                }
                            }
                        }

                        if changed {
                            rt2.items.set(items);
                        }

                        poll_status = rt2.items.read().iter().any(|t| matches!(t.status, vaughan_core::chains::TxStatus::Pending));
                    }
                }
            }
        }
    })
}
