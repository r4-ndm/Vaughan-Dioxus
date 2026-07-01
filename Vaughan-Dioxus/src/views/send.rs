use dioxus::prelude::*;

use futures_util::StreamExt;
use std::sync::Arc;
use std::str::FromStr;
use std::time::Duration;
use alloy::primitives::U256;

use vaughan_core::chains::evm::utils::parse_address;
use vaughan_core::chains::evm::networks::get_network_by_chain_id;
use vaughan_core::chains::evm::EvmAdapter;
use vaughan_core::chains::{ChainAdapter, ChainTransaction, Fee, TxStatus};
use vaughan_core::core::wallet::WalletState;
use vaughan_core::core::{
    address_to_hex, load_active_signer, parse_optional_u64_decimal, TransactionService,
    AccountType,
};
use vaughan_core::core::ambire_abi::AmbireAccount;
use vaughan_core::core::smart_account::{build_init_code, AMBIRE_ACCOUNT_BYTECODE};
use vaughan_core::core::scw_transaction::{
    build_signed_execute, build_signed_deploy_and_execute, wrap_scw_as_chain_transaction,
    get_smart_account_nonce, is_account_deployed,
};
use vaughan_core::error::{retry_async_transient, WalletError};

use crate::components::SubpageToolbar;
use crate::services::{AppServices, shared_services};

#[derive(Debug, Clone)]
pub enum SendCmd {
    Estimate {
        to: String,
        amount_input: String,
        data: Option<String>,
        max_fee_per_gas: Option<String>,
        max_priority_fee_per_gas: Option<String>,
        nonce: Option<String>,
    },
    Send {
        to: String,
        amount_input: String,
        data: Option<String>,
        max_fee_per_gas: Option<String>,
        max_priority_fee_per_gas: Option<String>,
        nonce: Option<String>,
    },
    RefreshStatus {
        tx_hash: String,
    },
    ReplacePending {
        mode: ReplaceMode,
        tx_hash: String,
    },
}

#[derive(Debug, Clone, Copy)]
pub enum ReplaceMode {
    SpeedUp,
    Cancel,
}

