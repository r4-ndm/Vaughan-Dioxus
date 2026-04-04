//! Vaughan dApp browser library.
//!
//! Task 30.3: IPC client bootstrap (connect + handshake).
//!
//! **Top-level allowlisted `--url`:** when [`resolve_main_webview_url`] accepts the initial URL (same rules as the wallet's `browser.rs`), the main webview uses [`WebviewUrl::External`] instead of `index.html` plus pending navigation.
//!
//! **Spike / probes:** set `VAUGHAN_SPIKE_EXTERNAL=1` to inject the one-shot `spike_ping` script and write `.topnav_spike_*.txt` under this crate — see [`doc/TOPNAV-SPIKE.md`](../doc/TOPNAV-SPIKE.md).
//!
//! **External load init order (topnav-4):** `initialization_script_for_all_frames` is kept for all navigations; Tauri may not guarantee it runs before page scripts on remote URLs. On each main-frame `PageLoadEvent::Finished`, a tiny `eval` runs only if `window.__VAUGHAN_ETH_INJECTED__` is still missing, and embeds the same provider source via `eval(JSON.parse(...))`.
//!
//! **External navigation chrome (topnav-7):** allowlisted top-level loads have no `index.html` toolbar; on desktop we add a **Navigation** window menu (Back / Forward / Reload) with shortcuts. Shell + iframe mode keeps the HTML address bar only.
//!
//! **In-app navigation:** [`navigate_trusted_dapp`] moves the **main** webview without respawning the process. The URL is re-checked in Rust against the same allowlist as `--url`; invoke is gated by the `allow-navigate-trusted-dapp` capability (same `remote.urls` set as IPC).
//!
//! **Wallet warm shell:** With `VAUGHAN_WALLET_SPAWNED=1` and `VAUGHAN_WALLET_WARM_SHELL=1`, the window starts hidden on `index.html`; newline-delimited JSON on stdin (`{"navigate_trusted":"<url>"}`) navigates the main webview, then shows and focuses it. The Dioxus wallet uses this so the first dApp avoids cold process start.

mod ipc;
mod ipc_pool;

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Duration;

use ipc_pool::WalletIpcPool;
use serde::Deserialize;
use tauri::webview::PageLoadEvent;
use tauri::Manager;
use tauri::Url;
use tauri::WebviewUrl;

use vaughan_ipc_types::{IpcRequest, IpcResponse};

/// Cap wallet control lines so a bug or broken pipe cannot grow unbounded in memory.
const MAX_WALLET_STDIN_LINE_BYTES: usize = 8192;

#[derive(Debug, Deserialize)]
struct WalletStdinNavigate {
    navigate_trusted: Option<String>,
}

/// Read until `\n`, EOF, or `max` content bytes (newline not counted). On overflow, discards through the
/// next `\n` and returns `Some(Err(()))`. `None` means EOF before any line byte; `Some(Ok(bytes))` is one
/// logical line (without `\n`), including an empty slice for a blank line.
fn read_wallet_stdin_framed_line(
    reader: &mut impl std::io::BufRead,
    max: usize,
) -> std::io::Result<Option<Result<Vec<u8>, ()>>> {
    let mut buf = Vec::new();
    let mut one = [0u8; 1];
    loop {
        match reader.read(&mut one)? {
            0 => {
                return if buf.is_empty() {
                    Ok(None)
                } else {
                    Ok(Some(Ok(buf)))
                };
            }
            _ => {
                if one[0] == b'\n' {
                    return Ok(Some(Ok(buf)));
                }
                if buf.len() >= max {
                    let mut cur = one[0];
                    while cur != b'\n' {
                        match reader.read(&mut one)? {
                            0 => return Ok(Some(Err(()))),
                            _ => cur = one[0],
                        }
                    }
                    return Ok(Some(Err(())));
                }
                buf.push(one[0]);
            }
        }
    }
}

/// Shared by [`navigate_trusted_dapp`] and wallet stdin control: allowlisted URL, navigate main, show + focus.
fn navigate_main_trusted_app(app: &tauri::AppHandle, url: String) -> Result<(), String> {
    let u = parse_allowlisted_navigation_url(&url)?;
    let w = app
        .get_webview_window("main")
        .ok_or_else(|| "main webview not found".to_string())?;
    w.navigate(u).map_err(|e| e.to_string())?;
    let _ = w.show();
    let _ = w.set_focus();
    Ok(())
}

#[derive(Debug, Clone)]
struct CliConfig {
    ipc_endpoint: String,
    token: String,
    initial_url: Option<String>,
}

