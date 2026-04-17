use dioxus::prelude::*;

use std::sync::Arc;
use std::time::Duration;

use futures_util::StreamExt;

use vaughan_core::chains::{ChainAdapter, TokenInfo};
use vaughan_core::core::WalletState;
use vaughan_core::error::WalletError;
use vaughan_core::monitoring::BalanceEvent;
use vaughan_core::monitoring::BalanceWatcher;

use crate::app::AppRuntime;
use crate::components::{
    AccountOption, AccountSelector, AddressDisplay, NetworkOption, NetworkSelector,
};
use crate::services::AppServices;

#[derive(Debug, Clone)]
pub enum DashboardCmd {
    Start,
    RefreshOnce,
    SetActiveNetwork(String),
    SetActiveAccount(String),
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
    let services: AppServices = use_context();
    let networks = use_signal(Vec::<NetworkOption>::new);
    let active_network_id = use_signal(|| None::<String>);
    let accounts = use_signal(Vec::<AccountOption>::new);
    let active_account_address = use_signal(|| None::<String>);
    let mut tracked_tokens = use_signal(Vec::<TokenInfo>::new);
    let selectors_booted = use_signal(|| false);
    let watcher_boot_sent = use_signal(|| false);
    let mut assets_open = use_signal(|| false);
    let mut selected_asset = use_signal(|| "native".to_string());
    let mut recipient = use_signal(String::new);
    let mut send_amount = use_signal(String::new);

    use_effect({
        let cmd_tx = cmd_tx;
        let mut watcher_boot_sent = watcher_boot_sent;
        move || {
            if watcher_boot_sent() {
                return;
            }
            watcher_boot_sent.set(true);
            cmd_tx.send(DashboardCmd::Start);
        }
    });

    use_effect({
        let services = services.clone();
        let mut networks = networks;
        let mut active_network_id = active_network_id;
        let mut accounts = accounts;
        let mut active_account_address = active_account_address;
        let mut selectors_booted = selectors_booted;
        move || {
            if selectors_booted() {
                return;
            }
            selectors_booted.set(true);
            let services = services.clone();
            spawn(async move {
                loop {
                    let nets = services
                        .network_service
                        .list_networks()
                        .await
                        .into_iter()
                        .map(|n| NetworkOption {
                            id: n.id,
                            name: n.name,
                            chain_id: n.chain_id,
                        })
                        .collect::<Vec<_>>();
                    let active_net = services
                        .network_service
                        .active_network()
                        .await
                        .map(|n| n.id);
                    let accts = services
                        .account_manager
                        .list_accounts()
                        .await
                        .into_iter()
                        .map(|a| AccountOption {
                            name: a.name,
                            address: format!("{:?}", a.address),
                        })
                        .collect::<Vec<_>>();
                    let active_acct = services
                        .account_manager
                        .active_account()
                        .await
                        .map(|a| format!("{:?}", a.address));
                    let chain_id = services
                        .network_service
                        .active_network()
                        .await
                        .map(|n| n.chain_id)
                        .unwrap_or(1);
                    let toks = services.token_manager.list(chain_id).await;
                    networks.set(nets);
                    active_network_id.set(active_net);
                    accounts.set(accts);
                    active_account_address.set(active_acct);
                    tracked_tokens.set(toks);
                    tokio::time::sleep(Duration::from_secs(2)).await;
                }
            });
        }
    });

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

    let asset_row_symbol = {
        let sel = selected_asset();
        if sel == "native" {
            display_symbol.clone()
        } else {
            tracked_tokens()
                .iter()
                .find(|t| t.contract_address.as_deref() == Some(sel.as_str()))
                .map(|t| t.symbol.clone())
                .unwrap_or_else(|| "Token".into())
        }
    };
    let asset_row_balance = if selected_asset() == "native" {
        display_balance.clone()
    } else {
        "—".into()
    };

    let addr_for_display = active_account_address().unwrap_or_default();

