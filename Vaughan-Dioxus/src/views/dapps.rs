use std::collections::HashSet;
use std::sync::Arc;
use std::time::Duration;

use dioxus::prelude::*;
use keyboard_types::Key;
use url::Url;

use vaughan_core::core::WalletState;
use vaughan_core::monitoring::BalanceEvent;
use vaughan_core::native_dapps;

use crate::app::AppRuntime;
use crate::pulsex_local;
use crate::browser::{
    self, format_user_dapp_url, google_favicon_url_for_dapp, trusted_dapp_visible_on_chain,
    validate_whitelisted_dapp_url, TrustedDapp, TRUSTED_DAPP_ENTRIES,
};
use crate::chain_bootstrap::refresh_evm_adapter_for_active_network;
use crate::components::{AccountOption, AccountSelector, NetworkOption, NetworkSelector};
use crate::dapp_approval::{broker, PendingSignMessage, PendingSignTransaction};
use crate::services::AppServices;

/// Curated entry for loopback PulseX (`browser.rs`); controls use the same URL key.
const PULSEX_LOCAL_URL: &str = "http://127.0.0.1:3691";
const PULSEX_SERVER_BIND: &str = "127.0.0.1:3691";

fn pulsex_install_supported() -> bool {
    cfg!(all(target_os = "linux", target_arch = "x86_64"))
}

#[derive(Clone, PartialEq, Eq)]
struct CustomDappEntry {
    name: String,
    url: String,
    description: String,
}

fn host_label(url: &str) -> String {
    Url::parse(url)
        .ok()
        .and_then(|u| u.host_str().map(|s| s.to_string()))
        .unwrap_or_else(|| url.to_string())
}

/// Either show the rocket warm-shell window or open a fresh one.
fn open_dapp(url: &str, is_fast: bool) -> Result<(), String> {
    if is_fast {
        browser::open_trusted_dapp_url_prefer_warm_window(url)
    } else {
        browser::open_trusted_dapp_url_new_window(url)
    }
}

/// Toggle a dApp in the per-chain fast-list, capped at 6, and persist to disk.
fn apply_fast_toggle(
    services: &AppServices,
    mut fast_dapp_keys: Signal<HashSet<String>>,
    mut err_sig: Signal<Option<String>>,
    chain_id: u64,
    key: String,
) {
    let mut next = fast_dapp_keys.read().clone();
    if next.contains(&key) {
        next.remove(&key);
    } else if next.len() >= 6 {
        err_sig.set(Some(
            "You can select up to 6 fast dApps. Deselect one first.".into(),
        ));
        return;
    } else {
        next.insert(key);
    }
    err_sig.set(None);
    let persisted: Vec<String> = next.iter().cloned().collect();
    let chain_key = chain_id.to_string();
    fast_dapp_keys.set(next);
    let services = services.clone();
    spawn(async move {
        let _ = services
            .persistence
            .update_and_save(|st| {
                let mut prefs = st.preferences.clone().unwrap_or_default();
                if prefs.polling_interval_secs == 0 {
                    prefs.polling_interval_secs = 10;
                }
                prefs.fast_dapps_by_chain_v1.insert(chain_key, persisted);
                st.preferences = Some(prefs);
            })
            .await;
    });
}

/// Kicks off the PulseX install/update flow on a background task. `success_msg`
/// is shown when the download + extract + checksum verify succeeds.
fn run_pulsex_install(
    services: AppServices,
    mut pulsex_busy: Signal<bool>,
    mut pulsex_toast: Signal<Option<String>>,
    success_msg: &'static str,
) {
    if *pulsex_busy.read() {
        return;
    }
    pulsex_busy.set(true);
    pulsex_toast.set(None);
    spawn(async move {
        let res = async {
            let m = native_dapps::load_pulsex_manifest(true).await?;
            native_dapps::download_install_pulsex_for_current_target(
                &m,
                services.persistence.clone(),
            )
            .await?;
            Ok(()) as Result<(), vaughan_core::error::WalletError>
        }
        .await;
        pulsex_busy.set(false);
        match res {
            Ok(()) => pulsex_toast.set(Some(success_msg.into())),
            Err(err) => pulsex_toast.set(Some(err.to_string())),
        }
    });
}