static IPC_REQ_ID: AtomicU64 = AtomicU64::new(1);

/// Polls for `invoke` (same resolution as `provider_inject.js`) and calls `spike_ping` once (topnav-1).
const SPIKE_TOPNAV_SCRIPT: &str = r#"
(function(){
  var DONE = '__VAUGHAN_TOPNAV_SPIKE_PING_SENT__';
  if (window[DONE]) return;
  function getInvoke() {
    if (
      window.__TAURI__ &&
      window.__TAURI__.core &&
      typeof window.__TAURI__.core.invoke === 'function'
    ) {
      return window.__TAURI__.core.invoke.bind(window.__TAURI__.core);
    }
    if (window.__TAURI__ && typeof window.__TAURI__.invoke === 'function') {
      return window.__TAURI__.invoke.bind(window.__TAURI__);
    }
    return null;
  }
  function ping() {
    if (window[DONE]) return;
    try {
      var c = getInvoke();
      if (!c) return;
      window[DONE] = true;
      var r = c('spike_ping');
      if (r && typeof r.then === 'function') {
        r.catch(function (e) { console.error('TOPNAV_SPIKE spike_ping', e); });
      }
    } catch (e) {
      console.error('TOPNAV_SPIKE', e);
    }
  }
  var n = 0;
  var iv = setInterval(function () {
    n++;
    if (window[DONE]) {
      clearInterval(iv);
      return;
    }
    if (getInvoke()) {
      clearInterval(iv);
      ping();
    } else if (n > 600) {
      clearInterval(iv);
    }
  }, 100);
})();
"#;

fn parse_args() -> Option<CliConfig> {
    let mut ipc = None::<String>;
    let mut token = None::<String>;
    let mut initial_url = None::<String>;

    let mut it = std::env::args().skip(1);
    while let Some(arg) = it.next() {
        match arg.as_str() {
            "--ipc" | "--ipc-endpoint" => ipc = it.next(),
            "--token" => token = it.next(),
            "--url" => initial_url = it.next(),
            _ => {}
        }
    }

    Some(CliConfig {
        ipc_endpoint: ipc?,
        token: token?,
        initial_url,
    })
}

/// Host suffixes allowed for **https** (`host == suffix` or a subdomain of `suffix`).
/// Keep in sync with `Vaughan-Dioxus/src/browser.rs` `ALLOWED_HOST_SUFFIXES` and `provider_inject.js`.
const ALLOWED_HOST_SUFFIXES: &[&str] = &[
    "uniswap.org",
    "uniswap.com",
    "sushi.com",
    "pancakeswap.finance",
    "curve.fi",
    "aave.com",
    "compound.finance",
    "1inch.com",
    "opensea.io",
    "stargate.finance",
    "v4.testnet.pulsechain.com",
    "pulsex.com",
    "piteas.io",
    "gopulse.com",
    "internetmoney.io",
    "provex.com",
    "libertyswap.finance",
    "0xcurv.win",
    "pump.tires",
    "9mm.pro",
    "9inch.io",
    "hyperliquid.xyz",
    "asterdex.com",
];

fn hostname_is_whitelisted(host: &str) -> bool {
    let h = host.trim().trim_end_matches('.').to_lowercase();
    if matches!(h.as_str(), "localhost" | "127.0.0.1") {
        return true;
    }
    for suffix in ALLOWED_HOST_SUFFIXES {
        if h == *suffix || h.ends_with(&format!(".{suffix}")) {
            return true;
        }
    }
    false
}

/// Parse and validate a URL for **top-level** navigation (parity with wallet
/// `validate_whitelisted_dapp_url`): **https** + allowlisted host, or **http** + localhost / 127.0.0.1 only.
fn parse_allowlisted_navigation_url(raw: &str) -> Result<Url, String> {
    let t = raw.trim();
    if t.is_empty() {
        return Err("URL is empty".to_string());
    }
    let u = Url::parse(t).map_err(|e| format!("invalid URL: {e}"))?;
    let Some(host) = u.host_str() else {
        return Err("URL must include a host".to_string());
    };
    let h = host.trim().to_lowercase();
    let allowed = match u.scheme() {
        "https" => hostname_is_whitelisted(host),
        "http" => h == "localhost" || h == "127.0.0.1",
        _ => false,
    };
    if allowed {
        Ok(u)
    } else {
        Err("URL is not allowlisted for top-level navigation".to_string())
    }
}

