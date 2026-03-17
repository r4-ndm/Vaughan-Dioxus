//! Vaughan Wallet — Dioxus entry point (desktop + mobile).

mod app;
mod components;
mod theme;
mod utils;
mod views;

fn main() {
    vaughan_core::logging::init_logging();

    #[cfg(feature = "mobile")]
    {
        dioxus_mobile::launch::launch(app::WalletApp);
        return;
    }

    #[cfg(not(feature = "mobile"))]
    {
        dioxus::launch(app::WalletApp);
    }
}
