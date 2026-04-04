//! dApp browser subprocess: separate OS process with IPC to the wallet (see `tasks.md` Task 33).
//!
//! ## CLI contract (topnav-6)
//! Every spawn runs `vaughan-tauri-browser --ipc <endpoint> --token <token>` with piped stdin for control.
//! The wallet sets `VAUGHAN_WALLET_SPAWNED=1` on every spawn. A **warm** start (no `--url`) also sets
//! `VAUGHAN_WALLET_WARM_SHELL=1` so the window stays hidden until the first trusted dApp is opened.
//! Opening a dApp sends a JSON line on stdin: `{"navigate_trusted":"<url>"}` (same allowlist as `--url`).
//! If the warm process is gone or the pipe fails, the wallet falls back to a full respawn with `--url`.
//! The monitor thread respawns after an unexpected child exit using [`BrowserInner::last_url`]; a **successful**
//! exit clears `last_url` so we do not relaunch. Set `VAUGHAN_NO_WARM_DAPP_BROWSER=1` on the wallet process to skip
//! warm spawn at startup.

use std::io::Write;
use std::path::PathBuf;
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, OnceLock};
use std::thread;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use url::Url;

pub use vaughan_trusted_hosts::hostname_is_whitelisted;

use crate::services::AppServices;
use crate::wallet_ipc::WalletIpcServer;

/// IPC endpoint + token for spawning the dApp browser (also used for lazy launch if the binary was missing at startup).
struct DappBoot {
    endpoint: String,
    token: String,
}

static WALLET_DAPP_BOOT: OnceLock<DappBoot> = OnceLock::new();

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
        "Local PulseX instance — start the server first, then open here.",
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

static BROWSER_STATE: OnceLock<Arc<Mutex<BrowserInner>>> = OnceLock::new();

struct BrowserInner {
    child: Option<Child>,
    /// Write end of the control pipe (`{"navigate_trusted": "..."}` lines); closed when the child is replaced.
    control_stdin: Option<ChildStdin>,
    /// Last URL opened from the wallet; kept after a crash so the monitor can respawn.
    last_url: Option<String>,
    endpoint: String,
    token: String,
    bin: PathBuf,
}

impl BrowserInner {
    fn child_is_alive(&mut self) -> bool {
        let Some(c) = &mut self.child else {
            return false;
        };
        match c.try_wait() {
            Ok(None) => true,
            Ok(Some(_)) | Err(_) => false,
        }
    }

    /// Sends a navigate command to a running warm browser. Fails if the process exited or the pipe broke.
    fn try_send_navigate_trusted(&mut self, url: &str) -> Result<(), String> {
        if !self.child_is_alive() {
            return Err("dApp browser process is not running".to_string());
        }
        let Some(stdin) = self.control_stdin.as_mut() else {
            return Err("dApp browser has no control stdin".to_string());
        };
        let line = serde_json::json!({ "navigate_trusted": url }).to_string();
        writeln!(stdin, "{line}").map_err(|e| e.to_string())?;
        stdin.flush().map_err(|e| e.to_string())?;
        Ok(())
    }

    /// Wallet spawn: piped stdin + env markers. Warm shell (`url` None) starts hidden until first navigate.
    fn spawn(&mut self, url: Option<&str>) -> Result<(), String> {
        self.control_stdin = None;

        if let Some(mut c) = self.child.take() {
            match c.try_wait() {
                Ok(Some(status)) => {
                    if status.success() {
                        self.last_url = None;
                    }
                }
                Ok(None) => {
                    let _ = c.kill();
                    let _ = c.wait();
                }
                Err(_) => {
                    let _ = c.kill();
                    let _ = c.wait();
                }
            }
        }

        let mut cmd = Command::new(&self.bin);
        cmd.env("VAUGHAN_WALLET_SPAWNED", "1")
            .arg("--ipc")
            .arg(&self.endpoint)
            .arg("--token")
            .arg(&self.token)
            .stdin(Stdio::piped())
            .stdout(Stdio::null())
            .stderr(Stdio::inherit());
        if url.is_none() {
            cmd.env("VAUGHAN_WALLET_WARM_SHELL", "1");
        }
        if let Some(u) = url {
            cmd.arg("--url").arg(u);
            self.last_url = Some(u.to_string());
        }

        let mut child = cmd.spawn().map_err(|e| e.to_string())?;
        self.control_stdin = child.stdin.take();
        self.child = Some(child);
        Ok(())
    }
}

