//! Vaughan Wallet — Dioxus entry point (desktop + mobile).

mod app;
mod browser;
mod chain_bootstrap;
mod components;
mod dapp_approval;
mod services;
mod theme;
mod utils;
mod views;
mod wallet_ipc;

fn main() {
    vaughan_core::logging::init_logging();
    tracing::info!(target: "vaughan_app", "logging initialized");

    #[cfg(feature = "mobile")]
    {
        tracing::info!(target: "vaughan_app", "launching mobile shell");
        dioxus_mobile::launch::launch(app::WalletApp);
        return;
    }

    #[cfg(not(feature = "mobile"))]
    {
        tracing::info!(target: "vaughan_app", "launching desktop wallet");
        let services = services::shared_services();
        tracing::info!(target: "vaughan_app", "shared services ready");

        let _browser_guard = browser::BrowserProcessGuard::launch_if_available(services);
        tracing::info!(
            target: "vaughan_app",
            "dApp browser IPC armed (child process starts on first trusted dApp open)"
        );

        let desktop_cfg = dioxus_desktop::Config::new().with_custom_event_handler(|event, _| {
            use dioxus_desktop::tao::event::Event;
            if matches!(event, Event::LoopDestroyed) {
                tracing::info!(target: "vaughan_app", "desktop event loop destroyed (graceful shutdown path)");
            }
        });

        dioxus::LaunchBuilder::new()
            .with_cfg(desktop_cfg)
            .launch(app::WalletApp);
    }
}
