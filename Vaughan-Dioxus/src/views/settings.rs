use dioxus::prelude::*;

use std::sync::Arc;

use futures_util::StreamExt;

use vaughan_core::core::network::NetworkHealth;
use vaughan_core::core::{NetworkConfig, WalletState};
use vaughan_core::error::retry_async_transient;

use crate::components::SubpageToolbar;
use crate::services::AppServices;

#[derive(Debug, Clone)]
pub enum SettingsCmd {
    RefreshNetworks,
    SetActive(String),
    CheckHealth(String),
    ToggleSound(bool),
    RefreshTokens,
    AddErc20 {
        contract: String,
        symbol: String,
        name: String,
        decimals: u8,
    },
    RemoveToken(String),
}

#[derive(Clone)]
pub struct SettingsRuntime {
    pub networks: Signal<Vec<NetworkConfig>>,
    pub active_network: Signal<Option<String>>,
    pub health: Signal<Option<NetworkHealth>>,
    pub loading: Signal<bool>,
    pub error: Signal<Option<String>>,
    pub sound_enabled: Signal<bool>,
    pub tokens: Signal<Vec<vaughan_core::chains::TokenInfo>>,
    pub token_contract: Signal<String>,
    pub token_symbol: Signal<String>,
    pub token_name: Signal<String>,
    pub token_decimals: Signal<String>,
}

pub fn provide_settings_runtime() -> SettingsRuntime {
    SettingsRuntime {
        networks: use_signal(|| Vec::new()),
        active_network: use_signal(|| None),
        health: use_signal(|| None),
        loading: use_signal(|| false),
        error: use_signal(|| None),
        sound_enabled: use_signal(|| false),
        tokens: use_signal(|| Vec::new()),
        token_contract: use_signal(|| "".into()),
        token_symbol: use_signal(|| "".into()),
        token_name: use_signal(|| "".into()),
        token_decimals: use_signal(|| "18".into()),
    }
}