/// Validates URL scheme and host against the Tauri-parity trusted list. Returns normalized URL string.
pub fn validate_whitelisted_dapp_url(url_str: &str) -> Result<String, String> {
    let u = Url::parse(url_str.trim()).map_err(|e| e.to_string())?;
    let host = u.host_str().ok_or("URL missing host")?;
    let h = host.trim().to_lowercase();

    match u.scheme() {
        "https" => {
            if !hostname_is_whitelisted(host) {
                return Err("That site is not on the trusted dApp list".into());
            }
        }
        "http" => {
            if h != "localhost" && h != "127.0.0.1" {
                return Err(
                    "Only https:// dApps are allowed (except http://localhost and http://127.0.0.1)."
                        .into(),
                );
            }
        }
        _ => return Err("Invalid URL scheme for a trusted dApp.".into()),
    }
    Ok(u.to_string())
}

/// Restarts the dApp browser pointed at a whitelisted URL (same IPC session).
/// If the browser was not launched at startup (missing binary), spawns it on first use.
pub fn open_trusted_dapp_url(url_str: &str) -> Result<(), String> {
    let full = validate_whitelisted_dapp_url(url_str)?;
    let boot = WALLET_DAPP_BOOT
        .get()
        .ok_or("Wallet IPC is not running; restart the wallet.")?;

    let bin = resolve_browser_executable().ok_or_else(|| {
        "dApp browser not found. From the repo root run:\n  cargo build -p vaughan-tauri-browser\n\
         (or build the whole workspace), then click again."
            .to_string()
    })?;

    if BROWSER_STATE.get().is_none() {
        let init = Arc::new(Mutex::new(BrowserInner {
            child: None,
            control_stdin: None,
            last_url: None,
            endpoint: boot.endpoint.clone(),
            token: boot.token.clone(),
            bin: bin.clone(),
        }));
        let _ = BROWSER_STATE.set(init);
    }

    let state = BROWSER_STATE
        .get()
        .ok_or_else(|| "dApp browser state unavailable".to_string())?;
    let mut inner = state.lock().map_err(|e| e.to_string())?;
    inner.bin = bin;
    if inner.child_is_alive() {
        if inner.try_send_navigate_trusted(&full).is_ok() {
            inner.last_url = Some(full);
            return Ok(());
        }
        tracing::warn!(target: "vaughan_browser", "dApp browser control pipe failed; respawning with --url");
    }
    inner.spawn(Some(&full))?;
    Ok(())
}

/// Starts wallet IPC for the dApp browser and optionally **warms** a hidden browser process (shell only)
/// so the first dApp open avoids process + WebKit cold start.
/// On drop, stops the health monitor and terminates any running browser child.
pub struct BrowserProcessGuard {
    /// Keeps the IPC accept loop alive for the dApp browser.
    #[allow(dead_code)]
    ipc_server: Option<WalletIpcServer>,
    browser_monitor_stop: Arc<AtomicBool>,
    browser_monitor: Option<thread::JoinHandle<()>>,
}

