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

#[derive(Debug, Clone, PartialEq, Eq)]
struct AdapterNetworkKey {
    id: String,
    rpc_url: String,
    chain_id: u64,
}

async fn current_network_key(services: &AppServices) -> AdapterNetworkKey {
    if let Some(n) = services.network_service.active_network().await {
        AdapterNetworkKey {
            id: n.id,
            rpc_url: n.rpc_url,
            chain_id: n.chain_id,
        }
    } else {
        AdapterNetworkKey {
            id: "ethereum".into(),
            rpc_url: String::new(),
            chain_id: 1,
        }
    }
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
        items: use_signal(Vec::new),
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

fn format_decimal_amount(raw: &str, decimals: Option<u8>) -> String {
    let Some(decimals) = decimals else {
        return raw.to_string();
    };
    let raw = raw.trim();
    if raw.is_empty() || !raw.chars().all(|c| c.is_ascii_digit()) {
        return raw.to_string();
    }

    let mut digits = raw.trim_start_matches('0').to_string();
    if digits.is_empty() {
        digits = "0".to_string();
    }
    let decimals = decimals as usize;
    if decimals == 0 {
        return digits;
    }

    let (int_part, frac_part_full) = if digits.len() > decimals {
        let split_at = digits.len() - decimals;
        (&digits[..split_at], &digits[split_at..])
    } else {
        ("0", "")
    };

    let frac_owned = if digits.len() > decimals {
        frac_part_full.to_string()
    } else {
        let mut f = String::with_capacity(decimals);
        f.push_str(&"0".repeat(decimals - digits.len()));
        f.push_str(&digits);
        f
    };

    // Keep display compact in list rows while preserving significance.
    let frac = frac_owned
        .chars()
        .take(6)
        .collect::<String>()
        .trim_end_matches('0')
        .to_string();
    if frac.is_empty() {
        int_part.to_string()
    } else {
        format!("{int_part}.{frac}")
    }
}

fn tx_amount_label(tx: &TxRecord) -> String {
    if tx.is_token_transfer {
        let symbol = tx.token_symbol.as_deref().unwrap_or("TOKEN");
        let amount = format_decimal_amount(&tx.value, tx.token_decimals);
        format!("{amount} {symbol}")
    } else {
        tx.value.clone()
    }
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
        .filter(|&t| matches_query(t, &q))
        .cloned()
        .collect();

    rsx! {
        div { style: "display: flex; flex-direction: column; gap: 16px;",
            div { class: "history-toolbar",
                SubpageToolbar { title: "Transaction History", on_back: on_back }
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
                                                "{tx_amount_label(tx)}"
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
            let mut adapter: Arc<dyn ChainAdapter> =
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
            let mut adapter_network_key = current_network_key(&services).await;

            crate::chain_bootstrap::register_default_evm_adapter(&wallet_state, adapter.clone())
                .await;
            let pw = services.session_password().await;
            crate::chain_bootstrap::reconcile_and_sync_wallet_state(
                &wallet_state,
                services.account_manager.as_ref(),
                pw.as_deref(),
            )
            .await;

            let history_svc = services.history_service.clone();
            let mut ticker = tokio::time::interval(Duration::from_secs(10));
            let mut network_tick = tokio::time::interval(Duration::from_secs(2));
            let mut poll_status = false;
            let mut refresh_requested = false;

            loop {
                tokio::select! {
                    cmd = rx.next() => {
                        let Some(cmd) = cmd else { break; };
                        match cmd {
                            HistoryCmd::Refresh => {
                                refresh_requested = true;
                            }
                        }
                    }
                    _ = network_tick.tick() => {
                        let key = current_network_key(&services).await;
                        if key != adapter_network_key {
                            match crate::chain_bootstrap::evm_adapter_for_network_service(
                                services.network_service.as_ref(),
                            )
                            .await
                            {
                                Ok(next_adapter) => {
                                    adapter = next_adapter;
                                    adapter_network_key = key;
                                    crate::chain_bootstrap::register_default_evm_adapter(&wallet_state, adapter.clone()).await;
                                    refresh_requested = true;
                                }
                                Err(e) => {
                                    rt2.error.set(Some(e.user_message()));
                                }
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
                    _ = async {}, if refresh_requested => {
                        refresh_requested = false;
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
        }
    })
}

#[cfg(test)]
mod tests {
    use super::format_decimal_amount;

    #[test]
    fn formats_with_decimals_and_trims() {
        assert_eq!(format_decimal_amount("1234500", Some(6)), "1.2345");
        assert_eq!(format_decimal_amount("1000000", Some(6)), "1");
    }

    #[test]
    fn formats_small_values_below_one() {
        assert_eq!(format_decimal_amount("1", Some(6)), "0.000001");
        assert_eq!(format_decimal_amount("12", Some(6)), "0.000012");
    }

    #[test]
    fn falls_back_when_decimals_missing_or_invalid() {
        assert_eq!(format_decimal_amount("12345", None), "12345");
        assert_eq!(format_decimal_amount("  ", Some(18)), "");
        assert_eq!(format_decimal_amount("abc", Some(18)), "abc");
    }
}
