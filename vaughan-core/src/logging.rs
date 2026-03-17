//! Structured logging setup for Vaughan wallet and dApp browser.
//!
//! Initialize once per process at application startup via `init_logging()`.
//! Filtering is controlled by the `RUST_LOG` environment variable (e.g. `RUST_LOG=debug,vaughan_core=trace`).

use tracing::info;
use tracing_subscriber::fmt;
use tracing_subscriber::EnvFilter;

/// Initialize the global tracing subscriber.
///
/// Uses `tracing_subscriber` with:
/// - Environment-based level filter from `RUST_LOG` (default: `info,vaughan_core=debug`)
/// - Target (module path) in log output
/// - No file/line in default config to keep output readable
///
/// # Panics
///
/// May panic if called more than once in the same process (global subscriber already set).
pub fn init_logging() {
    let filter = EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| EnvFilter::new("info,vaughan_core=debug"));

    fmt()
        .with_env_filter(filter)
        .with_target(true)
        .with_thread_ids(false)
        .with_file(false)
        .with_line_number(false)
        .init();

    info!(target: "vaughan_core::logging", "Logging initialized");
}
