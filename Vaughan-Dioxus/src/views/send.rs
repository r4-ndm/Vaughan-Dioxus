use dioxus::prelude::*;

use futures_util::StreamExt;
use std::sync::Arc;
use std::time::Duration;

use vaughan_core::chains::evm::utils::parse_address;
use vaughan_core::chains::Fee;
use vaughan_core::core::wallet::WalletState;
use vaughan_core::core::TransactionService;
use vaughan_core::error::{retry_async_transient, WalletError};

use crate::components::SubpageToolbar;

#[derive(Debug, Clone)]
pub enum SendCmd {
    Estimate {
        to: String,
        value_wei: String,
        data: Option<String>,
    },
    Send {
        to: String,
        value_wei: String,
        data: Option<String>,
    },
}

#[derive(Clone)]
pub struct SendRuntime {
    pub last_fee: Signal<Option<Fee>>,
    pub last_error: Signal<Option<String>>,
    pub last_tx: Signal<Option<String>>,
    pub busy: Signal<bool>,
}

#[component]
pub fn SendView(cmd_tx: Coroutine<SendCmd>, on_back: Callback<()>) -> Element {
    let mut rt: SendRuntime = use_context();

    let mut to = use_signal(|| "".to_string());
    let mut value_wei = use_signal(|| "".to_string());
    let mut data = use_signal(|| "".to_string());
    let mut confirm_open = use_signal(|| false);

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

    rsx! {
        div { style: "display: flex; flex-direction: column; gap: 16px;",
            SubpageToolbar { title: "Send", on_back: on_back.clone() }

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
                label { class: "field-label", "Amount (wei)" }
                input {
                    class: "input-std input-mono",
                    value: "{value_wei.read()}",
                    oninput: move |e| *value_wei.write() = e.value(),
                    placeholder: "1000000000000000000"
                }
                p { class: "muted", style: "margin: 8px 0 0 0; font-size: 11px;", "ETH formatting UI coming later." }
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

            div { class: "btn-row",
                button {
                    class: "btn",
                    disabled: *rt.busy.read(),
                    onclick: move |_| {
                        rt.last_error.set(None);
                        rt.last_tx.set(None);
                        cmd_tx.send(SendCmd::Estimate {
                            to: to.read().clone(),
                            value_wei: value_wei.read().clone(),
                            data: {
                                let d = data.read().trim().to_string();
                                if d.is_empty() { None } else { Some(d) }
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
                            "This will fail until a signer is fully wired. Fee estimation works."
                        }
                        p { style: "font-family: var(--font-mono); font-size: 11px; margin: 4px 0;", "to={to.read()} value_wei={value_wei.read()}" }
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
                                        value_wei: value_wei.read().clone(),
                                        data: {
                                            let d = data.read().trim().to_string();
                                            if d.is_empty() { None } else { Some(d) }
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

pub fn use_send_coroutine() -> (SendRuntime, Coroutine<SendCmd>) {
    let wallet_state: Arc<WalletState> = use_context();

    let rt = SendRuntime {
        last_fee: use_signal(|| None),
        last_error: use_signal(|| None),
        last_tx: use_signal(|| None),
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
                        value_wei,
                        data,
                    } => {
                        let wallet_state = wallet_state.clone();
                        let txs = txs.clone();
                        retry_async_transient(
                            move || {
                                let to = to.clone();
                                let value_wei = value_wei.clone();
                                let data = data.clone();
                                let wallet_state = wallet_state.clone();
                                let txs = txs.clone();
                                async move {
                                    parse_address(&to)?;
                                    let from = wallet_state
                                        .active_account()
                                        .await
                                        .ok_or_else(|| {
                                            WalletError::AccountNotFound("No active account".into())
                                        })?
                                        .address;

                                    let chain_id = wallet_state.active_chain().await;
                                    let chain_id_u64 = match chain_id {
                                        vaughan_core::chains::ChainType::Evm => 1,
                                        _ => 1,
                                    };

                                    let built = txs.build_evm_transaction(
                                        vaughan_core::core::transaction::TransactionIntent {
                                            from: format!("{:?}", from),
                                            to,
                                            value: value_wei,
                                            data,
                                            chain_id: chain_id_u64,
                                        },
                                    )?;

                                    wallet_state.estimate_fee(&built.tx).await
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
                        value_wei,
                        data,
                    } => {
                        let wallet_state = wallet_state.clone();
                        let txs = txs.clone();
                        retry_async_transient(
                            move || {
                                let to = to.clone();
                                let value_wei = value_wei.clone();
                                let data = data.clone();
                                let wallet_state = wallet_state.clone();
                                let txs = txs.clone();
                                async move {
                                    parse_address(&to)?;
                                    let from = wallet_state
                                        .active_account()
                                        .await
                                        .ok_or_else(|| {
                                            WalletError::AccountNotFound("No active account".into())
                                        })?
                                        .address;

                                    let built = txs.build_evm_transaction(
                                        vaughan_core::core::transaction::TransactionIntent {
                                            from: format!("{:?}", from),
                                            to,
                                            value: value_wei,
                                            data,
                                            chain_id: 1,
                                        },
                                    )?;

                                    wallet_state.send_transaction(built.tx).await
                                }
                            },
                            4,
                            Duration::from_millis(400),
                        )
                        .await
                        .map(|hash| rt2.last_tx.set(Some(hash.0)))
                    }
                };

                if let Err(e) = result {
                    rt2.last_error.set(Some(e.user_message()));
                }
                rt2.busy.set(false);
            }
        }
    });

    (rt, co)
}
