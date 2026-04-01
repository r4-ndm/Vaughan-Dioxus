//! dApp browser subprocess: separate OS process with IPC to the wallet (see `tasks.md` Task 33).

use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, OnceLock};
use std::thread;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use url::Url;

use crate::services::AppServices;
use crate::wallet_ipc::WalletIpcServer;

/// IPC endpoint + token for spawning the dApp browser (also used for lazy launch if the binary was missing at startup).
struct DappBoot {
    endpoint: String,
    token: String,
}

static WALLET_DAPP_BOOT: OnceLock<DappBoot> = OnceLock::new();

/// Human label + canonical https URL for trusted dApps (shown in wallet UI).
pub const TRUSTED_DAPP_ENTRIES: &[(&str, &str)] = &[
    ("Uniswap (app)", "https://app.uniswap.org"),
    ("Uniswap (uniswap.com)", "https://uniswap.com"),
];

/// Host suffixes allowed when opening from the wallet (subdomains included).
const ALLOWED_HOST_SUFFIXES: &[&str] = &["uniswap.org", "uniswap.com"];

static BROWSER_STATE: OnceLock<Arc<Mutex<BrowserInner>>> = OnceLock::new();

struct BrowserInner {
    child: Option<Child>,
    /// Last URL opened from the wallet; kept after a crash so the monitor can respawn.
    last_url: Option<String>,
    endpoint: String,
    token: String,
    bin: PathBuf,
}

impl BrowserInner {
    fn spawn(&mut self, url: Option<&str>) -> Result<(), String> {
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
        cmd.arg("--ipc")
            .arg(&self.endpoint)
            .arg("--token")
            .arg(&self.token)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::inherit());
        if let Some(u) = url {
            cmd.arg("--url").arg(u);
            self.last_url = Some(u.to_string());
        }

        self.child = Some(cmd.spawn().map_err(|e| e.to_string())?);
        Ok(())
    }
}

pub fn hostname_is_whitelisted(host: &str) -> bool {
    let h = host.trim().trim_end_matches('.').to_lowercase();
    for suffix in ALLOWED_HOST_SUFFIXES {
        if h == *suffix || h.ends_with(&format!(".{suffix}")) {
            return true;
        }
    }
    false
}

/// Validates `https` URL and host against the trusted list. Returns normalized URL string.
pub fn validate_whitelisted_https_url(url_str: &str) -> Result<String, String> {
    let u = Url::parse(url_str.trim()).map_err(|e| e.to_string())?;
    if u.scheme() != "https" {
        return Err("Only https:// URLs are allowed for trusted dApps".into());
    }
    let host = u.host_str().ok_or("URL missing host")?;
    if !hostname_is_whitelisted(host) {
        return Err("That site is not on the trusted dApp list".into());
    }
    Ok(u.to_string())
}

/// Restarts the dApp browser pointed at a whitelisted URL (same IPC session).
/// If the browser was not launched at startup (missing binary), spawns it on first use.
pub fn open_trusted_dapp_url(url_str: &str) -> Result<(), String> {
    let full = validate_whitelisted_https_url(url_str)?;
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
    inner.spawn(Some(&full))?;
    Ok(())
}

/// Starts wallet IPC for the dApp browser. The browser process is **not** spawned here;
/// it launches on first trusted dApp open from the DApps view (`open_trusted_dapp_url`).
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

        if resolve_browser_executable().is_none() {
            eprintln!(
                "dApp browser executable not found (expected next to the wallet or under target/debug). \
                 Build it with: cargo build -p vaughan-tauri-browser"
            );
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
