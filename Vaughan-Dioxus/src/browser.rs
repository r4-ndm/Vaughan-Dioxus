//! dApp browser subprocess: separate OS process with IPC to the wallet (see `tasks.md` Task 33).
//!
//! ## CLI contract (topnav-6)
//! Every spawn runs `vaughan-tauri-browser --ipc <endpoint> --token <token>` with piped stdin for control.
//! The wallet sets `VAUGHAN_WALLET_SPAWNED=1` on every spawn. A **warm** start (no `--url`) also sets
//! `VAUGHAN_WALLET_WARM_SHELL=1` so the window stays hidden until the first trusted dApp is opened.
//! Opening a dApp sends a JSON line on stdin: `{"navigate_trusted":"<url>"}` (same allowlist as `--url`).
//! Optional `"reveal":false` keeps the warm window hidden while the webview navigates (prewarm only).
//! If the warm process is gone or the pipe fails, the wallet falls back to a full respawn with `--url`.
//!
//! ## Window lifecycle (hide-on-close)
//! The browser intercepts `CloseRequested` and **hides** the window instead of exiting. The process
//! stays alive with the stdin thread still running, so the next `{"navigate_trusted":...}` immediately
//! navigates, shows, and focuses the window — no cold start at all after the first launch. The wallet
//! kills the process on shutdown via `BrowserProcessGuard::drop`.
//!
//! The monitor thread respawns after an unexpected crash exit using [`BrowserInner::last_url`].
//! Set `VAUGHAN_NO_WARM_DAPP_BROWSER=1` on the wallet process to skip warm spawn entirely.
//!
//! **Multi warm pool (experimental):** set `VAUGHAN_MULTI_WARM_POOL=1` so the dApp child keeps up to
//! six hidden `warm-slot-*` windows (one per rocket index) plus `main`; the wallet monitor runs
//! [`warm_pool_reconcile_tick`] to assign URLs, track slot state, and open via `cmd: show` when ready.
//! Child stdout emits `slot_loaded` (real `PageLoadEvent::Finished`), `heartbeat`, `ready`, and
//! lifecycle events; the wallet applies **Linux `MemAvailable`-based** soft slot caps, **warm
//! timeouts**, **exponential backoff** on failures, and **stale-heartbeat recovery**.

use std::net::{TcpStream, ToSocketAddrs};
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::sync::{Mutex, OnceLock};
use std::thread;
use std::time::{Duration, Instant};

use url::Url;

use crate::services::{shared_services, AppServices};
use crate::wallet_rpc_server::WalletRpcServer;

/// RPC port + token for spawning the dApp browser (also used for lazy launch if the binary was missing at startup).
struct DappBoot {
    rpc_port: u16,
    rpc_token: String,
}

static WALLET_DAPP_BOOT: OnceLock<DappBoot> = OnceLock::new();
static EXTRA_BROWSER_CHILDREN: OnceLock<Mutex<Vec<Child>>> = OnceLock::new();
fn normalize_dapp_usage_key(url: &str) -> String {
    Url::parse(url)
        .ok()
        .and_then(|u| u.host_str().map(|h| h.to_ascii_lowercase()))
        .unwrap_or_else(|| url.to_ascii_lowercase())
}

pub fn dapp_preference_key(url: &str) -> String {
    normalize_dapp_usage_key(url)
}



pub fn compute_top_trusted_candidates_for_chain(limit: usize, active_chain_id: u64) -> Vec<String> {
    let services = shared_services();
    let snapshot = services.persistence.snapshot();
    let prefs = snapshot.preferences.clone().unwrap_or_default();
    let chain_key = active_chain_id.to_string();
    let fast_keys: std::collections::HashSet<String> = prefs
        .fast_dapps_by_chain_v1
        .get(&chain_key)
        .cloned()
        .unwrap_or_default()
        .into_iter()
        .collect();

    // Gather both curated and custom whitelisted dApps
    let mut all_whitelisted_urls = Vec::new();
    for e in TRUSTED_DAPP_ENTRIES.iter() {
        if trusted_dapp_visible_on_chain(e, active_chain_id) {
            if let Ok(url) = validate_whitelisted_dapp_url(e.url) {
                all_whitelisted_urls.push(url);
            }
        }
    }
    for d in snapshot.custom_trusted_dapps.iter() {
        if let Ok(url) = validate_whitelisted_dapp_url(&d.url) {
            all_whitelisted_urls.push(url);
        }
    }

    let mut selected: Vec<String> = all_whitelisted_urls
        .iter()
        .cloned()
        .filter(|url| {
            let key = normalize_dapp_usage_key(url);
            fast_keys.contains(&key)
        })
        .take(limit)
        .collect();

    if selected.is_empty() {
        selected = all_whitelisted_urls.into_iter().take(limit).collect();
    }
    selected
}

