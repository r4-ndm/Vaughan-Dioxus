use dioxus::prelude::*;

use std::sync::Arc;

use vaughan_core::core::{HistoryService, NetworkService, TokenManager, WalletState};
use vaughan_core::core::account::AccountManager;
use vaughan_core::monitoring::BalanceEvent;

use crate::theme::ThemeStyles;
use crate::views::dashboard::{DashboardView, use_dashboard_coroutine};
use crate::views::send::{SendView, use_send_coroutine};
use crate::views::receive::{ReceiveView, provide_receive_runtime};
use crate::views::history::{HistoryView, provide_history_runtime, use_history_coroutine};
use crate::views::settings::{SettingsView, provide_settings_runtime, use_settings_coroutine};
use crate::views::import_export::{ImportExportView, provide_import_export_runtime, use_import_export_coroutine};

#[derive(Clone)]
pub struct AppServices {
    #[allow(dead_code)]
    pub wallet_state: Arc<WalletState>,
    #[allow(dead_code)]
    pub network_service: Arc<NetworkService>,
    #[allow(dead_code)]
    pub history_service: Arc<HistoryService>,
    #[allow(dead_code)]
    pub account_manager: Arc<AccountManager>,
    #[allow(dead_code)]
    pub token_manager: Arc<TokenManager>,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum AppView {
    Dashboard,
    #[allow(dead_code)]
    Send,
    Receive,
    History,
    #[allow(dead_code)]
    Dapps,
    Settings,
    ImportExport,
}

#[derive(Clone)]
pub struct AppRuntime {
    #[allow(dead_code)]
    pub balance: Signal<Option<vaughan_core::chains::Balance>>,
    #[allow(dead_code)]
    pub balance_events: Signal<Vec<BalanceEvent>>,
}

#[component]
fn ActionButtons(on_navigate: Callback<AppView>) -> Element {
    let mk = |label: &'static str, view: AppView| {
        let on_navigate = on_navigate.clone();
        rsx! {
            button {
                style: "flex: 1; padding: 10px 12px; background: #0b0b0b; border: 1px solid #222; color: #e6e6e6; font-size: 14px; cursor: pointer;",
                onclick: move |_| on_navigate.call(view),
                "{label}"
            }
        }
    };

    rsx! {
        div { class: "btn-row",
            {mk("Send", AppView::Send)}
            {mk("Receive", AppView::Receive)}
            {mk("Import/Export", AppView::ImportExport)}
            {mk("History", AppView::History)}
            {mk("Settings", AppView::Settings)}
        }
    }
}

#[component]
pub fn WalletApp() -> Element {
    // Core services
    let services = use_hook(|| {
        AppServices {
            wallet_state: Arc::new(WalletState::new()),
            network_service: Arc::new(NetworkService::new()),
            history_service: Arc::new(HistoryService::new(std::time::Duration::from_secs(10))),
            account_manager: Arc::new(AccountManager::new("vaughan-wallet").expect("AccountManager init")),
            token_manager: Arc::new(TokenManager::new()),
        }
    });

    // UI state
    let mut view = use_signal(|| AppView::Dashboard);

    // Provide services to all children.
    use_context_provider(|| services.clone());
    // Provide WalletState directly for convenience (Task 14.6).
    use_context_provider(|| services.wallet_state.clone());

    // Runtime state (events/data the UI reacts to).
    let balance = use_signal(|| None);
    let balance_events = use_signal(|| Vec::<BalanceEvent>::new());
    use_context_provider(|| AppRuntime {
        balance,
        balance_events,
    });

    // Dashboard coroutine handles async operations (balance fetch + watcher).
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

    let on_navigate = use_callback(move |next: AppView| {
        *view.write() = next;
    });

    rsx! {
        ThemeStyles {}
        div { class: "wallet-shell",
            header { class: "topbar",
                h1 { class: "logo", "VAUGHAN" }
                p { class: "muted", style: "margin-top: 6px; margin-bottom: 0; font-size: 12px;",
                    "Desktop-first UI (ported structure from Vaughan-old/web)."
                }
            }

            main { class: "content",
                match *view.read() {
                    AppView::Dashboard => rsx! {
                        DashboardView { cmd_tx: dash }
                    },
                    AppView::Send => rsx! { SendView { cmd_tx: send_co } },
                    AppView::Receive => rsx! { ReceiveView {} },
                    AppView::History => rsx! { HistoryView { cmd_tx: hist } },
                    AppView::Dapps => rsx! { h2 { "DApps" } p { class: "muted", "Stub view" } },
                    AppView::Settings => rsx! { SettingsView { cmd_tx: settings } },
                    AppView::ImportExport => rsx! { ImportExportView { cmd_tx: ie } },
                }
            }

            ActionButtons { on_navigate }
        }
    }
}