    rsx! {
        div {
            class: "dashboard-main",
            style: "display: flex; flex-direction: column; gap: 16px;",
            onclick: move |_| cmd_tx.send(DashboardCmd::RefreshOnce),

            if !addr_for_display.is_empty() {
                div {
                    style: "padding-top: 4px;",
                    onclick: move |e| e.stop_propagation(),
                    AddressDisplay { address: addr_for_display.clone() }
                }
            }

            div {
                style: "display: flex; gap: 8px; width: 100%; padding-top: 8px;",
                onclick: move |e| e.stop_propagation(),
                div { style: "flex: 1; min-width: 0;",
                    NetworkSelector {
                        networks: networks(),
                        active_id: active_network_id(),
                        on_select: move |id| cmd_tx.send(DashboardCmd::SetActiveNetwork(id)),
                    }
                }
                div { style: "flex: 1; min-width: 0;",
                    AccountSelector {
                        accounts: accounts(),
                        active_address: active_account_address(),
                        on_select: move |address| cmd_tx.send(DashboardCmd::SetActiveAccount(address)),
                    }
                }
            }

            div {
                class: "dash-assets-shell",
                style: "position: relative; z-index: 40; border: 1px solid var(--border); background: var(--card);",
                onclick: move |e| e.stop_propagation(),
                button {
                    r#type: "button",
                    style: "width: 100%; padding: 14px 16px; background: transparent; border: none; color: var(--foreground); cursor: pointer; text-align: left;",
                    title: "Select asset. Add tokens in Settings.",
                    onclick: move |_| assets_open.set(!assets_open()),
                    div { style: "display: grid; grid-template-columns: 1fr auto; align-items: center; gap: 12px;",
                        span { style: "font-weight: 700; font-size: 15px;", "{asset_row_symbol}" }
                        span { class: "muted", style: "font-size: 15px; text-align: right; font-variant-numeric: tabular-nums;",
                            "{asset_row_balance}"
                        }
                    }
                }
                if assets_open() {
                    div {
                        style: "position: absolute; left: 0; right: 0; top: 100%; margin-top: 4px; border: 1px solid var(--border); background: var(--card); z-index: 50; box-shadow: 0 8px 24px rgba(0,0,0,0.35);",
                        button {
                            r#type: "button",
                            style: "width: 100%; padding: 10px 16px; display: grid; grid-template-columns: 1fr auto; gap: 8px; background: transparent; border: none; border-bottom: 1px solid var(--border); color: var(--foreground); cursor: pointer; text-align: left;",
                            onclick: move |_| {
                                selected_asset.set("native".into());
                                assets_open.set(false);
                            },
                            span { style: "font-weight: 700;", "{display_symbol}" }
                            span { class: "muted", style: "text-align: right;", "{display_balance}" }
                        }
                        for tok in tracked_tokens() {
                            button {
                                key: "{tok.contract_address.clone().unwrap_or_default()}",
                                r#type: "button",
                                style: "width: 100%; padding: 10px 16px; display: grid; grid-template-columns: 1fr auto; gap: 8px; background: transparent; border: none; border-bottom: 1px solid var(--border); color: var(--foreground); cursor: pointer; text-align: left;",
                                onclick: {
                                    let addr = tok.contract_address.clone().unwrap_or_default();
                                    move |_| {
                                        selected_asset.set(addr.clone());
                                        assets_open.set(false);
                                    }
                                },
                                span { style: "font-weight: 700;", "{tok.symbol}" }
                                span { class: "muted", style: "text-align: right;", "—" }
                            }
                        }
                        if tracked_tokens().is_empty() {
                            p { class: "muted", style: "margin: 0; padding: 12px 16px; font-size: 13px; text-align: center;",
                                "No custom tokens tracked — add in Settings."
                            }
                        }
                    }
                }
            }

            div {
                class: "card-panel",
                style: "padding: 16px;",
                onclick: move |e| e.stop_propagation(),
                div { style: "display: flex; flex-direction: column; gap: 12px;",
                    div {
                        label { class: "field-label", "To address" }
                        input {
                            class: "input-std input-mono",
                            r#type: "text",
                            placeholder: "Recipient address (0x...)",
                            value: "{recipient.read()}",
                            oninput: move |e| *recipient.write() = e.value(),
                        }
                    }
                    div {
                        label { class: "field-label", "Send" }
                        input {
                            class: "input-std input-mono",
                            r#type: "text",
                            placeholder: "0.0",
                            value: "{send_amount.read()}",
                            oninput: move |e| *send_amount.write() = e.value(),
                        }
                    }
                    button {
                        r#type: "button",
                        class: "vaughan-btn",
                        style: "width: 100%; margin-top: 8px;",
                        disabled: recipient.read().trim().is_empty() || send_amount.read().trim().is_empty(),
                        onclick: move |_| on_go_send.call(()),
                        "Send"
                    }
                    p { class: "muted", style: "margin: 0; font-size: 11px;",
                        "Continue on the Send screen for fees and broadcast."
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
            let mut adapter: Arc<dyn ChainAdapter> =
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
            let mut adapter_network_key = current_network_key(&services).await;
            crate::chain_bootstrap::register_default_evm_adapter(&wallet_state, adapter.clone())
                .await;

            let mut watcher: Option<BalanceWatcher> = None;
            let mut updates_rx: Option<tokio::sync::mpsc::UnboundedReceiver<BalanceEvent>> = None;
            let mut network_tick = tokio::time::interval(Duration::from_secs(2));

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
                                let pw = services.session_password().await;
                                crate::chain_bootstrap::reconcile_and_sync_wallet_state(
                                    &wallet_state,
                                    services.account_manager.as_ref(),
                                    pw.as_deref(),
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
                            DashboardCmd::SetActiveNetwork(id) => {
                                if services.network_service.set_active_network(&id).await.is_err() {
                                    continue;
                                }
                                let _ = services
                                    .persistence
                                    .update_and_save(|st| st.active_network_id = Some(id.clone()))
                                    .await;
                                // Immediate refresh; adapter/watcher swap happens via network tick.
                                if let Ok(b) = wallet_state.get_active_balance().await {
                                    runtime.balance.set(Some(b.clone()));
                                    runtime
                                        .balance_events
                                        .with_mut(|v: &mut Vec<BalanceEvent>| v.push(BalanceEvent { balance: b }));
                                }
                            }
                            DashboardCmd::SetActiveAccount(address) => {
                                let acc = services
                                    .account_manager
                                    .list_accounts()
                                    .await
                                    .into_iter()
                                    .find(|a| format!("{:?}", a.address) == address);
                                let Some(acc) = acc else { continue; };
                                if services.account_manager.set_active(acc.id).await.is_err() {
                                    continue;
                                }
                                if wallet_state.set_active_account_by_id(acc.id).await.is_err() {
                                    wallet_state.add_account(acc.clone()).await;
                                    let _ = wallet_state.set_active_account_by_id(acc.id).await;
                                }
                                if let Ok(b) = wallet_state.get_active_balance().await {
                                    runtime.balance.set(Some(b.clone()));
                                    runtime
                                        .balance_events
                                        .with_mut(|v: &mut Vec<BalanceEvent>| v.push(BalanceEvent { balance: b }));
                                }
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

                                    let had_watcher = watcher.is_some();
                                    if let Some(w) = watcher.take() {
                                        w.stop().await;
                                    }
                                    updates_rx = None;

                                    if had_watcher {
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

                                    if let Ok(b) = wallet_state.get_active_balance().await {
                                        runtime.balance.set(Some(b.clone()));
                                        runtime
                                            .balance_events
                                            .with_mut(|v: &mut Vec<BalanceEvent>| v.push(BalanceEvent { balance: b }));
                                    }
                                }
                                Err(e) => {
                                    tracing::error!(
                                        target: "vaughan_gui",
                                        error = %e,
                                        "failed to rebuild adapter after network switch"
                                    );
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
