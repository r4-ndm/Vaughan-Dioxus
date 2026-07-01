//! Vaughan dApp browser library.
//!
//! Task 30.3: IPC client bootstrap (connect + handshake).
//!
//! **Top-level allowlisted `--url`:** when [`resolve_main_webview_url`] accepts the initial URL (same rules as the wallet's `browser.rs`), the main webview uses [`WebviewUrl::External`] instead of `index.html` plus pending navigation.
//!
//! **External load init order (topnav-4):** `initialization_script_for_all_frames` is kept for all navigations; Tauri may not guarantee it runs before page scripts on remote URLs. On each main-frame `PageLoadEvent::Finished`, a tiny `eval` runs only if `window.__VAUGHAN_ETH_INJECTED__` is still missing, and embeds the same provider source via `eval(JSON.parse(...))`.
//!
//! **External navigation chrome (topnav-7):** allowlisted top-level loads have no `index.html` toolbar; on desktop we add a **Navigation** window menu (Back / Forward / Reload) with shortcuts. Shell + iframe mode keeps the HTML address bar only.
//!
//! **In-app navigation:** [`navigate_trusted_dapp`] moves the **main** webview without respawning the process. The URL is re-checked in Rust against the same allowlist as `--url`; invoke is gated by the `allow-navigate-trusted-dapp` capability (same `remote.urls` set as IPC).
//!
//! **Wallet warm shell:** With `VAUGHAN_WALLET_SPAWNED=1` and `VAUGHAN_WALLET_WARM_SHELL=1`, the window starts hidden on `index.html`; newline-delimited JSON on stdin (`{"navigate_trusted":"<url>"}`) navigates the main webview, then shows and focuses it. Add `"reveal":false` to navigate while keeping the window hidden (used for prewarm). The Dioxus wallet uses this so the first dApp avoids cold process start.
//!
//! **Warm slot pool:** stdin `cmd` messages manage up to six `warm-slot-*` windows. Each slot emits
//! `slot_loaded` on real `PageLoadEvent::Finished` for allowlisted top-level URLs; stdout also carries
//! `heartbeat` every 5s for parent liveness checks.
//!
//! **New-window / `target=_blank`:** On desktop, Wry only wires WebKit's create signal when a
//! [`WebviewWindowBuilder::on_new_window`] handler exists. Without it, many dApp links that open
//! a new window appear to do nothing. We route allowlisted URLs into the **main** webview instead.
//! Serializes those navigations so rapid `window.open` bursts do not stack `navigate` on WebKit.
//!
//! **Reload:** Many SPAs capture **Ctrl+R** before the window menu sees it. The Navigation menu uses
//! **F5** (Linux/Windows) or **Super+R** (macOS) as the reload accelerator, and falls back to
//! `location.reload()` if the native webview reload fails (e.g. after a renderer glitch).

use std::sync::atomic::{AtomicBool, Ordering};



use serde::Deserialize;
use tauri::webview::NewWindowResponse;

use tauri::Manager;
use tauri::Url;
use tauri::WebviewUrl;
use tauri::WebviewWindow;

/// Prevents overlapping `navigate_main_trusted_app` calls from nested `on_new_window` bursts.
static ON_NEWWIN_NAV_BUSY: AtomicBool = AtomicBool::new(false);

struct ClearOnNewwinBusy;

impl Drop for ClearOnNewwinBusy {
    fn drop(&mut self) {
        ON_NEWWIN_NAV_BUSY.store(false, Ordering::Release);
    }
}

fn hard_reload_main_webview(w: &WebviewWindow) {
    if w.reload().is_err() {
        tracing::debug!(
            target: "vaughan_ipc_browser",
            "native webview.reload failed; falling back to location.reload()"
        );
        let _ = w.eval(
            "try{window.location.reload();}catch(e){console.error('[Vaughan] reload fallback',e);}",
        );
    }
}



/// Cap wallet control lines so a bug or broken pipe cannot grow unbounded in memory.
const MAX_WALLET_STDIN_LINE_BYTES: usize = 8192;


fn default_reveal_true() -> bool {
    true
}