#[derive(Debug, Clone)]
pub struct PendingTxContext {
    pub tx_hash: String,
    pub nonce: u64,
    pub to: String,
    pub value_wei: String,
    pub data: Option<String>,
    pub max_fee_per_gas: Option<String>,
    pub max_priority_fee_per_gas: Option<String>,
    pub status: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum QueueFilter {
    All,
    Pending,
    Confirmed,
    Failed,
    Replaced,
}

#[derive(Clone)]
pub struct SendRuntime {
    pub last_fee: Signal<Option<Fee>>,
    pub last_error: Signal<Option<String>>,
    pub last_tx: Signal<Option<String>>,
    pub pending_txs: Signal<Vec<PendingTxContext>>,
    pub busy: Signal<bool>,
}

#[component]
pub fn SendView(cmd_tx: Coroutine<SendCmd>, on_back: Callback<()>) -> Element {
    let mut rt: SendRuntime = use_context();

    let mut to = use_signal(|| "".to_string());
    let mut amount_input = use_signal(|| "".to_string());
    let mut data = use_signal(|| "".to_string());
    let mut max_fee_per_gas = use_signal(|| "".to_string());
    let mut max_priority_fee_per_gas = use_signal(|| "".to_string());
    let mut nonce_override = use_signal(|| "".to_string());
    let mut confirm_open = use_signal(|| false);
    let mut queue_filter = use_signal(|| QueueFilter::All);
    let mut auto_poll = use_signal(|| true);
    let mut auto_poll_booted = use_signal(|| false);

    let services: AppServices = use_context();
    let active_account_type = use_signal(|| None::<AccountType>);
    let _load_active = use_resource(move || {
        let services = services.clone();
        let mut active_account_type = active_account_type;
        async move {
            if let Some(acc) = services.account_manager.active_account().await {
                active_account_type.set(Some(acc.account_type));
            }
        }
    });

    let fee_text = rt
        .last_fee
        .read()
        .as_ref()
        .map(|f| {
            let max = f.max_fee_per_gas.clone().unwrap_or_else(|| "—".into());
            let prio = f
                .max_priority_fee_per_gas
                .clone()
                .unwrap_or_else(|| "—".into());
            format!(
                "gas_limit={} max_fee_per_gas={} max_priority_fee_per_gas={}",
                f.gas_limit, max, prio
            )
        })
        .unwrap_or_else(|| "—".into());
    let amount_preview = match parse_native_amount_to_wei(amount_input.read().as_str(), 18) {
        Ok(v) => v,
        Err(_) => "—".into(),
    };
    let pending_txs_snapshot = rt.pending_txs.read().clone();
    let filtered_pending_txs: Vec<PendingTxContext> = pending_txs_snapshot
        .iter()
        .filter(|p| match *queue_filter.read() {
            QueueFilter::All => true,
            QueueFilter::Pending => p.status == "Pending",
            QueueFilter::Confirmed => p.status == "Confirmed",
            QueueFilter::Failed => p.status == "Failed",
            QueueFilter::Replaced => p.status == "Replaced",
        })
        .cloned()
        .collect();

    use_effect(move || {
        if *auto_poll_booted.read() {
            return;
        }
        auto_poll_booted.set(true);
        let cmd_tx = cmd_tx;
        let pending_txs = rt.pending_txs;
        let auto_poll = auto_poll;
        spawn(async move {
            let mut ticker = tokio::time::interval(Duration::from_secs(6));
            loop {
                ticker.tick().await;
                if !*auto_poll.read() {
                    continue;
                }
                let hashes: Vec<String> = pending_txs
                    .read()
                    .iter()
                    .filter(|p| p.status == "Pending")
                    .map(|p| p.tx_hash.clone())
                    .collect();
                for tx_hash in hashes {
                    cmd_tx.send(SendCmd::RefreshStatus { tx_hash });
                }
            }
        });
    });

    rsx! {
        div { style: "display: flex; flex-direction: column; gap: 16px;",
            SubpageToolbar { title: "Send", on_back: on_back }

            div { class: "card-panel",
                label { class: "field-label", "Recipient (0x…)" }
                input {
                    class: "input-std input-mono",
                    value: "{to.read()}",
                    oninput: move |e| *to.write() = e.value(),
                    placeholder: "0x…"
                }
            }

            div { class: "card-panel",
                label { class: "field-label", "Amount (native token)" }
                input {
                    class: "input-std input-mono",
                    value: "{amount_input.read()}",
                    oninput: move |e| *amount_input.write() = e.value(),
                    placeholder: "0.01"
                }
                p { class: "muted", style: "margin: 8px 0 0 0; font-size: 11px;", "Amount in native units (e.g. PLS/ETH). Wei preview: {amount_preview}" }
            }

            div { class: "card-panel",
                label { class: "field-label", "Data (optional hex)" }
                input {
                    class: "input-std input-mono",
                    value: "{data.read()}",
                    oninput: move |e| *data.write() = e.value(),
                    placeholder: "0x"
                }
            }

            div { class: "card-panel",
                p { class: "section-label", "Advanced gas & nonce (MetaMask-style)" }
                label { class: "field-label", "Max fee per gas (wei, optional)" }
                input {
                    class: "input-std input-mono",
                    value: "{max_fee_per_gas.read()}",
                    oninput: move |e| *max_fee_per_gas.write() = e.value(),
                    placeholder: "optional"
                }
                label { class: "field-label", style: "margin-top: 8px;", "Max priority fee (wei, optional)" }
                input {
                    class: "input-std input-mono",
                    value: "{max_priority_fee_per_gas.read()}",
                    oninput: move |e| *max_priority_fee_per_gas.write() = e.value(),
                    placeholder: "optional"
                }
                label { class: "field-label", style: "margin-top: 8px;", "Nonce override (optional)" }
                input {
                    class: "input-std input-mono",
                    value: "{nonce_override.read()}",
                    oninput: move |e| *nonce_override.write() = e.value(),
                    placeholder: "optional"
                }
                div { class: "btn-row", style: "margin-top: 8px;",
                    button {
                        class: "btn",
                        disabled: rt.last_fee.read().is_none(),
                        onclick: move |_| {
                            if let Some(fee) = rt.last_fee.read().clone() {
                                if let Some(max) = fee.max_fee_per_gas {
                                    if let Ok(u) = U256::from_str(&max) {
                                        max_fee_per_gas.set((u * U256::from(9u64) / U256::from(10u64)).to_string());
                                    }
                                }
                                if let Some(prio) = fee.max_priority_fee_per_gas {
                                    if let Ok(u) = U256::from_str(&prio) {
                                        max_priority_fee_per_gas.set((u * U256::from(9u64) / U256::from(10u64)).to_string());
                                    }
                                }
                            }
                        },
                        "Low"
                    }
                    button {
                        class: "btn",
                        disabled: rt.last_fee.read().is_none(),
                        onclick: move |_| {
                            if let Some(fee) = rt.last_fee.read().clone() {
                                if let Some(max) = fee.max_fee_per_gas {
                                    max_fee_per_gas.set(max);
                                }
                                if let Some(prio) = fee.max_priority_fee_per_gas {
                                    max_priority_fee_per_gas.set(prio);
                                }
                            }
                        },
                        "Market"
                    }
                    button {
                        class: "btn",
                        disabled: rt.last_fee.read().is_none(),
                        onclick: move |_| {
                            if let Some(fee) = rt.last_fee.read().clone() {
                                if let Some(max) = fee.max_fee_per_gas {
                                    if let Ok(u) = U256::from_str(&max) {
                                        max_fee_per_gas.set((u * U256::from(12u64) / U256::from(10u64)).to_string());
                                    }
                                }
                                if let Some(prio) = fee.max_priority_fee_per_gas {
                                    if let Ok(u) = U256::from_str(&prio) {
                                        max_priority_fee_per_gas.set((u * U256::from(12u64) / U256::from(10u64)).to_string());
                                    }
                                }
                            }
                        },
                        "Aggressive"
                    }
                }
            }

            div { class: "card-panel",
                p { class: "section-label", "Estimated fee (wei)" }
                p { style: "margin: 0; font-family: var(--font-mono); font-size: 12px; color: var(--muted-foreground);",
                    "{fee_text}"
                }
            }

            if let Some(err) = rt.last_error.read().as_ref() {
                div { style: "border: 1px solid rgba(239,68,68,0.3); background: var(--error-bg); padding: 12px; border-radius: 8px;",
                    p { style: "margin: 0; color: var(--error-text); font-size: 13px;", "{err}" }
                }
            }

            if let Some(tx) = rt.last_tx.read().as_ref() {
                div { class: "card-panel",
                    p { class: "section-label", "Last tx hash" }
                    p { style: "margin: 0; font-family: var(--font-mono); font-size: 12px; word-break: break-all;",
                        "{tx}"
                    }
                }
            }

            if !pending_txs_snapshot.is_empty() {
                div { class: "card-panel",
                    p { class: "section-label", "Pending transaction queue" }
                    div { class: "btn-row", style: "margin-bottom: 10px;",
                        button { class: "btn", onclick: move |_| queue_filter.set(QueueFilter::All), "All" }
                        button { class: "btn", onclick: move |_| queue_filter.set(QueueFilter::Pending), "Pending" }
                        button { class: "btn", onclick: move |_| queue_filter.set(QueueFilter::Confirmed), "Confirmed" }
                        button { class: "btn", onclick: move |_| queue_filter.set(QueueFilter::Failed), "Failed" }
                        button { class: "btn", onclick: move |_| queue_filter.set(QueueFilter::Replaced), "Replaced" }
                        button {
                            class: "btn",
                            onclick: move |_| {
                                let next = !*auto_poll.read();
                                auto_poll.set(next);
                            },
                            if *auto_poll.read() { "Auto-refresh: On" } else { "Auto-refresh: Off" }
                        }
                    }
                    for p in filtered_pending_txs.iter().rev().take(8) {
                        div { style: "border: 1px solid var(--border-color); border-radius: 8px; padding: 10px; margin-bottom: 8px;",
                            div { style: "display: flex; justify-content: space-between; align-items: center; gap: 8px; margin-bottom: 4px;",
                                p { class: "muted", style: "margin: 0; font-size: 12px;", "Nonce {p.nonce}" }
                                span {
                                    style: "padding: 2px 8px; border-radius: 9999px; font-size: 11px; font-weight: 600; {status_chip_style(&p.status)}",
                                    "{p.status}"
                                }
                            }
                            p { style: "margin: 0 0 8px 0; font-family: var(--font-mono); font-size: 11px; word-break: break-all;",
                                "{p.tx_hash}"
                            }
                            if let Some(link) = explorer_tx_url_for_active_network(&p.tx_hash) {
                                p { class: "muted", style: "margin: 0 0 8px 0; font-size: 11px;",
                                    a {
                                        href: "#",
                                        onclick: move |_| {
                                            let _ = webbrowser::open(&link);
                                        },
                                        "View in explorer"
                                    }
                                }
                            }
                            div { class: "btn-row",
                                button {
                                    class: "btn",
                                    disabled: *rt.busy.read(),
                                    title: if *rt.busy.read() { "Another action is running." } else { "Check latest on-chain status." },
                                    onclick: {
                                        let tx_hash = p.tx_hash.clone();
                                        move |_| cmd_tx.send(SendCmd::RefreshStatus { tx_hash: tx_hash.clone() })
                                    },
                                    if p.status == "Refreshing" { "Refreshing..." } else { "Refresh" }
                                }
                                button {
                                    class: "btn",
                                    disabled: *rt.busy.read() || p.status != "Pending",
                                    title: {
                                        let reason = speedup_cancel_disabled_reason(&p.status, *rt.busy.read());
                                        if reason.is_empty() { "Resubmit same nonce with higher fees." } else { reason }
                                    },
                                    onclick: {
                                        let tx_hash = p.tx_hash.clone();
                                        move |_| cmd_tx.send(SendCmd::ReplacePending { mode: ReplaceMode::SpeedUp, tx_hash: tx_hash.clone() })
                                    },
                                    "Speed up"
                                }
                                button {
                                    class: "btn",
                                    disabled: *rt.busy.read() || p.status != "Pending",
                                    title: {
                                        let reason = speedup_cancel_disabled_reason(&p.status, *rt.busy.read());
                                        if reason.is_empty() { "Replace nonce with 0-value self-transfer." } else { reason }
                                    },
                                    onclick: {
                                        let tx_hash = p.tx_hash.clone();
                                        move |_| cmd_tx.send(SendCmd::ReplacePending { mode: ReplaceMode::Cancel, tx_hash: tx_hash.clone() })
                                    },
                                    "Cancel"
                                }
                            }
                            if *rt.busy.read() || p.status != "Pending" {
                                p {
                                    class: "muted",
                                    style: "margin: 6px 0 0 0; font-size: 11px;",
                                    "{speedup_cancel_disabled_reason(&p.status, *rt.busy.read())}"
                                }
                            }
                            p { class: "muted", style: "margin: 8px 0 0 0; font-size: 11px;",
                                "Speed up: rebroadcast same tx with higher fees. Cancel: replace nonce with a 0-value self-transfer."
                            }
                        }
                    }
                }
            }

            div { class: "btn-row",
                button {
                    class: "btn",
                    disabled: *rt.busy.read(),
                    onclick: move |_| {
                        rt.last_error.set(None);
                        rt.last_tx.set(None);
                        cmd_tx.send(SendCmd::Estimate {
                            to: to.read().clone(),
                            amount_input: amount_input.read().clone(),
                            data: {
                                let d = data.read().trim().to_string();
                                if d.is_empty() { None } else { Some(d) }
                            },
                            max_fee_per_gas: {
                                let v = max_fee_per_gas.read().trim().to_string();
                                if v.is_empty() { None } else { Some(v) }
                            },
                            max_priority_fee_per_gas: {
                                let v = max_priority_fee_per_gas.read().trim().to_string();
                                if v.is_empty() { None } else { Some(v) }
                            },
                            nonce: {
                                let v = nonce_override.read().trim().to_string();
                                if v.is_empty() { None } else { Some(v) }
                            },
                        });
                    },
                    "Estimate"
                }
                button {
                    class: "btn",
                    disabled: *rt.busy.read(),
                    onclick: move |_| *confirm_open.write() = true,
                    "Review"
                }
            }

            if *confirm_open.read() {
                div {
                    class: "modal-overlay",
                    onclick: move |_| *confirm_open.write() = false,
                    div {
                        class: "modal-sheet",
                        onclick: move |evt| evt.stop_propagation(),
                        h3 { style: "margin: 0 0 8px 0;", "Confirm send" }
                        p { class: "muted", style: "font-size: 12px; margin: 0 0 12px 0;",
                            "Review destination, amount, and fee details before signing."
                        }
                        if let Some(AccountType::SmartAccount) = *active_account_type.read() {
                            div {
                                style: "margin-bottom: 12px; font-size: 11px; background: rgba(59, 130, 246, 0.15); border: 1px solid rgba(59, 130, 246, 0.3); border-radius: 4px; padding: 10px; color: #60a5fa;",
                                h4 { style: "margin: 0 0 4px 0; font-size: 12px;", "Smart Account Execution" }
                                p { style: "margin: 0;", "This transaction will be wrapped and signed by your parent EOA owner." }
                                p { style: "margin: 4px 0 0 0; opacity: 0.8;", "If this is the first transaction, it will also deploy the smart contract wallet dynamically." }
                            }
                        }
                        p { style: "font-family: var(--font-mono); font-size: 11px; margin: 4px 0;", "to={to.read()} amount={amount_input.read()}" }
                        p { style: "font-family: var(--font-mono); font-size: 11px; margin: 4px 0;", "data={data.read()}" }
                        p { style: "font-family: var(--font-mono); font-size: 11px; margin: 4px 0;", "{fee_text}" }
                        div { class: "btn-row",
                            button { class: "btn", onclick: move |_| *confirm_open.write() = false, "Cancel" }
                            button {
                                class: "btn",
                                disabled: *rt.busy.read(),
                                onclick: move |_| {
                                    rt.last_error.set(None);
                                    rt.last_tx.set(None);
                                    cmd_tx.send(SendCmd::Send {
                                        to: to.read().clone(),
                                        amount_input: amount_input.read().clone(),
                                        data: {
                                            let d = data.read().trim().to_string();
                                            if d.is_empty() { None } else { Some(d) }
                                        },
                                        max_fee_per_gas: {
                                            let v = max_fee_per_gas.read().trim().to_string();
                                            if v.is_empty() { None } else { Some(v) }
                                        },
                                        max_priority_fee_per_gas: {
                                            let v = max_priority_fee_per_gas.read().trim().to_string();
                                            if v.is_empty() { None } else { Some(v) }
                                        },
                                        nonce: {
                                            let v = nonce_override.read().trim().to_string();
                                            if v.is_empty() { None } else { Some(v) }
                                        },
                                    });
                                    *confirm_open.write() = false;
                                },
                                "Send"
                            }
                        }
                    }
                }
            }
        }
    }
}

async fn active_evm_chain_id() -> u64 {
    shared_services()
        .network_service
        .active_network()
        .await
        .map(|n| n.chain_id)
        .unwrap_or(1)
}

async fn build_signed_adapter(chain_id: u64) -> Result<EvmAdapter, WalletError> {
    let services = shared_services();
    let net = services
        .network_service
        .active_network()
        .await
        .ok_or_else(|| WalletError::UnsupportedChain("No active network selected".into()))?;
    let password = services
        .session_password()
        .await
        .ok_or(WalletError::WalletLocked)?;
    let signer = load_active_signer(services.account_manager.as_ref(), &password).await?;
    EvmAdapter::with_signer(&net.rpc_url, chain_id, net.name, signer).await
}

async fn prepare_scw_transaction(
    services: &AppServices,
    adapter: &EvmAdapter,
    to: String,
    value_wei: String,
    data: Option<String>,
    chain_id: u64,
) -> Result<(ChainTransaction, Option<vaughan_core::core::AccountId>), WalletError> {
    let active_account = services
        .account_manager
        .active_account()
        .await
        .ok_or_else(|| WalletError::AccountNotFound("No active account".into()))?;

    if active_account.account_type != AccountType::SmartAccount {
        return Err(WalletError::Other("Not a smart account".into()));
    }

    let info = active_account
        .smart_account
        .as_ref()
        .ok_or_else(|| WalletError::Other("SmartAccount missing info".into()))?;

    let provider = adapter.provider();
    let is_deployed = is_account_deployed(active_account.address, &provider).await?;

    let inner_tx = AmbireAccount::Transaction {
        to: parse_address(&to)?,
        value: U256::from_str(&value_wei).map_err(|_| WalletError::InvalidAmount("Invalid wei amount".into()))?,
        data: data
            .map(|d| {
                let stripped = d.trim_start_matches("0x");
                hex::decode(stripped)
                    .map(alloy::primitives::Bytes::from)
                    .unwrap_or_default()
            })
            .unwrap_or_default(),
    };

    let password = services
        .session_password()
        .await
        .ok_or(WalletError::WalletLocked)?;

    let signer = load_active_signer(services.account_manager.as_ref(), &password).await?;

    if !is_deployed {
        let init_code = build_init_code(info.owner_address, AMBIRE_ACCOUNT_BYTECODE);
        let calldata = build_signed_deploy_and_execute(
            &signer,
            active_account.address,
            init_code,
            info.salt,
            vec![inner_tx],
            chain_id,
        )
        .await?;

        let wrapped_tx = wrap_scw_as_chain_transaction(
            info.owner_address,
            info.factory,
            &calldata,
            chain_id,
        );

        Ok((wrapped_tx, Some(active_account.id)))
    } else {
        let nonce = get_smart_account_nonce(active_account.address, &provider).await?;
        let calldata = build_signed_execute(
            &signer,
            active_account.address,
            vec![inner_tx],
            nonce,
            chain_id,
        )
        .await?;

        let wrapped_tx = wrap_scw_as_chain_transaction(
            info.owner_address,
            active_account.address,
            &calldata,
            chain_id,
        );

        Ok((wrapped_tx, None))
    }
}

fn validate_and_normalize_send_inputs(
    from_addr: &str,
    to: String,
    value_wei: String,
    data: Option<String>,
) -> Result<(String, String, Option<String>), WalletError> {
    let to = to.trim().to_string();
    if to.is_empty() {
        return Err(WalletError::InvalidAddress(
            "Recipient address is required".into(),
        ));
    }
    parse_address(&to)?;
    if from_addr.eq_ignore_ascii_case(&to) {
        return Err(WalletError::InvalidTransaction(
            "Sender and recipient cannot be the same address".into(),
        ));
    }

    let value_wei = value_wei.trim().to_string();
    if value_wei.is_empty() {
        return Err(WalletError::InvalidAmount("Amount is required".into()));
    }
    let parsed_value = U256::from_str(&value_wei)
        .map_err(|_| WalletError::InvalidAmount("Amount must be a decimal wei value".into()))?;
    if parsed_value.is_zero() {
        return Err(WalletError::InvalidAmount(
            "Amount must be greater than zero".into(),
        ));
    }

    let data = match data {
        None => None,
        Some(d) => {
            let t = d.trim();
            if t.is_empty() {
                None
            } else {
                let normalized = if t.starts_with("0x") || t.starts_with("0X") {
                    format!("0x{}", &t[2..])
                } else {
                    format!("0x{t}")
                };
                let raw = normalized.trim_start_matches("0x");
                if raw.len() % 2 != 0 {
                    return Err(WalletError::InvalidTransaction(
                        "Data hex must have an even number of characters".into(),
                    ));
                }
                hex::decode(raw).map_err(|_| {
                    WalletError::InvalidTransaction("Data must be valid hex".into())
                })?;
                Some(normalized)
            }
        }
    };

    Ok((to, value_wei, data))
}

fn parse_native_amount_to_wei(input: &str, decimals: u8) -> Result<String, WalletError> {
    let t = input.trim();
    if t.is_empty() {
        return Err(WalletError::InvalidAmount("Amount is required".into()));
    }
    if t.starts_with('-') {
        return Err(WalletError::InvalidAmount(
            "Amount must be non-negative".into(),
        ));
    }
    let parts: Vec<&str> = t.split('.').collect();
    if parts.len() > 2 {
        return Err(WalletError::InvalidAmount("Invalid amount format".into()));
    }
    let whole = parts[0];
    let frac = if parts.len() == 2 { parts[1] } else { "" };
    if !whole.chars().all(|c| c.is_ascii_digit()) || !frac.chars().all(|c| c.is_ascii_digit()) {
        return Err(WalletError::InvalidAmount("Invalid amount format".into()));
    }
    if frac.len() > decimals as usize {
        return Err(WalletError::InvalidAmount(format!(
            "Too many decimal places (max {decimals})"
        )));
    }
    let mut as_wei = String::new();
    as_wei.push_str(if whole.is_empty() { "0" } else { whole });
    as_wei.push_str(frac);
    for _ in 0..(decimals as usize - frac.len()) {
        as_wei.push('0');
    }
    let normalized = as_wei.trim_start_matches('0');
    Ok(if normalized.is_empty() {
        "0".into()
    } else {
        normalized.to_string()
    })
}

fn parse_optional_u256_decimal(value: Option<String>, field: &str) -> Result<Option<String>, WalletError> {
    let Some(v) = value else { return Ok(None) };
    let t = v.trim();
    if t.is_empty() {
        return Ok(None);
    }
    let parsed = U256::from_str(t)
        .map_err(|_| WalletError::InvalidAmount(format!("{field} must be a decimal wei value")))?;
    Ok(Some(parsed.to_string()))
}

fn bump_fee_12_percent_plus_one(value_wei: &str) -> Result<String, WalletError> {
    let v = U256::from_str(value_wei)
        .map_err(|_| WalletError::InvalidAmount("Invalid fee value".into()))?;
    let bumped = (v * U256::from(112u64) / U256::from(100u64)) + U256::from(1u64);
    Ok(bumped.to_string())
}

fn tx_status_label(status: TxStatus) -> &'static str {
    match status {
        TxStatus::Pending => "Pending",
        TxStatus::Confirmed => "Confirmed",
        TxStatus::Failed => "Failed",
    }
}

