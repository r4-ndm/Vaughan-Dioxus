//! Vaughan wallet core: chain-agnostic logic, chain adapters, security, monitoring.
//!
//! Used by the Dioxus wallet app and (via IPC) by the Tauri dApp browser.

pub mod chains;
pub mod core;
pub mod error;
pub mod native_dapps;
pub mod logging;
pub mod models;
pub mod monitoring;
pub mod security;

#[cfg(test)]
mod test_only;

pub use chains::{ChainAdapter, ChainType};
pub use error::WalletError;
pub use models::*;
