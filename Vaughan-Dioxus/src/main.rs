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

/// Match Vaughan-Tauri main window (`src-tauri/src/lib.rs`): 80% monitor height, width = height/φ, centered.
#[cfg(not(feature = "mobile"))]
fn apply_vaughan_tauri_default_window_bounds(window: &dioxus_desktop::tao::window::Window) {
    use dioxus_desktop::tao::dpi::{PhysicalPosition, PhysicalSize};

    const PHI_INV: f64 = 1.0_f64 / 1.618_f64;
    const HEIGHT_RATIO: f64 = 0.8_f64;

    let Some(monitor) = window.primary_monitor() else {
        return;
    };
    let screen = monitor.size();
    let target_height = (screen.height as f64 * HEIGHT_RATIO) as u32;
    let target_width = (target_height as f64 * PHI_INV) as u32;

    window.set_inner_size(PhysicalSize::new(target_width, target_height));

    let outer = window.outer_size();
    let pos = monitor.position();
    let x = pos.x + (screen.width as i32 - outer.width as i32) / 2;
    let y = pos.y + (screen.height as i32 - outer.height as i32) / 2;
    window.set_outer_position(PhysicalPosition::new(x, y));
}

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
            "dApp browser IPC armed (hidden warm shell may start when the browser binary is present)"
        );

        // Dioxus defaults to a standard OS menu (Window / Edit / Help); Vaughan-Tauri wallet has none.
        let desktop_cfg = dioxus_desktop::Config::new()
            .with_menu(Option::<dioxus_desktop::muda::Menu>::None)
            .with_on_window(|window, _dom| {
                apply_vaughan_tauri_default_window_bounds(window.as_ref());
            })
            .with_custom_event_handler(|event, _| {
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