/// PulseX-local install / update / start / stop controls; rendered inside the
/// PulseX card via `DappCard`'s `children` slot.
#[component]
fn PulsexLocalControls(
    pulsex_installed_ver: Signal<Option<String>>,
    pulsex_running: Signal<bool>,
    pulsex_update_available: Signal<bool>,
    pulsex_busy: Signal<bool>,
    pulsex_toast: Signal<Option<String>>,
) -> Element {
    let services: AppServices = use_context();
    rsx! {
        div { style: "margin-top: 8px; padding-top: 8px; border-top: 1px solid var(--border); display: flex; flex-direction: column; gap: 6px;",
            p { class: "muted", style: "margin: 0; font-size: 10px; line-height: 1.35;",
                if pulsex_install_supported() {
                    "Option B: public `pulsex-manifest.json` + SHA-256; binary lives under your app data directory."
                } else {
                    "Automated download targets Linux x86-64. On other systems, run `pulsex-server` yourself."
                }
            }
            if pulsex_install_supported() {
                div { style: "display: flex; flex-wrap: wrap; gap: 6px; align-items: center;",
                    if pulsex_installed_ver.read().is_none() {
                        button {
                            class: "btn",
                            disabled: *pulsex_busy.read(),
                            onclick: {
                                let services = services.clone();
                                move |e| {
                                    e.stop_propagation();
                                    run_pulsex_install(
                                        services.clone(),
                                        pulsex_busy,
                                        pulsex_toast,
                                        "Installed and verified.",
                                    );
                                }
                            },
                            "Install"
                        }
                    } else if *pulsex_update_available.read() {
                        button {
                            class: "btn",
                            disabled: *pulsex_busy.read(),
                            onclick: {
                                let services = services.clone();
                                move |e| {
                                    e.stop_propagation();
                                    run_pulsex_install(
                                        services.clone(),
                                        pulsex_busy,
                                        pulsex_toast,
                                        "Updated.",
                                    );
                                }
                            },
                            "Update"
                        }
                    }
                    if pulsex_installed_ver.read().is_some() {
                        if *pulsex_running.read() {
                            button {
                                class: "btn",
                                disabled: *pulsex_busy.read(),
                                onclick: {
                                    let mut pulsex_busy = pulsex_busy;
                                    let mut pulsex_toast = pulsex_toast;
                                    move |e| {
                                        e.stop_propagation();
                                        if *pulsex_busy.read() { return; }
                                        pulsex_busy.set(true);
                                        pulsex_local::stop_pulsex_local();
                                        pulsex_busy.set(false);
                                        pulsex_toast.set(Some("Server stopped.".into()));
                                    }
                                },
                                "Stop server"
                            }
                        } else {
                            button {
                                class: "btn",
                                disabled: *pulsex_busy.read(),
                                onclick: {
                                    let services = services.clone();
                                    let mut pulsex_busy = pulsex_busy;
                                    let mut pulsex_toast = pulsex_toast;
                                    move |e| {
                                        e.stop_propagation();
                                        if *pulsex_busy.read() { return; }
                                        let Some(rec) = native_dapps::pulsex_record(&services.persistence) else {
                                            pulsex_toast.set(Some("Install the local server first.".into()));
                                            return;
                                        };
                                        pulsex_busy.set(true);
                                        pulsex_toast.set(None);
                                        // Synchronous on the click path: `spawn` without await was unreliable
                                        // here and could leave `pulsex_busy` stuck.
                                        let r = pulsex_local::start_pulsex_local(
                                            &rec.binary_path,
                                            PULSEX_SERVER_BIND,
                                        );
                                        pulsex_busy.set(false);
                                        match r {
                                            Ok(()) => pulsex_toast.set(Some("Server started.".into())),
                                            Err(msg) => pulsex_toast.set(Some(msg)),
                                        }
                                    }
                                },
                                "Start server"
                            }
                        }
                    }
                }
            }
            if let Some(note) = pulsex_toast.read().as_ref() {
                p { style: "margin: 0; font-size: 10px; color: var(--muted-foreground);",
                    "{note}"
                }
            }
        }
    }
}