#[component]
pub fn SettingsView(
    cmd_tx: Coroutine<SettingsCmd>,
    on_back: Callback<()>,
    on_wallet_deleted: Callback<()>,
) -> Element {
    let mut rt: SettingsRuntime = use_context();
    let services: AppServices = use_context();
    let wallet_state: Arc<WalletState> = use_context();
    let locked = use_signal(|| true);

    let wipe_confirm_open = use_signal(|| false);
    let wipe_busy = use_signal(|| false);
    let wipe_err = use_signal(|| None::<String>);

    use_effect(move || {
        let ws = wallet_state.clone();
        let mut locked = locked.clone();
        spawn(async move {
            locked.set(ws.is_locked().await);
        });
    });

    use_effect(move || {
        cmd_tx.send(SettingsCmd::RefreshNetworks);
        cmd_tx.send(SettingsCmd::RefreshTokens);
    });

    rsx! {
        div { style: "display: flex; flex-direction: column; gap: 16px;",
            SubpageToolbar { title: "Settings", on_back: on_back.clone() }

            if *rt.loading.read() {
                p { class: "muted", "Loading…" }
            }
            if let Some(err) = rt.error.read().as_ref() {
                div { style: "border: 1px solid rgba(239,68,68,0.3); background: var(--error-bg); padding: 12px; border-radius: 8px;",
                    p { style: "margin: 0; color: var(--error-text); font-size: 13px;", "{err}" }
                }
            }

            // ---- Networks ----
            div { class: "card-panel rounded-lg",
                h3 { class: "settings-section-title", "🌐 Networks" }
                p { class: "muted", style: "margin: 0 0 10px 0; font-size: 12px;",
                    "Built-in networks from core. (Custom add/edit comes later.)"
                }

                for net in rt.networks.read().iter() {
                    div { key: "{net.id}", style: "border: 1px solid var(--border); background: var(--bg); padding: 10px; margin-bottom: 8px;",
                        div { style: "display: flex; justify-content: space-between; gap: 8px; align-items: baseline;",
                            div {
                                p { style: "margin: 0; font-weight: 700;", "{net.name}" }
                                p { class: "muted", style: "margin: 4px 0 0 0; font-size: 12px; font-family: var(--font-mono);",
                                    "{net.id}  chain_id={net.chain_id}"
                                }
                            }
                            div { style: "display: flex; gap: 8px;",
                                button {
                                    class: "btn",
                                    onclick: {
                                        let id = net.id.clone();
                                        move |_| cmd_tx.send(SettingsCmd::SetActive(id.clone()))
                                    },
                                    if rt.active_network.read().as_deref() == Some(net.id.as_str()) { "Active" } else { "Use" }
                                }
                                button {
                                    class: "btn",
                                    onclick: {
                                        let id = net.id.clone();
                                        move |_| cmd_tx.send(SettingsCmd::CheckHealth(id.clone()))
                                    },
                                    "Health"
                                }
                            }
                        }
                        p { class: "muted", style: "margin: 8px 0 0 0; font-size: 12px; font-family: var(--font-mono); word-break: break-all;",
                            "{net.rpc_url}"
                        }
                    }
                }

                if let Some(h) = rt.health.read().as_ref() {
                    div { style: "margin-top: 8px; border: 1px solid var(--border); background: var(--card-2); padding: 12px;",
                        p { style: "margin: 0; font-weight: 700;",
                            if h.ok { "RPC OK" } else { "RPC ERROR" }
                        }
                        p { class: "muted", style: "margin: 6px 0 0 0; font-size: 12px; font-family: var(--font-mono);",
                            "latency_ms={h.latency_ms} latest_block={h.latest_block.unwrap_or(0)}"
                        }
                        if let Some(e) = h.error.as_ref() {
                            p { style: "margin: 6px 0 0 0; color: #f5b; font-size: 12px; font-family: var(--font-mono);",
                                "{e}"
                            }
                        }
                    }
                }
            }

            // ---- Preferences ----
            div { class: "card-panel rounded-lg",
                h3 { class: "settings-section-title", "Preferences" }
                label { style: "display: flex; align-items: center; gap: 10px; font-size: 12px;",
                    input {
                        r#type: "checkbox",
                        checked: *rt.sound_enabled.read(),
                        onchange: move |e| cmd_tx.send(SettingsCmd::ToggleSound(e.value() == "true")),
                    }
                    span { class: "muted", "Sound notifications (feature-gated later)" }
                }
            }

            // ---- Tokens ----
            div { class: "card-panel rounded-lg",
                h3 { class: "settings-section-title", "Tokens" }
                p { class: "muted", style: "margin: 0 0 10px 0; font-size: 12px;",
                    "Tracked ERC-20 tokens for the active chain (in-memory for now)."
                }

                div { style: "display: flex; flex-direction: column; gap: 8px;",
                    input {
                        value: "{rt.token_contract.read()}",
                        oninput: move |e| *rt.token_contract.write() = e.value(),
                        style: "width: 100%; padding: 10px 12px; background: var(--bg); border: 1px solid var(--border); color: var(--fg); font-family: var(--font-mono); font-size: 12px;",
                        placeholder: "Token contract (0x...)"
                    }
                    input {
                        value: "{rt.token_symbol.read()}",
                        oninput: move |e| *rt.token_symbol.write() = e.value(),
                        style: "width: 100%; padding: 10px 12px; background: var(--bg); border: 1px solid var(--border); color: var(--fg); font-size: 12px;",
                        placeholder: "Symbol (e.g. USDC)"
                    }
                    input {
                        value: "{rt.token_name.read()}",
                        oninput: move |e| *rt.token_name.write() = e.value(),
                        style: "width: 100%; padding: 10px 12px; background: var(--bg); border: 1px solid var(--border); color: var(--fg); font-size: 12px;",
                        placeholder: "Name (e.g. USD Coin)"
                    }
                    input {
                        value: "{rt.token_decimals.read()}",
                        oninput: move |e| *rt.token_decimals.write() = e.value(),
                        style: "width: 100%; padding: 10px 12px; background: var(--bg); border: 1px solid var(--border); color: var(--fg); font-family: var(--font-mono); font-size: 12px;",
                        placeholder: "Decimals (usually 18)"
                    }

                    div { class: "btn-row",
                        button {
                            class: "btn",
                            onclick: move |_| {
                                let decimals = rt.token_decimals.read().trim().parse::<u8>().unwrap_or(18);
                                cmd_tx.send(SettingsCmd::AddErc20{
                                    contract: rt.token_contract.read().trim().to_string(),
                                    symbol: rt.token_symbol.read().trim().to_string(),
                                    name: rt.token_name.read().trim().to_string(),
                                    decimals,
                                });
                            },
                            "Add token"
                        }
                        button { class: "btn", onclick: move |_| cmd_tx.send(SettingsCmd::RefreshTokens), "Refresh list" }
                    }
                }

                div { style: "display: flex; flex-direction: column; gap: 8px; margin-top: 12px;",
                    for tok in rt.tokens.read().iter() {
                        div { key: "{tok.contract_address.as_deref().unwrap_or(\"\")}", style: "border: 1px solid var(--border); background: var(--bg); padding: 10px;",
                            div { style: "display: flex; justify-content: space-between; align-items: baseline; gap: 8px;",
                                div {
                                    p { style: "margin: 0; font-weight: 700;", "{tok.symbol} ({tok.decimals})" }
                                    p { class: "muted", style: "margin: 4px 0 0 0; font-size: 12px;", "{tok.name}" }
                                    p { class: "muted", style: "margin: 6px 0 0 0; font-family: var(--font-mono); font-size: 12px; word-break: break-all;",
                                        "{tok.contract_address.as_deref().unwrap_or(\"-\")}"
                                    }
                                }
                                button {
                                    class: "btn",
                                    onclick: {
                                        let addr = tok.contract_address.clone().unwrap_or_default();
                                        move |_| cmd_tx.send(SettingsCmd::RemoveToken(addr.clone()))
                                    },
                                    "Remove"
                                }
                            }
                        }
                    }
                }
            }

            // ---- Security / Wallet ----
            div { class: "card-panel rounded-lg",
                h3 { class: "settings-section-title", "Security" }
                p { class: "muted", style: "margin: 0 0 10px 0; font-size: 12px;",
                    "Signing/broadcast requires importing a private key or deriving an HD account (not wired in UI yet)."
                }
                p { style: "margin: 0; font-family: var(--font-mono); font-size: 12px;",
                    "wallet_locked={locked.read()}"
                }
            }

            if let Some(werr) = wipe_err.read().as_ref() {
                div { style: "border: 1px solid rgba(239,68,68,0.3); background: var(--error-bg); padding: 12px; border-radius: 8px;",
                    p { style: "margin: 0; color: var(--error-text); font-size: 13px;", "{werr}" }
                }
            }

            div { class: "card-panel rounded-lg danger-zone",
                h3 { class: "settings-section-title", style: "color: var(--error-text);",
                    "Danger zone"
                }
                p { class: "muted", style: "margin: 0 0 12px 0; font-size: 12px;",
                    "Delete wallet removes the master seed, imported keys, and in-memory accounts from this device. "
                    "You cannot undo this. Have your recovery phrase backed up before continuing."
                }
                if *wipe_confirm_open.read() {
                    p { style: "margin: 0 0 12px 0; font-size: 13px; color: var(--foreground);",
                        "Delete all wallet data on this device and return to setup?"
                    }
                    div { class: "btn-row",
                        button {
                            class: "vaughan-btn",
                            disabled: *wipe_busy.read(),
                            onclick: {
                                let mut wipe_confirm_open = wipe_confirm_open.clone();
                                let mut wipe_err = wipe_err.clone();
                                move |_| {
                                    wipe_confirm_open.set(false);
                                    wipe_err.set(None);
                                }
                            },
                            "Cancel"
                        }
                        button {
                            class: "btn-danger-outline",
                            disabled: *wipe_busy.read(),
                            onclick: {
                                let services = services.clone();
                                let on_wallet_deleted = on_wallet_deleted.clone();
                                let mut wipe_busy = wipe_busy.clone();
                                let mut wipe_err = wipe_err.clone();
                                let wipe_confirm_open = wipe_confirm_open.clone();
                                move |_| {
                                    wipe_err.set(None);
                                    wipe_busy.set(true);
                                    let s = services.clone();
                                    let od = on_wallet_deleted.clone();
                                    let mut wipe_busy = wipe_busy.clone();
                                    let mut wipe_err = wipe_err.clone();
                                    let mut wipe_confirm_open = wipe_confirm_open.clone();
                                    spawn(async move {
                                        match s.wipe_wallet().await {
                                            Ok(()) => od.call(()),
                                            Err(e) => {
                                                wipe_err.set(Some(e.user_message()));
                                                wipe_busy.set(false);
                                                wipe_confirm_open.set(false);
                                            }
                                        }
                                    });
                                }
                            },
                            if *wipe_busy.read() { "Deleting…" } else { "Delete wallet" }
                        }
                    }
                } else {
                    button {
                        class: "btn-danger-outline",
                        onclick: {
                            let mut wipe_confirm_open = wipe_confirm_open.clone();
                            let mut wipe_err = wipe_err.clone();
                            move |_| {
                                wipe_err.set(None);
                                wipe_confirm_open.set(true);
                            }
                        },
                        "Delete wallet from this device"
                    }
                }
            }
        }
    }
}