#[derive(Debug, Deserialize)]
struct WalletStdinControl {
    navigate_trusted: Option<String>,
    #[serde(default = "default_reveal_true")]
    reveal: bool,
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

/// Shared by [`navigate_trusted_dapp`] and wallet stdin control: allowlisted URL, navigate main.
/// When `reveal` is true, shows and focuses the window (normal dApp open).
fn navigate_main_trusted_url(
    app: &tauri::AppHandle,
    u: Url,
    reveal: bool,
) -> Result<(), String> {
    let w = app
        .get_webview_window("main")
        .ok_or_else(|| "main webview not found".to_string())?;
    if !reveal {
        if let Ok(true) = w.is_visible() {
            return Err("skip hidden prewarm navigate while window is visible".to_string());
        }
    }
    w.navigate(u).map_err(|e| e.to_string())?;
    if reveal {
        let _ = w.show();
        let _ = w.set_focus();
    }
    Ok(())
}

fn navigate_main_trusted_app(
    app: &tauri::AppHandle,
    url: String,
    reveal: bool,
) -> Result<(), String> {
    let u = parse_allowlisted_navigation_url(&url)?;
    navigate_main_trusted_url(app, u, reveal)
}



#[derive(Debug, Clone)]
struct CliConfig {
    rpc_port: Option<u16>,
    rpc_token: Option<String>,
    initial_url: Option<String>,
}

fn parse_args() -> Option<CliConfig> {
    let mut rpc_port = None::<u16>;
    let mut rpc_token = None::<String>;
    let mut initial_url = None::<String>;

    let mut it = std::env::args().skip(1);
    while let Some(arg) = it.next() {
        match arg.as_str() {
            "--rpc-port" => {
                if let Some(s) = it.next() {
                    rpc_port = s.parse().ok();
                }
            }
            "--rpc-token" => rpc_token = it.next(),
            "--url" => initial_url = it.next(),
            _ => {}
        }
    }

    Some(CliConfig {
        rpc_port,
        rpc_token,
        initial_url,
    })
}

/// Parse and validate a URL for **top-level** navigation (parity with wallet
/// `validate_whitelisted_dapp_url`): **https** + allowlisted host, or **http** + localhost / 127.0.0.1 only.
fn parse_allowlisted_navigation_url(raw: &str) -> Result<Url, String> {
    let t = raw.trim();
    if t.is_empty() {
        return Err("URL is empty".to_string());
    }
    vaughan_trusted_hosts::parse_navigation_url(t)
}

fn validate_allowlisted_top_level_url(u: Url) -> Result<Url, String> {
    vaughan_trusted_hosts::validate_navigation_url(u.as_str())?;
    Ok(u)
}

/// Resolve popup/new-window targets that may be relative (`/path`, `?q=...`) to the current main
/// page URL before applying the same allowlist rules.
fn resolve_allowlisted_new_window_target(app: &tauri::AppHandle, raw: &str) -> Result<Url, String> {
    let t = raw.trim();
    if t.is_empty() {
        return Err("URL is empty".to_string());
    }
    if let Ok(abs) = Url::parse(t) {
        return validate_allowlisted_top_level_url(abs);
    }

    let main = app
        .get_webview_window("main")
        .ok_or_else(|| "main webview not found".to_string())?;
    let base = main
        .url()
        .map_err(|e| format!("failed to read main webview URL: {e}"))?;
    let joined = base
        .join(t)
        .map_err(|e| format!("invalid popup target URL: {e}"))?;
    validate_allowlisted_top_level_url(joined)
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
    // Load custom trusted hosts from state.json
    for host in load_custom_hosts_from_state_json() {
        vaughan_trusted_hosts::add_custom_allowed_host(host);
    }

    let cli = parse_args();

    let navigate_after_load = cli
        .as_ref()
        .and_then(|c| c.initial_url.clone())
        .filter(|u| !u.trim().is_empty());

    let (webview_url, external_top_level) = resolve_main_webview_url(navigate_after_load.as_ref());

    let wallet_spawned = std::env::var("VAUGHAN_WALLET_SPAWNED")
        .map(|v| v == "1")
        .unwrap_or(false);
    let use_trusted_chrome = external_top_level || wallet_spawned;

    tracing::info!(
        target: "vaughan_ipc_browser",
        external_top_level,
        wallet_spawned,
        "Vaughan dApp browser starting"
    );

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

    let cli_config = cli.clone().unwrap_or(CliConfig {
        rpc_port: None,
        rpc_token: None,
        initial_url: None,
    });

    let rpc_port_scheme = cli_config.rpc_port.unwrap_or(0);
    let rpc_token_scheme = cli_config.rpc_token.clone().unwrap_or_default();

    tauri::Builder::default()
        .manage(cli_config)
        .invoke_handler(tauri::generate_handler![ipc_request, navigate_trusted_dapp, update_whitelist])
        .register_uri_scheme_protocol("vaughan", move |_app, request| {
            let path = request.uri().path();
            if path == "/provider" || path == "/provider/" {
                let html = include_str!("provider.html")
                    .replace("{{RPC_PORT}}", &rpc_port_scheme.to_string())
                    .replace("{{RPC_TOKEN}}", &rpc_token_scheme);
                tauri::http::Response::builder()
                    .header("Content-Type", "text/html")
                    .status(200)
                    .body(html.as_bytes().to_vec())
                    .unwrap()
            } else {
                tauri::http::Response::builder()
                    .status(404)
                    .body(Vec::new())
                    .unwrap()
            }
        })
        .setup(move |app| {
            let mut wv = tauri::WebviewWindowBuilder::new(app, "main", webview_url)
                .title("Vaughan - dApp Browser")
                .inner_size(1200.0, 800.0)
                .resizable(true)
                .devtools(true)
                .initialization_script_for_all_frames(include_str!("thin-proxy.js"));

            if let Some(script) = pending_js.as_ref() {
                wv = wv.initialization_script(script);
            }

            #[cfg(desktop)]
            {
                let app_newwin = app.handle().clone();
                wv = wv.on_new_window(move |url, _features| {
                    let url_str = url.to_string();
                    if let Ok(resolved_url) = resolve_allowlisted_new_window_target(&app_newwin, &url_str) {
                        if ON_NEWWIN_NAV_BUSY.swap(true, Ordering::AcqRel) {
                        } else {
                            let app_inner = app_newwin.clone();
                            let resolved = resolved_url.clone();
                            if let Err(e) = app_newwin.run_on_main_thread(move || {
                                let _clear_busy = ClearOnNewwinBusy;
                                if let Err(err) = navigate_main_trusted_url(&app_inner, resolved, true) {
                                    tracing::warn!(
                                        target: "vaughan_ipc_browser",
                                        err = %err,
                                        "on_new_window same-webview navigate failed"
                                    );
                                }
                            }) {
                                ON_NEWWIN_NAV_BUSY.store(false, Ordering::Release);
                                let _ = e;
                            }
                        }
                    } else if url_str == "about:blank" {
                        let app_inner = app_newwin.clone();
                        if let Err(e) = app_newwin.run_on_main_thread(move || {
                            if let Some(w) = app_inner.get_webview_window("main") {
                                // Some dApps open `about:blank` first, then redirect the returned handle.
                                // Pull the currently focused anchor href and re-route via Rust allowlist checks.
                                let _ = w.eval(
                                    r#"(() => {
  try {
    const a = document.activeElement;
    const href = a && a.href ? String(a.href) : "";
    if (!href) return;
    const inv =
      window.__TAURI__ && window.__TAURI__.core && window.__TAURI__.core.invoke
        ? window.__TAURI__.core.invoke
        : window.__TAURI__ && window.__TAURI__.invoke
        ? window.__TAURI__.invoke
        : null;
    if (!inv) return;
    inv("navigate_trusted_dapp", { url: href }).catch(() => {});
  } catch (_) {}
})();"#,
                                );
                            }
                        }) {
                            let _ = e;
                        }
                    }
                    NewWindowResponse::Deny
                });
            }

            #[cfg(desktop)]
            if use_trusted_chrome {
                use tauri::menu::{Menu, MenuItemBuilder, Submenu};

                // F5 is less often swallowed by dApp SPAs than Ctrl+R on Linux/Windows.
                let reload_accel = if cfg!(target_os = "macos") {
                    "Super+R"
                } else {
                    "F5"
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
                        hard_reload_main_webview(&w);
                        Ok(())
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
                                    let Ok(cmd) = serde_json::from_str::<WalletStdinControl>(t) else {
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
                                    let reveal = cmd.reveal;
                                    let app_for_dispatch = ctl.clone();
                                    let app_in_closure = ctl.clone();
                                    if let Err(e) = app_for_dispatch.run_on_main_thread(move || {
                                        if let Err(e) =
                                            navigate_main_trusted_app(&app_in_closure, url, reveal)
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

            if external_top_level {
                tracing::info!(
                    target: "vaughan_ipc_browser",
                    "main document is allowlisted external URL (no index.html shell)"
                );
            }

            if cli.is_none() {
                tracing::warn!(
                    target: "vaughan_ipc_browser",
                    "IPC config missing. Start with --ipc <endpoint> --token <token>"
                );
            }
            Ok(())
        })
        .build(tauri::generate_context!())
        .expect("error building tauri application")
        .run(move |_app, _event| {});
}

/// Navigates the **main** webview to `url` only if it passes [`parse_allowlisted_navigation_url`].
/// Use for in-app navigation (shell or external) without respawning; Rust rejects non-listed URLs.
#[tauri::command]
fn navigate_trusted_dapp(app: tauri::AppHandle, url: String) -> Result<(), String> {
    navigate_main_trusted_app(&app, url, true)
}

#[tauri::command]
fn update_whitelist(app: tauri::AppHandle, hosts: Vec<String>) {
    vaughan_trusted_hosts::reset_custom_allowed_hosts(hosts.clone());

    // Broadcast the updated whitelist array to all active Wry webviews/frames
    let custom_hosts_json = serde_json::to_string(&hosts).unwrap_or_else(|_| "[]".to_string());
    let eval_js = format!("window.__VAUGHAN_CUSTOM_TRUSTED_HOSTS = {};", custom_hosts_json);
    for (_, win) in app.webview_windows() {
        let _ = win.eval(&eval_js);
    }
}

#[tauri::command]
async fn ipc_request(
    _app: tauri::AppHandle,
    cli: tauri::State<'_, CliConfig>,
    request: serde_json::Value,
) -> Result<serde_json::Value, String> {
    let req_type = request.get("type")
        .and_then(|v| v.as_str())
        .ok_or_else(|| "Missing request type".to_string())?;
        
    let req_payload = request.get("payload")
        .cloned()
        .unwrap_or(serde_json::Value::Null);

    let method = match req_type {
        "GetNetworkInfo" => "vaughan_getNetworkInfo",
        "GetAccounts" => "vaughan_getAccounts",
        "SignTransaction" => "vaughan_signTransaction",
        "SignMessage" => "vaughan_signMessage",
        "AddTrustedHost" => "vaughan_addTrustedHost",
        "RemoveTrustedHost" => "vaughan_removeTrustedHost",
        "GetTrustedHosts" => "vaughan_getTrustedHosts",
        other => return Err(format!("Unknown legacy request type: {}", other)),
    };

    let json_rpc_body = serde_json::json!({
        "jsonrpc": "2.0",
        "method": method,
        "params": req_payload,
        "id": 1
    });

    let port = cli.rpc_port.unwrap_or(0);
    let token = cli.rpc_token.as_deref().unwrap_or("");
    if port == 0 {
        return Err("RPC port not configured".to_string());
    }

    let client = reqwest::Client::new();
    let url = format!("http://127.0.0.1:{}/rpc", port);

    let res = client.post(&url)
        .header("Content-Type", "application/json")
        .header("Authorization", format!("Bearer {}", token))
        .json(&json_rpc_body)
        .send()
        .await
        .map_err(|e| e.to_string())?;

    if !res.status().is_success() {
        return Err(format!("HTTP error status: {}", res.status()));
    }

    let json_res: serde_json::Value = res.json()
        .await
        .map_err(|e| e.to_string())?;

    if let Some(err) = json_res.get("error") {
        return Err(err.get("message")
            .and_then(|v| v.as_str())
            .unwrap_or("JSON-RPC error")
            .to_string());
    }

    let result = json_res.get("result")
        .cloned()
        .unwrap_or(serde_json::Value::Null);

    let response_type = match req_type {
        "GetNetworkInfo" => "NetworkInfo",
        "GetAccounts" => "Accounts",
        "SignTransaction" => "SignedTransaction",
        "SignMessage" => "SignedMessage",
        "AddTrustedHost" | "RemoveTrustedHost" | "GetTrustedHosts" => "CustomHosts",
        other => other,
    };

    Ok(serde_json::json!({
        "type": response_type,
        "payload": result
    }))
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

fn load_custom_hosts_from_state_json() -> Vec<String> {
    let base = dirs::data_dir()
        .unwrap_or_else(|| std::env::current_dir().unwrap_or_else(|_| std::path::PathBuf::from(".")));
    let path = base.join("vaughan-dioxus").join("state.json");
    if !path.exists() {
        return Vec::new();
    }
    let Ok(bytes) = std::fs::read(path) else {
        return Vec::new();
    };
    let Ok(val) = serde_json::from_slice::<serde_json::Value>(&bytes) else {
        return Vec::new();
    };
    let Some(custom_list) = val.get("custom_trusted_dapps").and_then(|v| v.as_array()) else {
        return Vec::new();
    };
    let mut hosts = Vec::new();
    for item in custom_list {
        if let Some(url_str) = item.get("url").and_then(|v| v.as_str()) {
            if let Ok(parsed) = url::Url::parse(url_str) {
                if let Some(host) = parsed.host_str() {
                    hosts.push(host.to_lowercase());
                }
            }
        }
    }
    hosts
}