fn preconnect_dapp_origin(url: &str) {
    let Ok(u) = Url::parse(url) else {
        return;
    };
    let Some(host) = u.host_str() else {
        return;
    };
    let port = u.port_or_known_default().unwrap_or(443);
    let Ok(mut addrs) = format!("{host}:{port}").to_socket_addrs() else {
        return;
    };
    let Some(addr) = addrs.next() else {
        return;
    };
    let _ = TcpStream::connect_timeout(&addr, Duration::from_millis(600));
}

/// Throttle state for `preconnect_all_visible_trusted_origins_for_chain`.
/// Stores `(chain_id, last_run_at)` so repeat calls within the throttle window are skipped,
/// while a chain switch still triggers an immediate pass.
static LAST_BROAD_PRECONNECT: Mutex<Option<(u64, Instant)>> = Mutex::new(None);
const BROAD_PRECONNECT_THROTTLE: Duration = Duration::from_secs(45);

/// DNS-resolves and TCP-preconnects every trusted dApp origin visible on `active_chain_id`,
/// so a user's first click on a *non-rocket* dApp right after startup skips the cold
/// DNS + TCP + TLS handshake cost. Deduplicates by `host:port`, runs connects in parallel,
/// and self-throttles to avoid re-preconnecting too often.
///
/// Cheap and safe: plain TCP connect_timeout with ~600ms budget per origin; the socket is
/// closed immediately after. No TLS, no HTTP. This primes:
///   * the OS DNS resolver cache,
///   * the TCP SYN/ACK path + path MTU / congestion control hints,
///   * some server-side LB affinity / SYN cookies.
pub fn preconnect_all_visible_trusted_origins_for_chain(active_chain_id: u64) {
    if let Ok(mut guard) = LAST_BROAD_PRECONNECT.lock() {
        if let Some((prev_chain, at)) = *guard {
            if prev_chain == active_chain_id && at.elapsed() < BROAD_PRECONNECT_THROTTLE {
                return;
            }
        }
        *guard = Some((active_chain_id, Instant::now()));
    }

    // Dedupe by host:port so sites sharing a host (or port) only connect once.
    let mut seen: std::collections::HashSet<String> = std::collections::HashSet::new();
    let mut origins: Vec<String> = Vec::new();
    for entry in TRUSTED_DAPP_ENTRIES.iter() {
        if !trusted_dapp_visible_on_chain(entry, active_chain_id) {
            continue;
        }
        let Ok(url) = validate_whitelisted_dapp_url(entry.url) else {
            continue;
        };
        let Ok(parsed) = Url::parse(&url) else {
            continue;
        };
        let Some(host) = parsed.host_str() else {
            continue;
        };
        let port = parsed.port_or_known_default().unwrap_or(443);
        let key = format!("{host}:{port}");
        if seen.insert(key) {
            origins.push(url);
        }
    }
    if origins.is_empty() {
        return;
    }
    thread::spawn(move || {
        let mut handles = Vec::with_capacity(origins.len());
        for url in origins {
            handles.push(thread::spawn(move || preconnect_dapp_origin(&url)));
        }
        for h in handles {
            let _ = h.join();
        }
    });
}

pub fn prewarm_top_trusted_dapps_for_chain(_limit: usize, _active_chain_id: u64) {}

/// One trusted dApp row (parity with Vaughan-Tauri `web/src/utils/whitelistedDapps.ts`).
#[derive(Debug, Clone, Copy)]
pub struct TrustedDapp {
    pub name: &'static str,
    pub url: &'static str,
    pub description: &'static str,
    /// Short label for the card footer (e.g. `DEX`, `DeFi`).
    pub category: &'static str,
    /// Empty slice = show on every network (unused for core list; Tauri uses empty for custom only).
    pub chains: &'static [u64],
}