fn upsert_pending(list: &mut Vec<PendingTxContext>, entry: PendingTxContext) {
    if let Some(existing) = list.iter_mut().find(|e| e.tx_hash == entry.tx_hash) {
        *existing = entry;
    } else {
        list.push(entry);
    }
    if list.len() > 20 {
        let drain = list.len() - 20;
        list.drain(0..drain);
    }
}

fn set_pending_status(list: &mut [PendingTxContext], tx_hash: &str, status: &str) {
    if let Some(existing) = list.iter_mut().find(|e| e.tx_hash == tx_hash) {
        existing.status = status.to_string();
    }
}

fn map_send_error_like_metamask(e: &WalletError) -> String {
    match e {
        WalletError::WalletLocked => "MetaMask-style: wallet is locked. Unlock and retry.".into(),
        WalletError::InvalidAddress(_) => "MetaMask-style: invalid recipient address.".into(),
        WalletError::InvalidAmount(_) => "MetaMask-style: invalid transaction amount.".into(),
        WalletError::InsufficientBalance { .. } => {
            "MetaMask-style: insufficient funds for gas * price + value.".into()
        }
        WalletError::GasEstimationFailed(_) => {
            "MetaMask-style: cannot estimate gas; transaction may fail or require manual gas.".into()
        }
        WalletError::SigningFailed(msg) if msg.contains("User denied") => {
            "MetaMask-style: user rejected the signature request.".into()
        }
        WalletError::TransactionFailed(_) => {
            "MetaMask-style: transaction submission failed at RPC layer.".into()
        }
        _ => e.user_message(),
    }
}