/// Single dApp tile rendered in the dApps grid. Used for both the curated
/// `TrustedDapp` list and user-added custom entries; per-card extras (like the
/// PulseX local install/run controls) come in via `children`.
#[component]
fn DappCard(
    name: String,
    url: String,
    description: String,
    category: String,
    is_fast: bool,
    fully_warmed: bool,
    remove_title: String,
    on_open: Callback<()>,
    on_remove: Callback<()>,
    on_toggle_fast: Callback<()>,
    children: Element,
) -> Element {
    let fav = google_favicon_url_for_dapp(&url).unwrap_or_default();
    let host = host_label(&url);
    let rocket_style = if is_fast {
        "min-width: 36px; opacity: 1; filter: grayscale(0) saturate(1.25);"
    } else {
        "min-width: 36px; opacity: 0.35; filter: grayscale(1) saturate(0.2);"
    };
    rsx! {
        div {
            class: "dapp-card",
            onclick: move |_| on_open.call(()),
            button {
                class: "dapp-card-remove",
                title: "{remove_title}",
                onclick: move |e| {
                    e.stop_propagation();
                    on_remove.call(());
                },
                "🗑"
            }
            div {
                div { class: "dapp-card-head",
                    div { class: "dapp-card-icon-wrap",
                        img { src: "{fav}", alt: "{name}" }
                    }
                    span { class: "dapp-card-ext", "↗" }
                }
                div {
                    h3 { style: "margin: 8px 0 4px 0; font-size: 15px; font-weight: 700; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; padding-right: 28px;",
                        "{name}"
                    }
                    p { class: "muted", style: "margin: 0; font-size: 11px; line-height: 1.35; overflow: hidden; display: -webkit-box; -webkit-line-clamp: 2; -webkit-box-orient: vertical;",
                        "{description}"
                    }
                }
                div { style: "margin-top: 12px; padding-top: 8px; border-top: 1px solid var(--border); display: flex; align-items: center; justify-content: space-between; gap: 8px;",
                    span { class: "dapp-card-cat", "{category}" }
                    span { class: "dapp-card-host", "{host}" }
                    button {
                        class: "btn",
                        title: "Prioritize this dApp for fast prewarm",
                        style: "{rocket_style}",
                        onclick: move |e| {
                            e.stop_propagation();
                            on_toggle_fast.call(());
                        },
                        "🚀"
                    }
                }
                if is_fast {
                    div { style: "margin-top: 8px; display: flex; align-items: center; gap: 8px;",
                        span {
                            style: if fully_warmed {
                                "font-size: 10px; font-weight: 700; color: #a8f6c3;"
                            } else {
                                "font-size: 10px; font-weight: 700; color: #ffd58a;"
                            },
                            if fully_warmed { "🚀 Fully Warmed" } else { "🚀 Not Warmed Yet" }
                        }
                        div { style: "height: 6px; flex: 1; border-radius: 999px; background: rgba(255,255,255,0.14); overflow: hidden;",
                            div {
                                style: if fully_warmed {
                                    "height: 100%; width: 100%; background: rgba(15,161,95,0.95);"
                                } else {
                                    "height: 100%; width: 35%; background: rgba(214,164,38,0.95);"
                                }
                            }
                        }
                    }
                }
                {children}
            }
        }
    }
}