/// If `--url` is allowlisted (parity with wallet `validate_whitelisted_dapp_url`), load it as
/// [`WebviewUrl::External`]; otherwise use the shell (`index.html`) and optional pending navigation.
fn resolve_main_webview_url(initial: Option<&String>) -> (WebviewUrl, bool) {
    let Some(raw) = initial else {
        return (WebviewUrl::App("index.html".into()), false);
    };
    let t = raw.trim();
    if t.is_empty() {
        return (WebviewUrl::App("index.html".into()), false);
    }
    match parse_allowlisted_navigation_url(raw) {
        Ok(u) => (WebviewUrl::External(u), true),
        Err(_) => (WebviewUrl::App("index.html".into()), false),
    }
}

pub fn run() {
    let cli = parse_args();

    let ipc_pool = cli.as_ref().map(|c| {
        WalletIpcPool::new(
            c.ipc_endpoint.clone(),
            c.token.clone(),
            Duration::from_secs(3),
        )
    });
    let ipc_pool_for_warm = ipc_pool.clone();

    let navigate_after_load = cli
        .as_ref()
        .and_then(|c| c.initial_url.clone())
        .filter(|u| !u.trim().is_empty());

    let (webview_url, external_top_level) = resolve_main_webview_url(navigate_after_load.as_ref());

    let run_topnav_spike = std::env::var("VAUGHAN_SPIKE_EXTERNAL")
        .map(|v| v == "1")
        .unwrap_or(false);

    let wallet_spawned = std::env::var("VAUGHAN_WALLET_SPAWNED")
        .map(|v| v == "1")
        .unwrap_or(false);
    let wallet_warm_shell = std::env::var("VAUGHAN_WALLET_WARM_SHELL")
        .map(|v| v == "1")
        .unwrap_or(false);
    let use_trusted_chrome = external_top_level || wallet_spawned;

    eprintln!(
        "Vaughan dApp browser: starting (top_level_external={external_top_level}, wallet_spawned={wallet_spawned}, warm_shell={wallet_warm_shell})"
    );

    // File-based confirmation for TOPNAV spike (stderr may not flush on SIGTERM from `timeout`).
    if run_topnav_spike {
        let _ = std::fs::write(
            concat!(env!("CARGO_MANIFEST_DIR"), "/.topnav_spike_start.txt"),
            format!(
                "external_top_level={external_top_level}\nurl_mode={}\n",
                if external_top_level {
                    "WebviewUrl::External"
                } else {
                    "WebviewUrl::App(index.html)"
                }
            ),
        );
    }

    // Shell (`index.html`) only: allowlisted `--url` uses `WebviewUrl::External` (no pending, no iframe shell).
    let pending_js = if external_top_level {
        None
    } else {
        navigate_after_load.as_ref().map(|url| {
            format!(
                "window.__VAUGHAN_PENDING_INITIAL_URL={};",
                serde_json::to_string(url).unwrap_or_else(|_| "\"\"".into())
            )
        })
    };

    tauri::Builder::default()
        .manage(ipc_pool)
        .invoke_handler(tauri::generate_handler![
            ipc_request,
            spike_ping,
            navigate_trusted_dapp
        ])
        .setup(move |app| {
            const PROVIDER_INIT: &str = include_str!("../provider_inject.js");

            let provider_fallback_eval: Option<Arc<String>> = if use_trusted_chrome {
                let quoted = serde_json::to_string(PROVIDER_INIT).expect("serialize provider script");
                Some(Arc::new(format!(
                    "(function(){{if(window.__VAUGHAN_ETH_INJECTED__)return;try{{eval({quoted});}}catch(e){{console.error('[Vaughan] provider fallback eval failed',e);}}}})();"
                )))
            } else {
                None
            };

            let mut wv = tauri::WebviewWindowBuilder::new(app, "main", webview_url)
                .title("Vaughan - dApp Browser")
                .inner_size(1200.0, 800.0)
                .resizable(true)
                .initialization_script_for_all_frames(PROVIDER_INIT);

            if wallet_warm_shell {
                wv = wv.visible(false);
            }

            if let Some(fallback_js) = provider_fallback_eval {
                wv = wv.on_page_load(move |window, payload| {
                    if payload.event() != PageLoadEvent::Finished {
                        return;
                    }
                    match payload.url().scheme() {
                        "about" | "data" | "blob" => return,
                        _ => {}
                    }
                    let _ = window.eval(fallback_js.as_str());
                });
            }

            if external_top_level && run_topnav_spike {
                wv = wv.initialization_script(SPIKE_TOPNAV_SCRIPT);
            }

            if let Some(script) = pending_js.as_ref() {
                wv = wv.initialization_script(script);
            }

            #[cfg(desktop)]
            if use_trusted_chrome {
                use tauri::menu::{Menu, MenuItemBuilder, Submenu};

                let reload_accel = if cfg!(target_os = "macos") {
                    "Super+R"
                } else {
                    "Control+R"
                };

                let back = MenuItemBuilder::with_id("vaughan/nav/back", "Back")
                    .accelerator("Alt+ArrowLeft")
                    .build(app)
                    .map_err(|e| -> Box<dyn std::error::Error> { e.into() })?;
                let forward = MenuItemBuilder::with_id("vaughan/nav/forward", "Forward")
                    .accelerator("Alt+ArrowRight")
                    .build(app)
                    .map_err(|e| -> Box<dyn std::error::Error> { e.into() })?;
                let reload = MenuItemBuilder::with_id("vaughan/nav/reload", "Reload")
                    .accelerator(reload_accel)
                    .build(app)
                    .map_err(|e| -> Box<dyn std::error::Error> { e.into() })?;

                let submenu = Submenu::with_items(
                    app,
                    "Navigation",
                    true,
                    &[&back, &forward, &reload],
                )
                .map_err(|e| -> Box<dyn std::error::Error> { e.into() })?;
                let menu = Menu::with_items(app, &[&submenu])
                    .map_err(|e| -> Box<dyn std::error::Error> { e.into() })?;

                wv = wv.menu(menu).on_menu_event(|window, event| {
                    let Some(w) = window.get_webview_window("main") else {
                        return;
                    };
                    let _ = if event.id() == "vaughan/nav/back" {
                        w.eval("window.history.back();")
                    } else if event.id() == "vaughan/nav/forward" {
                        w.eval("window.history.forward();")
                    } else if event.id() == "vaughan/nav/reload" {
                        w.reload()
                    } else {
                        Ok(())
                    };
                });
            }

            wv.build()
                .map_err(|e| -> Box<dyn std::error::Error> { e.into() })?;

            if wallet_spawned {
                let ctl = app.handle().clone();
                if let Err(e) = std::thread::Builder::new()
                    .name("vaughan-wallet-stdin".into())
                    .spawn(move || {
                        use std::io::BufReader;
                        let stdin = std::io::stdin();
                        let mut reader = BufReader::new(stdin.lock());
                        loop {
                            match read_wallet_stdin_framed_line(
                                &mut reader,
                                MAX_WALLET_STDIN_LINE_BYTES,
                            ) {
                                Ok(None) => break,
                                Ok(Some(Err(()))) => {
                                    tracing::warn!(
                                        target: "vaughan_ipc_browser",
                                        max = MAX_WALLET_STDIN_LINE_BYTES,
                                        "wallet stdin line exceeded cap; skipped"
                                    );
                                    continue;
                                }
                                Ok(Some(Ok(buf))) => {
                                    let raw = buf.strip_suffix(b"\r").unwrap_or(buf.as_slice());
                                    let t = match std::str::from_utf8(raw) {
                                        Ok(s) => s.trim(),
                                        Err(_) => {
                                            tracing::warn!(
                                                target: "vaughan_ipc_browser",
                                                "wallet stdin control line is not valid UTF-8"
                                            );
                                            continue;
                                        }
                                    };
                                    if t.is_empty() {
                                        continue;
                                    }
                                    let Ok(cmd) = serde_json::from_str::<WalletStdinNavigate>(t) else {
                                        tracing::warn!(
                                            target: "vaughan_ipc_browser",
                                            line = %t,
                                            "ignored malformed wallet stdin control line"
                                        );
                                        continue;
                                    };
                                    let Some(url) = cmd
                                        .navigate_trusted
                                        .filter(|s| !s.trim().is_empty())
                                    else {
                                        continue;
                                    };
                                    let app_for_dispatch = ctl.clone();
                                    let app_in_closure = ctl.clone();
                                    if let Err(e) = app_for_dispatch.run_on_main_thread(move || {
                                        if let Err(e) =
                                            navigate_main_trusted_app(&app_in_closure, url)
                                        {
                                            tracing::warn!(
                                                target: "vaughan_ipc_browser",
                                                err = %e,
                                                "stdin navigate_trusted failed"
                                            );
                                        }
                                    }) {
                                        tracing::warn!(
                                            target: "vaughan_ipc_browser",
                                            err = %e,
                                            "run_on_main_thread failed for stdin navigate"
                                        );
                                    }
                                }
                                Err(e) => {
                                    tracing::debug!(
                                        target: "vaughan_ipc_browser",
                                        err = %e,
                                        "wallet stdin read ended"
                                    );
                                    break;
                                }
                            }
                        }
                    })
                {
                    tracing::warn!(
                        target: "vaughan_ipc_browser",
                        err = %e,
                        "failed to spawn wallet stdin control thread"
                    );
                }
            }

            if let Some(pool) = ipc_pool_for_warm.as_ref() {
                let p = Arc::clone(pool);
                tauri::async_runtime::spawn(async move {
                    p.warm_connections().await;
                });
            }

            if external_top_level {
                eprintln!(
                    "Vaughan dApp browser: main document is allowlisted external URL (no index.html shell)."
                );
            }

            if cli.is_none() {
                eprintln!("IPC config missing. Start with --ipc <endpoint> --token <token>");
            }
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error running tauri application");
}

/// Navigates the **main** webview to `url` only if it passes [`parse_allowlisted_navigation_url`].
/// Use for in-app navigation (shell or external) without respawning; Rust rejects non-listed URLs.
#[tauri::command]
fn navigate_trusted_dapp(app: tauri::AppHandle, url: String) -> Result<(), String> {
    navigate_main_trusted_app(&app, url)
}

/// TOPNAV spike: invoked from JS when `VAUGHAN_SPIKE_EXTERNAL=1` and the spike init script is injected.
#[tauri::command]
fn spike_ping() {
    eprintln!("TOPNAV_SPIKE: spike_ping invoked from JS — top-level external invoke path works");
    let _ = std::fs::write(
        concat!(env!("CARGO_MANIFEST_DIR"), "/.topnav_spike_invoke.txt"),
        "spike_ping_ok\n",
    );
    tracing::info!(
        target: "vaughan_ipc_browser",
        "TOPNAV_SPIKE: spike_ping invoked from JS — top-level external invoke path works"
    );
}

/// Forwards to the wallet over a pooled local socket (K persistent connections by default).
#[tauri::command]
async fn ipc_request(
    pool: tauri::State<'_, Option<Arc<WalletIpcPool>>>,
    request: IpcRequest,
) -> Result<IpcResponse, String> {
    let Some(pool) = pool.as_ref() else {
        return Err("Wallet IPC not configured".to_string());
    };

    let op_timeout = Duration::from_secs(10);
    let id = IPC_REQ_ID.fetch_add(1, Ordering::Relaxed);
    let env = pool.request(id, request, op_timeout).await?;

    Ok(env.body)
}

#[cfg(test)]
mod topnav_url_tests {
    use super::*;

    fn resolve(s: &str) -> (WebviewUrl, bool) {
        resolve_main_webview_url(Some(&s.to_string()))
    }

    #[test]
    fn allowlisted_uniswap_is_external_top_level() {
        let (wv, ext) = resolve("https://app.uniswap.org/");
        assert!(ext);
        assert!(matches!(wv, WebviewUrl::External(_)));
    }

    #[test]
    fn allowlisted_aave_is_external_top_level() {
        let (wv, ext) = resolve("https://app.aave.com/");
        assert!(ext);
        assert!(matches!(wv, WebviewUrl::External(_)));
    }

    #[test]
    fn allowlisted_sushi_is_external_top_level() {
        let (wv, ext) = resolve("https://www.sushi.com/swap");
        assert!(ext);
        assert!(matches!(wv, WebviewUrl::External(_)));
    }

    #[test]
    fn non_allowlisted_https_uses_shell() {
        let (wv, ext) = resolve("https://evil.example/phish");
        assert!(!ext);
        assert!(matches!(wv, WebviewUrl::App(_)));
    }

    #[test]
    fn loopback_http_is_external_top_level() {
        let (wv, ext) = resolve("http://127.0.0.1:3000/");
        assert!(ext);
        assert!(matches!(wv, WebviewUrl::External(_)));
    }

    #[test]
    fn no_url_uses_shell() {
        let (wv, ext) = resolve_main_webview_url(None);
        assert!(!ext);
        assert!(matches!(wv, WebviewUrl::App(_)));
    }

    #[test]
    fn parse_rejects_empty_and_evil() {
        assert!(parse_allowlisted_navigation_url("  ").is_err());
        assert!(parse_allowlisted_navigation_url("https://evil.example/phish").is_err());
    }

    #[test]
    fn parse_accepts_allowlisted_https_and_loopback_http() {
        assert!(parse_allowlisted_navigation_url("https://app.uniswap.org/").is_ok());
        assert!(parse_allowlisted_navigation_url("http://127.0.0.1:3000/").is_ok());
    }
}