fn status_chip_style(status: &str) -> &'static str {
    match status {
        "Pending" => "background: rgba(245,158,11,0.15); color: #f59e0b;",
        "Refreshing" => "background: rgba(59,130,246,0.15); color: #3b82f6;",
        "Confirmed" => "background: rgba(34,197,94,0.15); color: #22c55e;",
        "Failed" => "background: rgba(239,68,68,0.15); color: #ef4444;",
        "Replaced" => "background: rgba(148,163,184,0.15); color: #94a3b8;",
        _ => "background: rgba(148,163,184,0.15); color: #94a3b8;",
    }
}

fn speedup_cancel_disabled_reason(status: &str, busy: bool) -> &'static str {
    if busy {
        "Another transaction action is in progress."
    } else if status != "Pending" {
        "Only pending transactions can be sped up or canceled."
    } else {
        ""
    }
}

fn explorer_tx_url_for_active_network(tx_hash: &str) -> Option<String> {
    let services = shared_services();
    let net = services.network_service.active_network();
    let rt = tokio::runtime::Handle::try_current().ok()?;
    let active = rt.block_on(net)?;
    let base = active.explorer_url?;
    let trimmed = base.trim_end_matches('/');
    Some(format!("{trimmed}/tx/{tx_hash}"))
}

pub fn use_send_coroutine() -> (SendRuntime, Coroutine<SendCmd>) {
    let wallet_state: Arc<WalletState> = use_context();

    let rt = SendRuntime {
        last_fee: use_signal(|| None),
        last_error: use_signal(|| None),
        last_tx: use_signal(|| None),
        pending_txs: use_signal(Vec::new),
        busy: use_signal(|| false),
    };

    let mut rt2 = rt.clone();
    let txs = Arc::new(TransactionService::new());

    let co = use_coroutine(move |mut rx: UnboundedReceiver<SendCmd>| {
        let wallet_state = wallet_state.clone();
        let txs = txs.clone();
        async move {
            while let Some(cmd) = rx.next().await {
                rt2.busy.set(true);
                let result = match cmd {
                    SendCmd::Estimate {
                        to,
                        amount_input,
                        data,
                        max_fee_per_gas,
                        max_priority_fee_per_gas,
                        nonce,
                    } => {
                        let wallet_state = wallet_state.clone();
                        let txs = txs.clone();
                        retry_async_transient(
                            move || {
                                let to = to.clone();
                                let amount_input = amount_input.clone();
                                let data = data.clone();
                                let max_fee_per_gas = max_fee_per_gas.clone();
                                let max_priority_fee_per_gas = max_priority_fee_per_gas.clone();
                                let nonce = nonce.clone();
                                let wallet_state = wallet_state.clone();
                                let txs = txs.clone();
                                async move {
                                    let active_account = wallet_state
                                        .active_account()
                                        .await
                                        .ok_or_else(|| {
                                            WalletError::AccountNotFound("No active account".into())
                                        })?;
                                    let chain_id_u64 = active_evm_chain_id().await;
                                    let decimals = get_network_by_chain_id(chain_id_u64)
                                        .map(|n| n.decimals)
                                        .unwrap_or(18);
                                    let value_wei = parse_native_amount_to_wei(&amount_input, decimals)?;

                                    if active_account.account_type == AccountType::SmartAccount {
                                        let adapter = build_signed_adapter(chain_id_u64).await?;
                                        let (prepared_tx, _) = prepare_scw_transaction(
                                            &shared_services(),
                                            &adapter,
                                            to.clone(),
                                            value_wei,
                                            data.clone(),
                                            chain_id_u64,
                                        ).await?;
                                        wallet_state.estimate_fee(&prepared_tx).await
                                    } else {
                                        let from_addr = format!("{:?}", active_account.address);
                                        let (to, value_wei, data) =
                                            validate_and_normalize_send_inputs(
                                                &from_addr,
                                                to,
                                                value_wei,
                                                data,
                                            )?;
                                        let max_fee_per_gas =
                                            parse_optional_u256_decimal(max_fee_per_gas, "max_fee_per_gas")?;
                                        let max_priority_fee_per_gas = parse_optional_u256_decimal(
                                            max_priority_fee_per_gas,
                                            "max_priority_fee_per_gas",
                                        )?;
                                        let nonce = parse_optional_u64_decimal(nonce.as_deref())?;

                                        let mut built = txs.build_evm_transaction(
                                            vaughan_core::core::transaction::TransactionIntent {
                                                from: from_addr,
                                                to,
                                                value: value_wei,
                                                data,
                                                chain_id: chain_id_u64,
                                            },
                                        )?;
                                        let ChainTransaction::Evm(ref mut tx) = built.tx;
                                        tx.max_fee_per_gas = max_fee_per_gas;
                                        tx.max_priority_fee_per_gas = max_priority_fee_per_gas;
                                        tx.nonce = nonce;

                                        wallet_state.estimate_fee(&built.tx).await
                                    }
                                }
                            },
                            4,
                            Duration::from_millis(350),
                        )
                        .await
                        .map(|fee| rt2.last_fee.set(Some(fee)))
                    }
                    SendCmd::Send {
                        to,
                        amount_input,
                        data,
                        max_fee_per_gas,
                        max_priority_fee_per_gas,
                        nonce,
                    } => {
                        let wallet_state = wallet_state.clone();
                        let txs = txs.clone();
                        retry_async_transient(
                            move || {
                                let to = to.clone();
                                let amount_input = amount_input.clone();
                                let data = data.clone();
                                let max_fee_per_gas = max_fee_per_gas.clone();
                                let max_priority_fee_per_gas = max_priority_fee_per_gas.clone();
                                let nonce = nonce.clone();
                                let wallet_state = wallet_state.clone();
                                let txs = txs.clone();
                                async move {
                                    let active_account = wallet_state
                                        .active_account()
                                        .await
                                        .ok_or_else(|| {
                                            WalletError::AccountNotFound("No active account".into())
                                        })?;
                                    let chain_id_u64 = active_evm_chain_id().await;
                                    let adapter = build_signed_adapter(chain_id_u64).await?;

                                    if active_account.account_type == AccountType::SmartAccount {
                                        let decimals = get_network_by_chain_id(chain_id_u64)
                                            .map(|n| n.decimals)
                                            .unwrap_or(18);
                                        let value_wei = parse_native_amount_to_wei(&amount_input, decimals)?;
                                        let (prepared_tx, to_deploy_id) = prepare_scw_transaction(
                                            &shared_services(),
                                            &adapter,
                                            to.clone(),
                                            value_wei.clone(),
                                            data.clone(),
                                            chain_id_u64,
                                        ).await?;

                                        let owner_address_hex = format!("{:?}", active_account.smart_account.as_ref().unwrap().owner_address);
                                        let nonce = adapter.get_nonce(&owner_address_hex).await?;
                                        let hash = adapter.send_transaction(prepared_tx).await?;

                                        if let Some(id) = to_deploy_id {
                                            let _ = shared_services().account_manager.mark_smart_account_deployed(id).await;
                                        }

                                        Ok((hash.0, nonce, to, value_wei, data, None, None))
                                    } else {
                                        let from_addr = address_to_hex(active_account.address);
                                        let decimals = get_network_by_chain_id(chain_id_u64)
                                            .map(|n| n.decimals)
                                            .unwrap_or(18);
                                        let value_wei = parse_native_amount_to_wei(&amount_input, decimals)?;
                                        let (to, value_wei, data) = validate_and_normalize_send_inputs(
                                            &from_addr,
                                            to,
                                            value_wei,
                                            data,
                                        )?;
                                        let user_max_fee = parse_optional_u256_decimal(
                                            max_fee_per_gas,
                                            "max_fee_per_gas",
                                        )?;
                                        let user_priority_fee = parse_optional_u256_decimal(
                                            max_priority_fee_per_gas,
                                            "max_priority_fee_per_gas",
                                        )?;
                                        let user_nonce = parse_optional_u64_decimal(nonce.as_deref())?;
                                        let mut built = txs.build_evm_transaction(
                                            vaughan_core::core::transaction::TransactionIntent {
                                                from: from_addr.clone(),
                                                to: to.clone(),
                                                value: value_wei.clone(),
                                                data: data.clone(),
                                                chain_id: chain_id_u64,
                                            },
                                        )?;
                                        let fee = wallet_state.estimate_fee(&built.tx).await?;
                                        let nonce = adapter.get_nonce(&from_addr).await?;
                                        let ChainTransaction::Evm(ref mut tx) = built.tx;
                                        tx.chain_id = chain_id_u64;
                                        tx.gas_limit = Some(fee.gas_limit);
                                        tx.max_fee_per_gas = user_max_fee.or(fee.max_fee_per_gas.clone());
                                        tx.max_priority_fee_per_gas =
                                            user_priority_fee.or(fee.max_priority_fee_per_gas.clone());
                                        let final_nonce = user_nonce.unwrap_or(nonce);
                                        tx.nonce = Some(final_nonce);
                                        let final_max_fee = tx.max_fee_per_gas.clone();
                                        let final_priority_fee = tx.max_priority_fee_per_gas.clone();
                                        let hash = adapter.send_transaction(built.tx).await?;
                                        Ok((hash.0, final_nonce, to, value_wei, data, final_max_fee, final_priority_fee))
                                    }
                                }
                            },
                            4,
                            Duration::from_millis(400),
                        )
                        .await
                        .map(
                            |(tx_hash, nonce, to, value_wei, data, max_fee_per_gas, max_priority_fee_per_gas)| {
                                rt2.last_tx.set(Some(tx_hash.clone()));
                                let mut list = rt2.pending_txs.read().clone();
                                upsert_pending(&mut list, PendingTxContext {
                                    tx_hash,
                                    nonce,
                                    to,
                                    value_wei,
                                    data,
                                    max_fee_per_gas,
                                    max_priority_fee_per_gas,
                                    status: "Pending".into(),
                                });
                                rt2.pending_txs.set(list);
                            },
                        )
                    }
                    SendCmd::RefreshStatus { tx_hash } => {
                        let tx_hash_for_update = tx_hash.clone();
                        {
                            let mut list = rt2.pending_txs.read().clone();
                            set_pending_status(&mut list, &tx_hash_for_update, "Refreshing");
                            rt2.pending_txs.set(list);
                        }
                        retry_async_transient(
                            move || {
                                let tx_hash = tx_hash.clone();
                                async move {
                                    let chain_id_u64 = active_evm_chain_id().await;
                                    let adapter = build_signed_adapter(chain_id_u64).await?;
                                    let status = adapter.get_tx_status(&tx_hash).await?;
                                    Ok(status)
                                }
                            },
                            2,
                            Duration::from_millis(250),
                        )
                        .await
                        .map(|status| {
                            let status_txt = tx_status_label(status).to_string();
                            let mut list = rt2.pending_txs.read().clone();
                            set_pending_status(&mut list, &tx_hash_for_update, &status_txt);
                            rt2.pending_txs.set(list);
                        })
                    }
                    SendCmd::ReplacePending { mode, tx_hash } => {
                        let pending_snapshot = rt2
                            .pending_txs
                            .read()
                            .iter()
                            .find(|p| p.tx_hash == tx_hash)
                            .cloned();
                        if let Some(pending) = pending_snapshot {
                            let pending_nonce = pending.nonce;
                            let old_hash = pending.tx_hash.clone();
                            let wallet_state = wallet_state.clone();
                            let txs = txs.clone();
                            retry_async_transient(
                            move || {
                                let wallet_state = wallet_state.clone();
                                let txs = txs.clone();
                                let pending = pending.clone();
                                async move {
                                    let from = wallet_state
                                        .active_account()
                                        .await
                                        .ok_or_else(|| {
                                            WalletError::AccountNotFound("No active account".into())
                                        })?;
                                    let from_addr = address_to_hex(from.address);
                                    let chain_id_u64 = active_evm_chain_id().await;
                                    let (to, value_wei, data) = match mode {
                                        ReplaceMode::SpeedUp => (
                                            pending.to.clone(),
                                            pending.value_wei.clone(),
                                            pending.data.clone(),
                                        ),
                                        ReplaceMode::Cancel => (from_addr.clone(), "0".into(), None),
                                    };
                                    let mut built = txs.build_evm_transaction(
                                        vaughan_core::core::transaction::TransactionIntent {
                                            from: from_addr.clone(),
                                            to: to.clone(),
                                            value: value_wei.clone(),
                                            data: data.clone(),
                                            chain_id: chain_id_u64,
                                        },
                                    )?;
                                    let fee = wallet_state.estimate_fee(&built.tx).await?;
                                    let adapter = build_signed_adapter(chain_id_u64).await?;
                                    let ChainTransaction::Evm(ref mut tx) = built.tx;
                                    tx.chain_id = chain_id_u64;
                                    tx.gas_limit = Some(fee.gas_limit);
                                    tx.nonce = Some(pending.nonce);
                                    let base_max_fee = pending
                                        .max_fee_per_gas
                                        .clone()
                                        .or(fee.max_fee_per_gas.clone())
                                        .ok_or_else(|| WalletError::InvalidTransaction("Missing max_fee_per_gas".into()))?;
                                    let base_priority_fee = pending
                                        .max_priority_fee_per_gas
                                        .clone()
                                        .or(fee.max_priority_fee_per_gas.clone())
                                        .ok_or_else(|| WalletError::InvalidTransaction("Missing max_priority_fee_per_gas".into()))?;
                                    tx.max_fee_per_gas = Some(bump_fee_12_percent_plus_one(&base_max_fee)?);
                                    tx.max_priority_fee_per_gas =
                                        Some(bump_fee_12_percent_plus_one(&base_priority_fee)?);
                                    let next_max_fee = tx.max_fee_per_gas.clone();
                                    let next_priority_fee = tx.max_priority_fee_per_gas.clone();
                                    let hash = adapter.send_transaction(built.tx).await?;
                                    Ok((hash.0, to, value_wei, data, next_max_fee, next_priority_fee))
                                }
                            },
                                3,
                                Duration::from_millis(300),
                            )
                            .await
                            .map(
                                |(tx_hash, to, value_wei, data, max_fee_per_gas, max_priority_fee_per_gas)| {
                                    rt2.last_tx.set(Some(tx_hash.clone()));
                                    let mut list = rt2.pending_txs.read().clone();
                                    set_pending_status(&mut list, &old_hash, "Replaced");
                                    upsert_pending(&mut list, PendingTxContext {
                                        tx_hash,
                                        nonce: pending_nonce,
                                        to,
                                        value_wei,
                                        data,
                                        max_fee_per_gas,
                                        max_priority_fee_per_gas,
                                        status: "Pending".into(),
                                    });
                                    rt2.pending_txs.set(list);
                                },
                            )
                        } else {
                            Err(WalletError::InvalidTransaction(
                                "No pending transaction to replace".into(),
                            ))
                        }
                    }
                };

                if let Err(e) = result {
                    rt2.last_error.set(Some(map_send_error_like_metamask(&e)));
                }
                rt2.busy.set(false);
            }
        }
    });

    (rt, co)
}