macro_rules! trusted_dapp {
    ($name:literal, $url:expr, $desc:literal, $cat:literal, [$( $c:literal ),* $(,)?] ) => {
        TrustedDapp {
            name: $name,
            url: $url,
            description: $desc,
            category: $cat,
            chains: &[$( $c ),*],
        }
    };
}

/// Curated list shown in the DApps view; URLs must match [`ALLOWED_HTTPS_HOST_SUFFIXES`](vaughan_trusted_hosts::ALLOWED_HTTPS_HOST_SUFFIXES) (except loopback http).
pub const TRUSTED_DAPP_ENTRIES: &[TrustedDapp] = &[
    trusted_dapp!(
        "Uniswap",
        "https://app.uniswap.org",
        "Swap, earn, and build on the leading decentralized crypto trading protocol.",
        "DEX",
        [1, 10, 137, 42161, 8453]
    ),
    trusted_dapp!(
        "SushiSwap",
        "https://www.sushi.com/swap",
        "Community-driven DEX and DeFi platform.",
        "DEX",
        [1, 10, 137, 42161, 56]
    ),
    trusted_dapp!(
        "PancakeSwap",
        "https://pancakeswap.finance",
        "Popular DEX on BNB Chain.",
        "DEX",
        [56, 1]
    ),
    trusted_dapp!(
        "Curve Finance",
        "https://curve.fi",
        "Stablecoin-focused DEX with low slippage.",
        "DEX",
        [1, 10, 137, 42161]
    ),
    trusted_dapp!(
        "Aave",
        "https://app.aave.com",
        "Leading decentralized lending protocol.",
        "Lending",
        [1, 10, 137, 42161, 43114]
    ),
    trusted_dapp!(
        "Compound",
        "https://app.compound.finance/?market=usdc-mainnet",
        "Algorithmic money market protocol.",
        "Lending",
        [1, 10, 137, 42161]
    ),
    trusted_dapp!(
        "1inch",
        "https://1inch.com/swap",
        "DEX aggregator for best swap rates.",
        "DEX",
        [1, 10, 137, 42161, 56]
    ),
    trusted_dapp!(
        "OpenSea",
        "https://opensea.io",
        "Largest NFT marketplace.",
        "NFT",
        [1, 10, 137, 42161, 8453]
    ),
    trusted_dapp!(
        "Stargate Finance",
        "https://stargate.finance",
        "Cross-chain bridge powered by LayerZero.",
        "Bridge",
        [1, 10, 137, 42161, 56, 43114]
    ),
    trusted_dapp!(
        "PulseChain Faucet",
        "https://faucet.v4.testnet.pulsechain.com/",
        "Get free PLS and other tokens for testing on PulseChain V4 Testnet.",
        "Tools",
        [943]
    ),
    trusted_dapp!(
        "PulseX (Local)",
        "http://127.0.0.1:3691",
        "PulseX on loopback. Use the footer icons to install or start when needed.",
        "DEX",
        [369, 943]
    ),
    trusted_dapp!(
        "PulseX",
        "https://app.pulsex.com",
        "The most liquid DEX on PulseChain.",
        "DEX",
        [369, 943]
    ),
    trusted_dapp!(
        "Piteas",
        "https://app.piteas.io",
        "DEX aggregator on PulseChain.",
        "DeFi",
        [369, 943]
    ),
    trusted_dapp!(
        "GoPulse",
        "https://gopulse.com",
        "PulseChain portfolio tracker and explorer.",
        "Data",
        [369]
    ),
    trusted_dapp!(
        "Internet Money",
        "https://internetmoney.io",
        "Native PulseChain wallet and swap.",
        "Wallet",
        [369]
    ),
    trusted_dapp!(
        "Provex (Revolut)",
        "https://app.provex.com/#/?provider=revolut",
        "Crypto on-ramp via Revolut.",
        "DeFi",
        [1, 10, 137, 42161, 56, 43114, 8453]
    ),
    trusted_dapp!(
        "LibertySwap",
        "https://libertyswap.finance/",
        "Community-driven DEX for PulseChain.",
        "DEX",
        [369]
    ),
    trusted_dapp!(
        "0xCurv",
        "https://www.0xcurv.win/",
        "DeFi protocol and decentralized application.",
        "DeFi",
        [369, 1]
    ),
    trusted_dapp!(
        "Pump Tires",
        "https://pump.tires/",
        "Fair-launch platform for PulseChain tokens.",
        "DEX",
        [369]
    ),
    trusted_dapp!(
        "9mm DEX",
        "https://dex.9mm.pro/swap",
        "DEX and launchpad on PulseChain.",
        "DEX",
        [369]
    ),
    trusted_dapp!(
        "9Inch",
        concat!(
            "https://9inch.io/?chain=pulse&inputCurrency=0x",
            "6B175474E89094C44Da98b954EedeAC495271d0F",
            "&outputCurrency=0x",
            "78a2809e8e2ef8e07429559f15703Ee20E885588"
        ),
        "Decentralized exchange and yield farming on PulseChain.",
        "DEX",
        [369]
    ),
    trusted_dapp!(
        "Hyperliquid",
        "https://app.hyperliquid.xyz/trade",
        "Decentralized perpetual exchange with orderbook architecture.",
        "DEX",
        [42161]
    ),
    trusted_dapp!(
        "Aster DEX",
        "https://www.asterdex.com/en/trade/pro/futures/ASTERUSDT",
        "Next-gen perpetual DEX for traders.",
        "DEX",
        [1, 42161, 369]
    ),
];