impl BrowserProcessGuard {
    pub fn launch_if_available(services: AppServices) -> Self {
        let endpoint = ipc_endpoint();
        let token = ipc_token();
        let ipc_server = match WalletIpcServer::start(endpoint.clone(), token.clone(), services) {
            Ok(server) => {
                let _ = WALLET_DAPP_BOOT.set(DappBoot {
                    endpoint: endpoint.clone(),
                    token: token.clone(),
                });
                Some(server)
            }
            Err(err) => {
                eprintln!("Failed to start wallet IPC server: {err}");
                None
            }
        };

        let browser_bin = resolve_browser_executable();
        if browser_bin.is_none() {
            eprintln!(
                "dApp browser executable not found (expected next to the wallet or under target/debug). \
                 Build it with: cargo build -p vaughan-tauri-browser"
            );
        }

        if ipc_server.is_some() {
            let skip_warm = std::env::var("VAUGHAN_NO_WARM_DAPP_BROWSER")
                .map(|v| v == "1")
                .unwrap_or(false);
            if !skip_warm {
                if let Some(bin) = browser_bin {
                    if BROWSER_STATE.get().is_none() {
                        let _ = BROWSER_STATE.set(Arc::new(Mutex::new(BrowserInner {
                            child: None,
                            control_stdin: None,
                            last_url: None,
                            endpoint: endpoint.clone(),
                            token: token.clone(),
                            bin: bin.clone(),
                        })));
                    }
                    if let Some(state) = BROWSER_STATE.get() {
                        if let Ok(mut inner) = state.lock() {
                            inner.bin = bin;
                            if inner.child.is_none() {
                                match inner.spawn(None) {
                                    Ok(()) => tracing::info!(
                                        target: "vaughan_browser",
                                        "dApp browser warm process started (hidden until first trusted dApp)"
                                    ),
                                    Err(e) => eprintln!("dApp browser warm spawn failed: {e}"),
                                }
                            }
                        }
                    }
                }
            }
        }

        let browser_monitor_stop = Arc::new(AtomicBool::new(false));
        let stop_for_monitor = Arc::clone(&browser_monitor_stop);
        let browser_monitor = thread::Builder::new()
            .name("vaughan-browser-monitor".into())
            .spawn(move || {
                while !stop_for_monitor.load(Ordering::SeqCst) {
                    thread::sleep(Duration::from_secs(2));
                    if stop_for_monitor.load(Ordering::SeqCst) {
                        break;
                    }
                    let Some(state) = BROWSER_STATE.get() else {
                        continue;
                    };
                    let Ok(mut inner) = state.lock() else {
                        continue;
                    };
                    if let Some(mut c) = inner.child.take() {
                        match c.try_wait() {
                            Ok(Some(status)) => {
                                inner.control_stdin = None;
                                if status.success() {
                                    inner.last_url = None;
                                }
                            }
                            Ok(None) => {
                                inner.child = Some(c);
                            }
                            Err(_) => {}
                        }
                    }
                    if inner.child.is_none() {
                        if let Some(url) = inner.last_url.clone() {
                            if let Some(p) = resolve_browser_executable() {
                                inner.bin = p;
                            }
                            if inner.bin.exists() && inner.spawn(Some(url.as_str())).is_ok() {
                                tracing::info!(target: "vaughan_browser", "restarted dApp browser after process exit");
                            }
                        }
                    }
                }
            })
            .ok();

        Self {
            ipc_server,
            browser_monitor_stop,
            browser_monitor,
        }
    }
}

impl Drop for BrowserProcessGuard {
    fn drop(&mut self) {
        self.browser_monitor_stop.store(true, Ordering::SeqCst);
        if let Some(h) = self.browser_monitor.take() {
            let _ = h.join();
        }
        if let Some(state) = BROWSER_STATE.get() {
            if let Ok(mut inner) = state.lock() {
                inner.control_stdin = None;
                if let Some(mut c) = inner.child.take() {
                    let _ = c.kill();
                    let _ = c.wait();
                }
            }
        }
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

#[cfg(unix)]
fn ipc_endpoint() -> String {
    let path = std::env::temp_dir().join(format!("vaughan-wallet-{}.sock", std::process::id()));
    path.to_string_lossy().into_owned()
}

#[cfg(windows)]
fn ipc_endpoint() -> String {
    format!(r"\\.\pipe\vaughan-wallet-{}", std::process::id())
}

fn ipc_token() -> String {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    format!("vaughan-{}-{now}", std::process::id())
}