#[component]
pub fn DappsView(on_back: Callback<()>) -> Element {
    let services: AppServices = use_context();
    let wallet_state: Arc<WalletState> = use_context();
    let runtime: AppRuntime = use_context();

    let networks = use_signal(Vec::<NetworkOption>::new);
    let active_network_id = use_signal(|| None::<String>);
    let active_chain_id = use_signal(|| 1u64);
    let accounts = use_signal(Vec::<AccountOption>::new);
    let active_account_address = use_signal(|| None::<String>);

    let no_accounts_for_dapps = use_signal(|| false);
    let selectors_booted = use_signal(|| false);

    let mut custom_url = use_signal(String::new);
    let mut custom_dapps = use_signal(Vec::<CustomDappEntry>::new);
    let mut hidden_core_urls = use_signal(Vec::<String>::new);
    let fast_dapp_keys = use_signal(HashSet::<String>::new);
    let last_fast_chain_loaded = use_signal(|| None::<u64>);
    let mut url_bar_error = use_signal(|| None::<String>);
    let mut launching_custom = use_signal(|| false);

    let pending_message = use_signal(|| broker().pending_sign_message());
    let pending_tx = use_signal(|| broker().pending_sign_transaction());
    let mut dapp_open_error = use_signal(|| None::<String>);

    let pulsex_installed_ver = use_signal(|| None::<String>);
    let pulsex_running = use_signal(|| false);
    let pulsex_update_available = use_signal(|| false);
    let pulsex_busy = use_signal(|| false);
    let pulsex_toast = use_signal(|| None::<String>);
    let pulsex_poll_booted = use_signal(|| false);

    let mut refresh = {
        let mut pending_message = pending_message;
        let mut pending_tx = pending_tx;
        move || {
            pending_message.set(broker().pending_sign_message());
            pending_tx.set(broker().pending_sign_transaction());
        }
    };

    // Fast dApps are stored per chain; `active_chain_id` may start at 1 then update when the
    // network poll restores PulseChain — reload whenever the chain id actually changes.
    use_effect({
        let services = services.clone();
        let mut fast_dapp_keys = fast_dapp_keys;
        let mut last_fast_chain_loaded = last_fast_chain_loaded;
        move || {
            let cid = active_chain_id();
            if *last_fast_chain_loaded.read() == Some(cid) {
                return;
            }
            last_fast_chain_loaded.set(Some(cid));
            let pref = services
                .persistence
                .snapshot()
                .preferences
                .unwrap_or_default();
            let chain_key = cid.to_string();
            let fast_for_chain = pref
                .fast_dapps_by_chain_v1
                .get(&chain_key)
                .cloned()
                .unwrap_or_default();
            fast_dapp_keys.set(fast_for_chain.into_iter().take(6).collect());
        }
    });

    use_effect({
        let services = services.clone();
        let mut pulsex_installed_ver = pulsex_installed_ver;
        let mut pulsex_running = pulsex_running;
        let mut pulsex_update_available = pulsex_update_available;
        let mut pulsex_poll_booted = pulsex_poll_booted;
        move || {
            if pulsex_poll_booted() {
                return;
            }
            pulsex_poll_booted.set(true);
            let persistence = services.persistence.clone();
            spawn(async move {
                loop {
                    let rec = native_dapps::pulsex_record(&persistence);
                    pulsex_installed_ver.set(rec.as_ref().map(|r| r.installed_version.clone()));
                    pulsex_running.set(pulsex_local::is_pulsex_running());
                    if let Ok(m) =
                        native_dapps::parse_manifest_json(native_dapps::embedded_manifest_str())
                    {
                        pulsex_update_available
                            .set(native_dapps::pulsex_update_available(&m, rec.as_ref()));
                    }
                    tokio::time::sleep(Duration::from_secs(2)).await;
                }
            });
        }
    });

    use_effect({
        let services = services.clone();
        move || {
            let mgr = services.account_manager.clone();
            let mut no_accounts_for_dapps = no_accounts_for_dapps;
            spawn(async move {
                let empty = mgr.list_accounts().await.is_empty();
                no_accounts_for_dapps.set(empty);
            });
        }
    });

    use_effect({
        let services = services.clone();
        let mut networks = networks;
        let mut active_network_id = active_network_id;
        let mut active_chain_id = active_chain_id;
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
                    let chain = services
                        .network_service
                        .active_network()
                        .await
                        .map(|n| n.chain_id)
                        .unwrap_or(1);
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
                    networks.set(nets);
                    active_network_id.set(active_net);
                    active_chain_id.set(chain);
                    accounts.set(accts);
                    active_account_address.set(active_acct);
                    tokio::time::sleep(Duration::from_secs(2)).await;
                }
            });
        }
    });

    let on_network = {
        let services = services.clone();
        let wallet_state = wallet_state.clone();
        let runtime = runtime.clone();
        let mut active_network_id = active_network_id;
        let mut active_chain_id = active_chain_id;
        let mut fast_dapp_keys = fast_dapp_keys;
        move |id: String| {
            let services = services.clone();
            let wallet_state = wallet_state.clone();
            let mut runtime = runtime.clone();
            spawn(async move {
                if services
                    .network_service
                    .set_active_network(&id)
                    .await
                    .is_err()
                {
                    return;
                }
                if let Some(net) = services.network_service.active_network().await {
                    active_network_id.set(Some(net.id.clone()));
                    active_chain_id.set(net.chain_id);
                    let chain_key = net.chain_id.to_string();
                    let pref = services
                        .persistence
                        .snapshot()
                        .preferences
                        .unwrap_or_default();
                    let fast_for_chain = pref
                        .fast_dapps_by_chain_v1
                        .get(&chain_key)
                        .cloned()
                        .unwrap_or_default();
                    fast_dapp_keys.set(fast_for_chain.into_iter().take(6).collect());
                }
                let _ = services
                    .persistence
                    .update_and_save(|st| st.active_network_id = Some(id.clone()))
                    .await;
                refresh_evm_adapter_for_active_network(
                    wallet_state.as_ref(),
                    services.network_service.as_ref(),
                )
                .await;
                if let Ok(b) = wallet_state.get_active_balance().await {
                    runtime.balance.set(Some(b.clone()));
                    runtime
                        .balance_events
                        .with_mut(|v: &mut Vec<BalanceEvent>| v.push(BalanceEvent { balance: b }));
                }
            });
        }
    };

    let on_account = {
        let services = services.clone();
        let wallet_state = wallet_state.clone();
        let runtime = runtime.clone();
        move |address: String| {
            let services = services.clone();
            let wallet_state = wallet_state.clone();
            let mut runtime = runtime.clone();
            spawn(async move {
                let acc = services
                    .account_manager
                    .list_accounts()
                    .await
                    .into_iter()
                    .find(|a| format!("{:?}", a.address) == address);
                let Some(acc) = acc else { return };
                if services.account_manager.set_active(acc.id).await.is_err() {
                    return;
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
            });
        }
    };

    let chain = active_chain_id();
    let hidden = hidden_core_urls.read().clone();
    let custom_list = custom_dapps.read().clone();

    let core_filtered: Vec<&'static TrustedDapp> = TRUSTED_DAPP_ENTRIES
        .iter()
        .filter(|e| trusted_dapp_visible_on_chain(e, chain))
        .filter(|e| !hidden.contains(&e.url.to_string()))
        .collect();

    rsx! {
        div { class: "dapps-browser-shell",
            div { class: "dapps-header-wrap",
                button {
                    class: "back-link dapps-back",
                    onclick: move |_| on_back.call(()),
                    "←"
                }
                h1 { class: "dapps-title", "DApps Browser" }
            }

            div {
                class: "dapps-selectors-row",
                onclick: move |e| e.stop_propagation(),
                div {
                    NetworkSelector {
                        networks: networks(),
                        active_id: active_network_id(),
                        on_select: {
                            let on_network = on_network.clone();
                            move |id| on_network(id)
                        },
                    }
                }
                div {
                    AccountSelector {
                        accounts: accounts(),
                        active_address: active_account_address(),
                        on_select: {
                            let on_account = on_account.clone();
                            move |addr| on_account(addr)
                        },
                    }
                }
            }

            if *no_accounts_for_dapps.read() {
                p {
                    class: "muted",
                    style: "margin: 0; padding: 10px 12px; font-size: 13px; text-align: center; border: 1px solid var(--border); background: var(--card); color: var(--error-text);",
                    "No accounts in this wallet yet. Use Create or Import in the dock, then connect in the dApp."
                }
            }

            div { class: "dapp-grid",
                for d in custom_list.iter() {
                    {
                        let d = d.clone();
                        let fast_key = browser::dapp_preference_key(&d.url);
                        let is_fast = fast_dapp_keys.read().contains(&fast_key);
                        let fully_warmed = is_fast
                            && matches!(
                                browser::dapp_warm_hint_for_url(&d.url),
                                "Ready" | "Claimed"
                            );
                        let remove_url = d.url.clone();
                        let open_url = d.url.clone();
                        let services_for_toggle = services.clone();
                        rsx! {
                            DappCard {
                                key: "{d.url}",
                                name: d.name.clone(),
                                url: d.url.clone(),
                                description: d.description.clone(),
                                category: "Custom".to_string(),
                                is_fast,
                                fully_warmed,
                                remove_title: "Remove from your list".to_string(),
                                on_open: move |_| match open_dapp(&open_url, is_fast) {
                                    Ok(()) => dapp_open_error.set(None),
                                    Err(e) => dapp_open_error.set(Some(e)),
                                },
                                on_remove: move |_| {
                                    let u = remove_url.clone();
                                    custom_dapps.with_mut(|v| v.retain(|x| x.url != u));
                                },
                                on_toggle_fast: move |_| {
                                    apply_fast_toggle(
                                        &services_for_toggle,
                                        fast_dapp_keys,
                                        dapp_open_error,
                                        active_chain_id(),
                                        fast_key.clone(),
                                    );
                                },
                            }
                        }
                    }
                }
                for entry in core_filtered.iter() {
                    {
                        let entry = *entry;
                        let fast_key = browser::dapp_preference_key(entry.url);
                        let is_fast = fast_dapp_keys.read().contains(&fast_key);
                        let fully_warmed = is_fast
                            && matches!(
                                browser::dapp_warm_hint_for_url(entry.url),
                                "Ready" | "Claimed"
                            );
                        let services_for_toggle = services.clone();
                        rsx! {
                            DappCard {
                                key: "{entry.url}",
                                name: entry.name.to_string(),
                                url: entry.url.to_string(),
                                description: entry.description.to_string(),
                                category: entry.category.to_string(),
                                is_fast,
                                fully_warmed,
                                remove_title: "Hide from list".to_string(),
                                on_open: move |_| match open_dapp(entry.url, is_fast) {
                                    Ok(()) => dapp_open_error.set(None),
                                    Err(e) => dapp_open_error.set(Some(e)),
                                },
                                on_remove: move |_| {
                                    let u = entry.url.to_string();
                                    hidden_core_urls.with_mut(|v| {
                                        if !v.contains(&u) {
                                            v.push(u);
                                        }
                                    });
                                },
                                on_toggle_fast: move |_| {
                                    apply_fast_toggle(
                                        &services_for_toggle,
                                        fast_dapp_keys,
                                        dapp_open_error,
                                        active_chain_id(),
                                        fast_key.clone(),
                                    );
                                },
                                if entry.url == PULSEX_LOCAL_URL {
                                    PulsexLocalControls {
                                        pulsex_installed_ver,
                                        pulsex_running,
                                        pulsex_update_available,
                                        pulsex_busy,
                                        pulsex_toast,
                                    }
                                }
                            }
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

            div { class: "dapp-url-bar-form",
                button {
                    r#type: "button",
                    class: "dapp-url-bar-plus",
                    disabled: custom_url.read().trim().is_empty() || *launching_custom.read(),
                    title: "Add URL to your custom dApps list (must be on the trusted list)",
                    onclick: move |_| {
                        let raw = custom_url.read().clone();
                        if raw.trim().is_empty() {
                            return;
                        }
                        let formatted = format_user_dapp_url(&raw);
                        match validate_whitelisted_dapp_url(&formatted) {
                            Ok(normalized) => {
                                let dup_core = TRUSTED_DAPP_ENTRIES.iter().any(|e| e.url == normalized);
                                let dup_custom = custom_dapps.read().iter().any(|c| c.url == normalized);
                                if dup_core || dup_custom {
                                    url_bar_error.set(Some("This dApp is already in your list.".into()));
                                    return;
                                }
                                let name = host_label(&normalized);
                                custom_dapps.with_mut(|v| {
                                    v.push(CustomDappEntry {
                                        name,
                                        url: normalized.clone(),
                                        description: "Custom user-added dApp".into(),
                                    });
                                });
                                url_bar_error.set(None);
                                custom_url.set(String::new());
                            }
                            Err(e) => url_bar_error.set(Some(e)),
                        }
                    },
                    "+"
                }
                input {
                    class: "dapp-url-bar-input",
                    r#type: "text",
                    placeholder: "Type a URL — + to add to this list, Go to open in browser",
                    value: "{custom_url.read()}",
                    disabled: *launching_custom.read(),
                    oninput: move |e| {
                        *custom_url.write() = e.value();
                        url_bar_error.set(None);
                    },
                    onkeydown: move |e: Event<KeyboardData>| {
                        if e.key() == Key::Enter && !custom_url.read().trim().is_empty() && !*launching_custom.read() {
                            let raw = custom_url.read().clone();
                            let formatted = format_user_dapp_url(&raw);
                            launching_custom.set(true);
                            url_bar_error.set(None);
                            match validate_whitelisted_dapp_url(&formatted) {
                                Ok(normalized) => {
                                    match browser::open_trusted_dapp_url_new_window(&normalized) {
                                        Ok(()) => dapp_open_error.set(None),
                                        Err(err) => dapp_open_error.set(Some(err)),
                                    }
                                }
                                Err(err) => url_bar_error.set(Some(err)),
                            }
                            launching_custom.set(false);
                        }
                    },
                }
                button {
                    r#type: "button",
                    class: "dapp-url-bar-go",
                    disabled: custom_url.read().trim().is_empty() || *launching_custom.read(),
                    onclick: move |_| {
                        let raw = custom_url.read().clone();
                        if raw.trim().is_empty() {
                            return;
                        }
                        let formatted = format_user_dapp_url(&raw);
                        launching_custom.set(true);
                        url_bar_error.set(None);
                        match validate_whitelisted_dapp_url(&formatted) {
                            Ok(normalized) => {
                                match browser::open_trusted_dapp_url_new_window(&normalized) {
                                    Ok(()) => dapp_open_error.set(None),
                                    Err(err) => dapp_open_error.set(Some(err)),
                                }
                            }
                            Err(err) => url_bar_error.set(Some(err)),
                        }
                        launching_custom.set(false);
                    },
                    if *launching_custom.read() { "…" } else { "Go" }
                }
            }

            if let Some(err) = url_bar_error.read().as_ref() {
                p { style: "margin: -12px 0 0 0; font-size: 12px; color: var(--error-text); text-align: center;", "{err}" }
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
