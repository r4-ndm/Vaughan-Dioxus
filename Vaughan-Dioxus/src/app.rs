use dioxus::prelude::*;

use vaughan_core::monitoring::BalanceEvent;

use crate::components::ColoredAddressText;
use crate::services::shared_services;
use crate::theme::ThemeStyles;
use crate::views::dapps::DappsView;
use crate::views::dashboard::{use_dashboard_coroutine, DashboardCmd, DashboardView};
use crate::views::history::{provide_history_runtime, use_history_coroutine, HistoryView};
use crate::views::import_export::{
    provide_import_export_runtime, use_import_export_coroutine, ImportExportView,
};
use crate::views::onboarding::OnboardingView;
use crate::views::receive::{provide_receive_runtime, ReceiveView};
use crate::views::send::{use_send_coroutine, SendView};
use crate::views::settings::{provide_settings_runtime, use_settings_coroutine, SettingsView};
use crate::views::unlock::StartupUnlockView;

#[derive(Clone, Copy, PartialEq, Eq)]
enum WalletPhase {
    Onboarding,
    Unlock,
    Main,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum AppView {
    Dashboard,
    Send,
    Receive,
    History,
    Dapps,
    Settings,
    ImportExport,
}

#[derive(Clone)]
pub struct AppRuntime {
    pub balance: Signal<Option<vaughan_core::chains::Balance>>,
    pub balance_events: Signal<Vec<BalanceEvent>>,
}

fn show_bottom_dock(view: AppView) -> bool {
    matches!(view, AppView::Dashboard)
}

#[component]
fn ActionDock(
    on_navigate: Callback<AppView>,
    on_refresh: Callback<()>,
    on_hardware: Callback<()>,
) -> Element {
    rsx! {
        div { class: "actions-dock",
            div { class: "actions-grid-4",
                button {
                    class: "vaughan-btn",
                    onclick: move |_| on_refresh.call(()),
                    "Refresh"
                }
                button {
                    class: "vaughan-btn",
                    onclick: move |_| on_navigate.call(AppView::Receive),
                    "Receive"
                }
                button {
                    class: "vaughan-btn",
                    onclick: move |_| on_navigate.call(AppView::Dapps),
                    "Dapps"
                }
                button {
                    class: "vaughan-btn",
                    onclick: move |_| on_navigate.call(AppView::ImportExport),
                    "Create"
                }
                button {
                    class: "vaughan-btn",
                    onclick: move |_| on_navigate.call(AppView::ImportExport),
                    "Import"
                }
                button {
                    class: "vaughan-btn",
                    onclick: move |_| on_hardware.call(()),
                    "Hardware"
                }
                button {
                    class: "vaughan-btn",
                    onclick: move |_| on_navigate.call(AppView::Settings),
                    "Settings"
                }
                button {
                    class: "vaughan-btn",
                    onclick: move |_| on_navigate.call(AppView::History),
                    "History"
                }
            }
        }
    }
}

#[component]
pub fn WalletApp() -> Element {
    let services = use_hook(shared_services);

    let view = use_signal(|| AppView::Dashboard);
    let phase = use_signal(|| {
        if services.account_manager.has_master_wallet() {
            WalletPhase::Unlock
        } else {
            WalletPhase::Onboarding
        }
    });
    let mut show_hardware_modal = use_signal(|| false);

    let finish_onboarding = use_callback({
        let mut phase = phase.clone();
        move |_| phase.set(WalletPhase::Main)
    });

    let on_startup_unlocked = use_callback({
        let mut phase = phase.clone();
        move |_| phase.set(WalletPhase::Main)
    });

    use_effect({
        let services = services.clone();
        let mut phase = phase.clone();
        move || {
            let services = services.clone();
            spawn(async move {
                if services.account_manager.has_master_wallet()
                    && services.session_password().await.is_some()
                {
                    phase.set(WalletPhase::Main);
                }
            });
        }
    });

    use_context_provider(|| services.clone());
    use_context_provider(|| services.wallet_state.clone());

    let balance = use_signal(|| None);
    let balance_events = use_signal(|| Vec::<BalanceEvent>::new());
    use_context_provider(|| AppRuntime {
        balance,
        balance_events,
    });

    let dash = use_dashboard_coroutine();
    let (send_rt, send_co) = use_send_coroutine();
    use_context_provider(|| send_rt);
    let receive_rt = provide_receive_runtime();
    use_context_provider(|| receive_rt.clone());

    let history_rt = provide_history_runtime();
    use_context_provider(|| history_rt.clone());
    let hist = use_history_coroutine();
    let settings_rt = provide_settings_runtime();
    use_context_provider(|| settings_rt.clone());
    let settings = use_settings_coroutine();
    let ie_rt = provide_import_export_runtime();
    use_context_provider(|| ie_rt.clone());
    let ie = use_import_export_coroutine();

    let on_navigate = use_callback({
        let mut view = view.clone();
        move |next: AppView| *view.write() = next
    });

    let on_back = use_callback({
        let mut view = view.clone();
        move |_| *view.write() = AppView::Dashboard
    });

    let on_refresh = use_callback({
        let dash = dash.clone();
        move |_| dash.send(DashboardCmd::RefreshOnce)
    });

    let on_hardware = use_callback({
        let mut show_hardware_modal = show_hardware_modal.clone();
        move |_| show_hardware_modal.set(true)
    });

    let on_go_send = use_callback({
        let mut view = view.clone();
        move |_| *view.write() = AppView::Send
    });

    let header_account_addr = use_signal(|| String::new());

    use_effect({
        let services = services.clone();
        let view = view.clone();
        let phase = phase.clone();
        let header_account_addr = header_account_addr.clone();
        move || {
            let _ = *view.read();
            let _ = *phase.read();
            let mgr = services.account_manager.clone();
            let mut header_account_addr = header_account_addr.clone();
            spawn(async move {
                let s = match mgr.active_account().await {
                    Some(a) => format!("{:?}", a.address),
                    None => mgr
                        .list_accounts()
                        .await
                        .first()
                        .map(|a| format!("{:?}", a.address))
                        .unwrap_or_default(),
                };
                header_account_addr.set(s);
            });
        }
    });

    let on_wallet_deleted = use_callback({
        let mut phase = phase.clone();
        let mut view = view.clone();
        let mut header_account_addr = header_account_addr.clone();
        move |_| {
            phase.set(WalletPhase::Onboarding);
            *view.write() = AppView::Dashboard;
            header_account_addr.set(String::new());
        }
    });

    rsx! {
        match *phase.read() {
            WalletPhase::Onboarding => rsx! {
                ThemeStyles {}
                OnboardingView { on_complete: finish_onboarding }
            },
            WalletPhase::Unlock => rsx! {
                StartupUnlockView { on_unlocked: on_startup_unlocked }
            },
            WalletPhase::Main => rsx! {
            ThemeStyles {}
            div { class: "wallet-shell",
                header { class: "wallet-logo-block",
                    h1 { class: "vaughan-logo-gradient", "VAUGHAN" }
                    if !header_account_addr.read().is_empty() {
                        div { class: "header-active-account",
                            ColoredAddressText { address: header_account_addr.read().clone() }
                        }
                    }
                }

                main { class: "content-stack",
                    match *view.read() {
                        AppView::Dashboard => rsx! {
                            DashboardView { cmd_tx: dash, on_go_send: on_go_send.clone() }
                        },
                        AppView::Send => rsx! { SendView { cmd_tx: send_co, on_back: on_back.clone() } },
                        AppView::Receive => rsx! { ReceiveView { on_back: on_back.clone() } },
                        AppView::History => rsx! { HistoryView { cmd_tx: hist, on_back: on_back.clone() } },
                        AppView::Dapps => rsx! { DappsView { on_back: on_back.clone() } },
                        AppView::Settings => rsx! {
                            SettingsView {
                                cmd_tx: settings,
                                on_back: on_back.clone(),
                                on_wallet_deleted: on_wallet_deleted.clone(),
                            }
                        },
                        AppView::ImportExport => rsx! { ImportExportView { cmd_tx: ie, on_back: on_back.clone() } },
                    }
                }

                if show_bottom_dock(*view.read()) {
                    ActionDock {
                        on_navigate: on_navigate.clone(),
                        on_refresh: on_refresh.clone(),
                        on_hardware: on_hardware.clone(),
                    }
                }
            }

            if *show_hardware_modal.read() {
                div {
                    class: "modal-overlay",
                    onclick: move |_| show_hardware_modal.set(false),
                    div {
                        class: "modal-sheet",
                        onclick: move |evt| evt.stop_propagation(),
                        h2 { style: "margin: 0 0 8px 0; font-size: 1.1rem;", "Hardware Wallet" }
                        p { style: "margin: 0; color: var(--muted-foreground); font-size: 14px;",
                            "Coming soon — Trezor and Ledger support."
                        }
                        button {
                            class: "vaughan-btn",
                            style: "margin-top: 16px;",
                            onclick: move |_| show_hardware_modal.set(false),
                            "Close"
                        }
                    }
                }
            }
            },
        }
    }
}