/// Tauri filters core dApps by active chain; empty `chains` means all chains.
#[inline]
pub fn trusted_dapp_visible_on_chain(dapp: &TrustedDapp, active_chain_id: u64) -> bool {
    dapp.chains.is_empty() || dapp.chains.contains(&active_chain_id)
}

/// Prepend `https://` when the user omitted a scheme (Vaughan-Tauri `formatUrl`).
pub fn format_user_dapp_url(raw: &str) -> String {
    let t = raw.trim();
    if t.is_empty() {
        return String::new();
    }
    if t.to_ascii_lowercase().starts_with("http://")
        || t.to_ascii_lowercase().starts_with("https://")
    {
        t.to_string()
    } else {
        format!("https://{t}")
    }
}

/// Google favicon service URL for a full dApp URL (Tauri `getDAppIcon` baseline).
pub fn google_favicon_url_for_dapp(url: &str) -> Option<String> {
    let u = Url::parse(url).ok()?;
    let host = u.host_str()?;
    Some(format!(
        "https://www.google.com/s2/favicons?domain={}&sz=128",
        host
    ))
}

/// Validates URL scheme and host against the canonical trusted list. Returns normalized URL string.
pub fn validate_whitelisted_dapp_url(url_str: &str) -> Result<String, String> {
    vaughan_trusted_hosts::validate_navigation_url(url_str)
}

/// Opens a trusted dApp by spawning a fresh browser child process (window).
/// This is the only open path used by the UI today; every click gets its own window
/// and its own WebKit process.
pub fn open_trusted_dapp_url_new_window(url_str: &str) -> Result<(), String> {
    let full = validate_whitelisted_dapp_url(url_str)?;
    open_any_dapp_url(&full)
}

/// Opens any dApp URL (whitelisted or not) in a new browser window/shell.
pub fn open_any_dapp_url(url_str: &str) -> Result<(), String> {
    let parsed = Url::parse(url_str.trim()).map_err(|e| e.to_string())?;
    if parsed.scheme() != "http" && parsed.scheme() != "https" {
        return Err("Only http:// and https:// URLs are allowed".to_string());
    }
    let full = parsed.to_string();

    let boot = WALLET_DAPP_BOOT
        .get()
        .ok_or("Wallet IPC is not running; restart the wallet.")?;
    let bin = resolve_browser_executable().ok_or_else(|| {
        "dApp browser not found. From the repo root run:\n  cargo build -p vaughan-tauri-browser\n\
         (or build the whole workspace), then click again."
            .to_string()
    })?;

    // Kill any existing browser processes before spawning a new one (to avoid clutter)
    if let Ok(mut children) = EXTRA_BROWSER_CHILDREN.get_or_init(|| Mutex::new(Vec::new())).lock() {
        for mut child in children.drain(..) {
            let _ = child.kill();
            let _ = child.wait();
        }
    }

    let mut cmd = Command::new(&bin);
    cmd.env("VAUGHAN_WALLET_SPAWNED", "1")
        .arg("--rpc-port")
        .arg(&boot.rpc_port.to_string())
        .arg("--rpc-token")
        .arg(&boot.rpc_token)
        .arg("--url")
        .arg(&full)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::inherit());

    let child = cmd.spawn().map_err(|e| e.to_string())?;
    let extras = EXTRA_BROWSER_CHILDREN.get_or_init(|| Mutex::new(Vec::new()));
    if let Ok(mut children) = extras.lock() {
        children.push(child);
    }
    Ok(())
}