pub fn use_settings_coroutine() -> Coroutine<SettingsCmd> {
    let services: AppServices = use_context();
    let rt: SettingsRuntime = use_context();

    use_coroutine(move |mut rx: UnboundedReceiver<SettingsCmd>| {
        let services = services.clone();
        let mut rt2 = rt.clone();

        async move {
            while let Some(cmd) = rx.next().await {
                match cmd {
                    SettingsCmd::RefreshNetworks => {
                        rt2.loading.set(true);
                        rt2.error.set(None);

                        let nets = services.network_service.list_networks().await;
                        rt2.networks.set(nets);
                        rt2.active_network.set(
                            services
                                .network_service
                                .active_network()
                                .await
                                .map(|n| n.id),
                        );

                        rt2.loading.set(false);
                    }
                    SettingsCmd::RefreshTokens => {
                        rt2.error.set(None);
                        // Determine active chain_id (default to Ethereum mainnet).
                        let chain_id = services
                            .network_service
                            .active_network()
                            .await
                            .map(|n| n.chain_id)
                            .unwrap_or(1);
                        let toks = services.token_manager.list(chain_id).await;
                        rt2.tokens.set(toks);
                    }
                    SettingsCmd::AddErc20 {
                        contract,
                        symbol,
                        name,
                        decimals,
                    } => {
                        rt2.error.set(None);
                        let chain_id = services
                            .network_service
                            .active_network()
                            .await
                            .map(|n| n.chain_id)
                            .unwrap_or(1);
                        if let Err(e) = services
                            .token_manager
                            .add_erc20(chain_id, &contract, &symbol, &name, decimals)
                            .await
                        {
                            rt2.error.set(Some(e.user_message()));
                        } else {
                            let toks = services.token_manager.list(chain_id).await;
                            rt2.tokens.set(toks);
                        }
                    }
                    SettingsCmd::RemoveToken(addr) => {
                        rt2.error.set(None);
                        let chain_id = services
                            .network_service
                            .active_network()
                            .await
                            .map(|n| n.chain_id)
                            .unwrap_or(1);
                        let _ = services.token_manager.remove(chain_id, &addr).await;
                        let toks = services.token_manager.list(chain_id).await;
                        rt2.tokens.set(toks);
                    }
                    SettingsCmd::SetActive(id) => {
                        rt2.error.set(None);
                        if let Err(e) = services.network_service.set_active_network(&id).await {
                            rt2.error.set(Some(e.user_message()));
                        } else {
                            rt2.active_network.set(Some(id));
                            // Refresh tokens for newly active network.
                            let chain_id = services
                                .network_service
                                .active_network()
                                .await
                                .map(|n| n.chain_id)
                                .unwrap_or(1);
                            let toks = services.token_manager.list(chain_id).await;
                            rt2.tokens.set(toks);
                        }
                    }
                    SettingsCmd::CheckHealth(id) => {
                        rt2.error.set(None);
                        match retry_async_transient(
                            || {
                                let id = id.clone();
                                let service = services.network_service.clone();
                                async move { service.check_health(&id).await }
                            },
                            4,
                            std::time::Duration::from_millis(250),
                        )
                        .await
                        {
                            Ok(h) => rt2.health.set(Some(h)),
                            Err(e) => rt2.error.set(Some(e.user_message())),
                        }
                    }
                    SettingsCmd::ToggleSound(v) => {
                        rt2.sound_enabled.set(v);
                    }
                }
            }
        }
    })
}
