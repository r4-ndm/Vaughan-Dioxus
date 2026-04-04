//! Vaughan dApp browser — Tauri entry point (stub).

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    let _ = tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| {
                "warn,vaughan_ipc_browser=debug"
                    .parse()
                    .expect("static env filter")
            }),
        )
        .try_init();
    vaughan_tauri_browser::run()
}
