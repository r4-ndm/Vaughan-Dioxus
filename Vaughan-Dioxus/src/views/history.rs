use dioxus::prelude::*;

use std::sync::Arc;
use std::time::Duration;
use futures_util::StreamExt;

use vaughan_core::chains::{ChainAdapter, TxRecord};
use vaughan_core::chains::evm::EvmAdapter;
use vaughan_core::chains::evm::networks::get_network_by_id;
use vaughan_core::core::{HistoryService, WalletState};
use vaughan_core::error::retry_async;
use crate::app::AppServices;
use crate::components::TxStatusBadge;

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
pub fn HistoryView(cmd_tx: Coroutine<HistoryCmd>) -> Element {
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
        div { style: "display: flex; flex-direction: column; gap: 12px;",
            h2 { "History" }

            div { style: "display: flex; gap: 8px; align-items: center;",
                input {
                    value: "{rt.query.read()}",
                    oninput: move |e| *rt.query.write() = e.value(),
                    style: "flex: 1; padding: 10px 12px; background: var(--bg); border: 1px solid var(--border); color: var(--fg); font-family: var(--font-mono); font-size: 12px;",
                    placeholder: "Search hash / from / to / token…"
                }
                button {
                    class: "btn",
                    disabled: *rt.loading.read(),
                    onclick: move |_| cmd_tx.send(HistoryCmd::Refresh),
                    "Refresh"
                }
            }

            if *rt.loading.read() {
                p { class: "muted", "Loading…" }
            }

            if let Some(err) = rt.error.read().as_ref() {
                div { style: "border: 1px solid #442; background: #110; padding: 12px;",
                    p { style: "margin: 0; color: #f5b;", "{err}" }
                }
            }

            div { style: "display: flex; flex-direction: column; gap: 10px;",
                for (idx, tx) in filtered.iter().take(50).enumerate() {
                    div { key: "{idx}", style: "border: 1px solid var(--border); background: var(--card); padding: 12px;",
                        div { style: "display: flex; justify-content: space-between; gap: 8px; align-items: baseline;",
                            span { style: "font-family: var(--font-mono); font-size: 12px;", "{tx.hash}" }
                            TxStatusBadge { status: tx.status }
                        }
                        div { class: "muted", style: "margin-top: 6px; font-size: 12px; font-family: var(--font-mono);",
                            "from={tx.from} → to={tx.to}"
                        }
                        div { style: "margin-top: 6px; display: flex; justify-content: space-between; gap: 8px; align-items: baseline;",
                            span { style: "font-size: 14px; font-weight: 600;",
                                if tx.is_token_transfer {
                                    "{tx.value} {tx.token_symbol.as_deref().unwrap_or(\"TOKEN\")}"
                                } else {
                                    "{tx.value}"
                                }
                            }
                            span { class: "muted", style: "font-size: 12px;",
                                if tx.is_token_transfer { "erc20" } else { "native" }
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
            // Ensure adapter exists (same default as Dashboard).
            let net = get_network_by_id("ethereum").expect("built-in ethereum network");
            let adapter: Arc<dyn ChainAdapter> = Arc::new(
                EvmAdapter::new(&net.rpc_url, net.chain_id, net.name.clone())
                    .await
                    .expect("EvmAdapter should construct"),
            );
            wallet_state.register_adapter(vaughan_core::chains::ChainType::Evm, adapter.clone()).await;
            wallet_state.set_locked(false).await;

            // Same demo address as Dashboard until account mgmt is wired.
            let address = "0xd8dA6BF26964aF9D7eEd9e03E53415D37aA96045";

            let history: &HistoryService = services.history_service.as_ref();
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

                                let limit = 100u32;
                                let (native, token) = tokio::join!(
                                    history.get_transactions(adapter.as_ref(), address, limit),
                                    history.get_token_transfers(adapter.as_ref(), address, limit),
                                );

                                match (native, token) {
                                    (Ok(mut a), Ok(mut b)) => {
                                        a.append(&mut b);
                                        a.sort_by(|x, y| y.timestamp.cmp(&x.timestamp));
                                        poll_status = a.iter().any(|t| matches!(t.status, vaughan_core::chains::TxStatus::Pending));
                                        rt2.items.set(a);
                                    }
                                    (Err(e), _) | (_, Err(e)) => {
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
                                if let Ok(s) = retry_async(
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