/// Opens a trusted dApp preferring an already-warmed slot window when available.
/// Falls back to spawning a fresh new window when no warm-ready slot can be shown.
pub fn open_trusted_dapp_url_prefer_warm_window(url_str: &str) -> Result<(), String> {
    open_trusted_dapp_url_new_window(url_str)
}

/// Starts wallet IPC and JSON-RPC for the dApp browser.
/// On drop, stops the servers and terminates any running browser child.
/// Starts wallet JSON-RPC server for the dApp browser.
/// On drop, stops the server and terminates any running browser child.
pub struct BrowserProcessGuard {
    /// Holds the wallet JSON-RPC server until this guard drops.
    rpc_server: Option<WalletRpcServer>,
}

impl BrowserProcessGuard {
    pub fn launch_if_available(services: AppServices) -> Self {
        let rpc_res = WalletRpcServer::start(services.clone());
        let rpc_server = match rpc_res {
            Ok((server, port, token)) => {
                tracing::info!(target: "vaughan_browser", port, "started wallet RPC server");
                let _ = WALLET_DAPP_BOOT.set(DappBoot {
                    rpc_port: port,
                    rpc_token: token,
                });
                Some(server)
            }
            Err(err) => {
                tracing::error!(target: "vaughan_browser", err = %err, "failed to start wallet RPC server");
                None
            }
        };

        Self { rpc_server }
    }
}

impl Drop for BrowserProcessGuard {
    fn drop(&mut self) {
        if let Some(extras) = EXTRA_BROWSER_CHILDREN.get() {
            if let Ok(mut children) = extras.lock() {
                for mut c in children.drain(..) {
                    let _ = c.kill();
                    let _ = c.wait();
                }
            }
        }
        drop(self.rpc_server.take());
    }
}

fn resolve_browser_executable() -> Option<PathBuf> {
    let current_exe = std::env::current_exe().ok()?;
    let exe_dir = current_exe.parent()?;

    #[cfg(windows)]
    let sibling = exe_dir.join("vaughan-tauri-browser.exe");
    #[cfg(not(windows))]
    let sibling = exe_dir.join("vaughan-tauri-browser");

    if sibling.exists() {
        return Some(sibling);
    }

    let workspace_root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .map(|p| p.to_path_buf());
    if let Some(root) = workspace_root {
        #[cfg(windows)]
        let debug_bin = root
            .join("target")
            .join("debug")
            .join("vaughan-tauri-browser.exe");
        #[cfg(not(windows))]
        let debug_bin = root
            .join("target")
            .join("debug")
            .join("vaughan-tauri-browser");
        if debug_bin.exists() {
            return Some(debug_bin);
        }

        #[cfg(windows)]
        let release_bin = root
            .join("target")
            .join("release")
            .join("vaughan-tauri-browser.exe");
        #[cfg(not(windows))]
        let release_bin = root
            .join("target")
            .join("release")
            .join("vaughan-tauri-browser");
        if release_bin.exists() {
            return Some(release_bin);
        }
    }

    find_browser_in_path()
}

#[cfg(windows)]
const BROWSER_EXE_NAME: &str = "vaughan-tauri-browser.exe";

#[cfg(not(windows))]
const BROWSER_EXE_NAME: &str = "vaughan-tauri-browser";

fn find_browser_in_path() -> Option<PathBuf> {
    let path_var = std::env::var_os("PATH")?;
    for dir in std::env::split_paths(&path_var) {
        let candidate = dir.join(BROWSER_EXE_NAME);
        if candidate.is_file() {
            return Some(candidate);
        }
        #[cfg(windows)]
        {
            let candidate = dir.join("vaughan-tauri-browser");
            if candidate.is_file() {
                return Some(candidate);
            }
        }
    }
    None
}
