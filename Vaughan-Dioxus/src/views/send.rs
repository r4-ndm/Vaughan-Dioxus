use dioxus::prelude::*;

use std::sync::Arc;
use futures_util::StreamExt;

use vaughan_core::chains::Fee;
use vaughan_core::chains::evm::utils::parse_address;
use vaughan_core::core::TransactionService;
use vaughan_core::core::wallet::WalletState;
use vaughan_core::error::WalletError;

#[derive(Debug, Clone)]
pub enum SendCmd {
    Estimate { to: String, value_wei: String, data: Option<String> },
    Send { to: String, value_wei: String, data: Option<String> },
}

#[derive(Clone)]
pub struct SendRuntime {
    pub last_fee: Signal<Option<Fee>>,
    pub last_error: Signal<Option<String>>,
    pub last_tx: Signal<Option<String>>,
    pub busy: Signal<bool>,
}

#[component]
pub fn SendView(cmd_tx: Coroutine<SendCmd>) -> Element {
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
            let prio = f.max_priority_fee_per_gas.clone().unwrap_or_else(|| "—".into());
            format!("gas_limit={} max_fee_per_gas={} max_priority_fee_per_gas={}", f.gas_limit, max, prio)
        })
        .unwrap_or_else(|| "—".into());

    rsx! {
        div { style: "display: flex; flex-direction: column; gap: 12px;",
            h2 { "Send" }

            div { style: "border: 1px solid var(--border); background: var(--card); padding: 14px;",
                p { class: "muted", style: "margin: 0; font-size: 12px;", "Recipient (0x...)" }
                input {
                    value: "{to.read()}",
                    oninput: move |e| *to.write() = e.value(),
                    style: "width: 100%; margin-top: 8px; padding: 10px 12px; background: var(--bg); border: 1px solid var(--border); color: var(--fg); font-family: var(--font-mono); font-size: 12px;",
                    placeholder: "0x…"
                }
            }

            div { style: "border: 1px solid var(--border); background: var(--card); padding: 14px;",
                p { class: "muted", style: "margin: 0; font-size: 12px;", "Amount (wei, decimal)" }
                input {
                    value: "{value_wei.read()}",
                    oninput: move |e| *value_wei.write() = e.value(),
                    style: "width: 100%; margin-top: 8px; padding: 10px 12px; background: var(--bg); border: 1px solid var(--border); color: var(--fg); font-family: var(--font-mono); font-size: 12px;",
                    placeholder: "1000000000000000000"
                }
                p { class: "muted", style: "margin-top: 8px; font-size: 12px;", "For now this expects wei; we’ll add ETH formatting later." }
            }

            div { style: "border: 1px solid var(--border); background: var(--card); padding: 14px;",
                p { class: "muted", style: "margin: 0; font-size: 12px;", "Data (optional hex)" }
                input {
                    value: "{data.read()}",
                    oninput: move |e| *data.write() = e.value(),
                    style: "width: 100%; margin-top: 8px; padding: 10px 12px; background: var(--bg); border: 1px solid var(--border); color: var(--fg); font-family: var(--font-mono); font-size: 12px;",
                    placeholder: "0x"
                }
            }

            div { style: "border: 1px solid var(--border); background: var(--card); padding: 14px;",
                p { class: "muted", style: "margin: 0; font-size: 12px;", "Estimated fee (EIP-1559 fields in wei)" }
                p { style: "margin-top: 8px; font-family: var(--font-mono); font-size: 12px;", "{fee_text}" }
            }

            if let Some(err) = rt.last_error.read().as_ref() {
                div { style: "border: 1px solid #442; background: #110; padding: 12px;",
                    p { style: "margin: 0; color: #f5b;", "{err}" }
                }
            }

            if let Some(tx) = rt.last_tx.read().as_ref() {
                div { style: "border: 1px solid #224; background: #081018; padding: 12px;",
                    p { class: "muted", style: "margin: 0 0 6px 0; font-size: 12px;", "Last tx hash" }
                    p { style: "margin: 0; font-family: var(--font-mono); font-size: 12px; word-break: break-all;", "{tx}" }
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
                    style: "position: fixed; inset: 0; background: rgba(0,0,0,0.7); display: flex; align-items: center; justify-content: center; padding: 16px;",
                    onclick: move |_| *confirm_open.write() = false,
                    div {
                        style: "width: 100%; max-width: 520px; background: var(--card-2); border: 1px solid var(--border); padding: 16px;",
                        onclick: move |evt| evt.stop_propagation(),
                        h3 { "Confirm send" }
                        p { class: "muted", style: "font-size: 12px;", "This will fail until we wire a real signer (import/private key). Fee estimation works." }
                        p { style: "font-family: var(--font-mono); font-size: 12px;", "to={to.read()} value_wei={value_wei.read()}" }
                        p { style: "font-family: var(--font-mono); font-size: 12px;", "data={data.read()}" }
                        p { style: "font-family: var(--font-mono); font-size: 12px;", "{fee_text}" }
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
                let result: Result<(), WalletError> = (|| async {
                    match cmd {
                    SendCmd::Estimate { to, value_wei, data } => {
                        parse_address(&to)?; // validate now for clear error
                        let from = wallet_state
                            .active_account()
                            .await
                            .ok_or_else(|| WalletError::AccountNotFound("No active account".into()))?
                            .address;

                        let chain_id = wallet_state
                            .active_chain()
                            .await;
                        let chain_id_u64 = match chain_id {
                            vaughan_core::chains::ChainType::Evm => 1, // for now; will be wired from NetworkService
                            _ => 1,
                        };

                        let built = txs.build_evm_transaction(vaughan_core::core::transaction::TransactionIntent {
                            from: format!("{:?}", from),
                            to,
                            value: value_wei,
                            data,
                            chain_id: chain_id_u64,
                        })?;

                        let fee = wallet_state.estimate_fee(&built.tx).await?;
                        rt2.last_fee.set(Some(fee));
                        Ok(())
                    }
                    SendCmd::Send { to, value_wei, data } => {
                        parse_address(&to)?;
                        let from = wallet_state
                            .active_account()
                            .await
                            .ok_or_else(|| WalletError::AccountNotFound("No active account".into()))?
                            .address;

                        let built = txs.build_evm_transaction(vaughan_core::core::transaction::TransactionIntent {
                            from: format!("{:?}", from),
                            to,
                            value: value_wei,
                            data,
                            chain_id: 1,
                        })?;

                        // Try to broadcast via adapter (will error until signer is configured).
                        let hash = wallet_state.send_transaction(built.tx).await?;
                        rt2.last_tx.set(Some(hash.0));
                        Ok(())
                    }
                    }
                })().await;

                if let Err(e) = result {
                    rt2.last_error.set(Some(e.to_string()));
                }
                rt2.busy.set(false);
            }
        }
    });

    (rt, co)
}

