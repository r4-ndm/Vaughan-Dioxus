use dioxus::prelude::*;

use crate::browser::{self, TRUSTED_DAPP_ENTRIES};
use crate::dapp_approval::{broker, PendingSignMessage, PendingSignTransaction};
use crate::services::AppServices;

#[component]
pub fn DappsView(on_back: Callback<()>) -> Element {
    let services: AppServices = use_context();
    let no_accounts_for_dapps = use_signal(|| false);

    use_effect({
        let services = services.clone();
        let no_accounts_for_dapps = no_accounts_for_dapps.clone();
        move || {
            let mgr = services.account_manager.clone();
            let mut no_accounts_for_dapps = no_accounts_for_dapps.clone();
            spawn(async move {
                let empty = mgr.list_accounts().await.is_empty();
                no_accounts_for_dapps.set(empty);
            });
        }
    });

    let pending_message = use_signal(|| broker().pending_sign_message());
    let pending_tx = use_signal(|| broker().pending_sign_transaction());
    let dapp_open_error = use_signal(|| None::<String>);

    let mut refresh = {
        let mut pending_message = pending_message.clone();
        let mut pending_tx = pending_tx.clone();
        move || {
            pending_message.set(broker().pending_sign_message());
            pending_tx.set(broker().pending_sign_transaction());
        }
    };

    rsx! {
        div { style: "display: flex; flex-direction: column; gap: 20px;",
            div { class: "dapps-header-wrap",
                button {
                    class: "back-link dapps-back",
                    onclick: move |_| on_back.call(()),
                    "←"
                }
                h1 { class: "dapps-title", "DApps Browser" }
            }

            p { class: "muted", style: "margin: 0; font-size: 13px; text-align: center;",
                "Trusted sites open in the Vaughan browser with the wallet connected. Approve signing requests below."
            }

            if *no_accounts_for_dapps.read() {
                p {
                    class: "muted",
                    style: "margin: 0; padding: 10px 12px; font-size: 13px; text-align: center; border: 1px solid var(--border); background: var(--card); color: var(--error-text);",
                    "No accounts in this wallet yet. Use Create or Import in the dock, then connect in Uniswap — eth_requestAccounts only returns addresses from those accounts."
                }
            }

            div { class: "dapp-grid",
                for (label, url) in TRUSTED_DAPP_ENTRIES.iter() {
                    div {
                        key: "{url}",
                        class: "dapp-card",
                        onclick: {
                            let mut err_sig = dapp_open_error.clone();
                            let url = (*url).to_string();
                            move |_| {
                                match browser::open_trusted_dapp_url(&url) {
                                    Ok(()) => err_sig.set(None),
                                    Err(e) => err_sig.set(Some(e)),
                                }
                            }
                        },
                        div {
                            h3 { style: "margin: 0 0 6px 0; font-size: 15px; font-weight: 700;", "{label}" }
                            p { class: "muted", style: "margin: 0; font-size: 11px; line-height: 1.35; overflow: hidden; display: -webkit-box; -webkit-line-clamp: 2; -webkit-box-orient: vertical;",
                                "{url}"
                            }
                        }
                        div { style: "margin-top: 12px; padding-top: 10px; border-top: 1px solid var(--border); font-size: 10px; font-weight: 600; color: var(--muted-foreground);",
                            "Trusted"
                        }
                    }
                }
            }

            if let Some(msg) = dapp_open_error.read().as_ref() {
                p {
                    class: "muted",
                    style: "margin: 0; font-size: 12px; color: var(--error-text); white-space: pre-wrap; text-align: center;",
                    "{msg}"
                }
            }

            div { class: "btn-row",
                button {
                    class: "vaughan-btn",
                    onclick: move |_| refresh(),
                    "Refresh approvals"
                }
            }

            {
                match pending_tx.read().clone() {
                    Some(PendingSignTransaction { request_id, payload }) => rsx! {
                        div {
                            class: "approval-card",
                            p { strong { "Request ID: " } "{request_id}" }
                            p { strong { "Type: " } "SignTransaction" }
                            p { strong { "From: " } "{payload.from}" }
                            p { strong { "To: " } "{payload.to}" }
                            p { strong { "Value: " } "{payload.value}" }
                            p { strong { "Chain ID: " } "{payload.chain_id}" }
                            if let Some(nonce) = payload.nonce.as_ref() {
                                p { strong { "Nonce: " } "{nonce}" }
                            }
                            if let Some(gas_limit) = payload.gas_limit.as_ref() {
                                p { strong { "Gas Limit: " } "{gas_limit}" }
                            }
                            if let Some(max_fee) = payload.max_fee_per_gas.as_ref() {
                                p { strong { "Max Fee Per Gas: " } "{max_fee}" }
                            }
                            if let Some(prio_fee) = payload.max_priority_fee_per_gas.as_ref() {
                                p { strong { "Max Priority Fee: " } "{prio_fee}" }
                            } else if let Some(gas_price) = payload.gas_price.as_ref() {
                                p { strong { "Gas Price: " } "{gas_price}" }
                            }
                            if let Some(data) = payload.data.as_ref() {
                                p { strong { "Data: " } "{data}" }
                            }
                            p { class: "muted", style: "font-size: 12px;", "Warning: amount and destination will be signed if approved." }

                            div { class: "btn-row", style: "margin-top: 10px;",
                                button {
                                    class: "btn",
                                    onclick: move |_| {
                                        let _ = broker().approve_sign_transaction(request_id);
                                        refresh();
                                    },
                                    "Approve Tx"
                                }
                                button {
                                    class: "btn",
                                    onclick: move |_| {
                                        let _ = broker().reject_sign_transaction(request_id);
                                        refresh();
                                    },
                                    "Reject Tx"
                                }
                            }
                        }
                    },
                    None => rsx! {},
                }
            }

            {
                match pending_message.read().clone() {
                    Some(PendingSignMessage { request_id, payload }) => rsx! {
                        div {
                            class: "approval-card",
                            p { strong { "Request ID: " } "{request_id}" }
                            p { strong { "Type: " } "SignMessage" }
                            p { strong { "Address: " } "{payload.address}" }
                            p { strong { "Chain ID: " } "{payload.chain_id}" }
                            p { strong { "Message: " } "{payload.message}" }

                            div { class: "btn-row", style: "margin-top: 10px;",
                                button {
                                    class: "btn",
                                    onclick: move |_| {
                                        let _ = broker().approve_sign_message(request_id);
                                        refresh();
                                    },
                                    "Approve Message"
                                }
                                button {
                                    class: "btn",
                                    onclick: move |_| {
                                        let _ = broker().reject_sign_message(request_id);
                                        refresh();
                                    },
                                    "Reject Message"
                                }
                            }
                        }
                    },
                    None => rsx! {
                        if pending_tx.read().is_none() {
                            p { class: "muted", style: "margin-top: 14px;", "No pending dApp approvals." }
                        }
                    },
                }
            }
        }
    }
}
