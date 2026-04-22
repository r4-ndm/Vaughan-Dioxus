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

mod ipc;
mod ipc_pool;

use std::io::Write;
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::Duration;

use ipc_pool::WalletIpcPool;
use serde::Deserialize;
use tauri::webview::NewWindowResponse;
use tauri::webview::PageLoadEvent;
use tauri::Manager;
use tauri::Url;
use tauri::WebviewUrl;
use tauri::WebviewWindow;

use vaughan_ipc_types::{IpcRequest, IpcResponse};
use vaughan_trusted_hosts::hostname_is_whitelisted;

fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

fn emit_wallet_event(event: &str, data: serde_json::Value) {
    let payload = serde_json::json!({
        "event": event,
        "data": data,
        "ts_ms": now_ms(),
    });
    let mut out = std::io::stdout().lock();
    let _ = writeln!(out, "{payload}");
    let _ = out.flush();
}

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

fn warmup_status_bar_script(remaining_secs: u64) -> String {
    const TOTAL_SECS: u64 = 150;
    let initial_remaining = remaining_secs.min(TOTAL_SECS);
    let mode = if initial_remaining > 0 {
        "warming"
    } else {
        "ready"
    };
    format!(
        r#"(function(){{
  if (window.__VAUGHAN_WARMUP_BAR_INIT__) return;
  window.__VAUGHAN_WARMUP_BAR_INIT__ = true;
  var MODE = {mode:?};
  var REM = {initial_remaining};
  var TOTAL = {TOTAL_SECS};
  var ROOT_ID = 'vaughan-warmup-root';
  var FILL_ID = 'vaughan-warmup-fill';
  var LABEL_ID = 'vaughan-warmup-label';
  var READY_TEXT = 'Rocket warmup ready - fastest launch state';
  function ensure() {{
    var root = document.getElementById(ROOT_ID);
    if (!root) {{
      root = document.createElement('div');
      root.id = ROOT_ID;
      root.style.position = 'fixed';
      root.style.left = '0';
      root.style.right = '0';
      root.style.top = '0';
      root.style.zIndex = '2147483647';
      root.style.padding = '6px 10px 8px 10px';
      root.style.background = 'rgba(18,20,24,0.96)';
      root.style.borderBottom = '1px solid rgba(255,255,255,0.12)';
      root.style.pointerEvents = 'none';

      var label = document.createElement('div');
      label.id = LABEL_ID;
      label.style.font = '700 13px/1.25 system-ui,-apple-system,Segoe UI,Roboto,sans-serif';
      label.style.marginBottom = '6px';
      label.style.textAlign = 'center';

      var track = document.createElement('div');
      track.style.height = '10px';
      track.style.borderRadius = '999px';
      track.style.background = 'rgba(255,255,255,0.14)';
      track.style.overflow = 'hidden';

      var fill = document.createElement('div');
      fill.id = FILL_ID;
      fill.style.height = '100%';
      fill.style.width = '0%';
      fill.style.borderRadius = '999px';
      fill.style.transition = 'width 0.6s ease';
      fill.style.backgroundSize = '24px 24px';
      fill.style.backgroundImage =
        'linear-gradient(45deg, rgba(255,255,255,0.22) 25%, transparent 25%, transparent 50%, rgba(255,255,255,0.22) 50%, rgba(255,255,255,0.22) 75%, transparent 75%, transparent)';
      fill.style.animation = 'vaughanWarmupStripe 1s linear infinite';

      track.appendChild(fill);
      root.appendChild(label);
      root.appendChild(track);
      document.documentElement.appendChild(root);

      var style = document.createElement('style');
      style.textContent = '@keyframes vaughanWarmupStripe{{from{{background-position:0 0;}}to{{background-position:24px 0;}}}}';
      document.documentElement.appendChild(style);
      var body = document.body;
      if (body && !body.dataset.vaughanWarmupPad) {{
        body.dataset.vaughanWarmupPad = '1';
        body.style.paddingTop = '44px';
      }}
    }}
    var fill = document.getElementById(FILL_ID);
    var label = document.getElementById(LABEL_ID);
    if (!fill || !label) return;
    if (MODE === 'warming') {{
      var left = Math.max(0, REM);
      var pct = Math.round(((TOTAL - left) / TOTAL) * 100);
      fill.style.width = pct + '%';
      fill.style.backgroundColor = '#d58b00';
      label.style.color = '#ffe8b0';
      label.textContent = 'Rocket warmup ' + pct + '%  (~' + left + 's left)';
      if (left > 0) REM = left - 1;
      if (left <= 0) MODE = 'ready';
    }} else {{
      fill.style.width = '100%';
      fill.style.backgroundColor = '#0fa15f';
      fill.style.backgroundImage = 'none';
      fill.style.animation = 'none';
      label.style.color = '#c8ffd9';
      label.textContent = READY_TEXT;
    }}
  }}
  ensure();
  setInterval(ensure, 1000);
}})();"#,
    )
}

/// Cap wallet control lines so a bug or broken pipe cannot grow unbounded in memory.
const MAX_WALLET_STDIN_LINE_BYTES: usize = 8192;
const WARM_SLOT_MVP_CAP: u8 = 6;
const PROVIDER_INIT_SCRIPT: &str = include_str!("../provider_inject.js");

/// Same `eval` fallback as the main webview, installed on warm-slot `PageLoadEvent::Finished`.
static WARM_SLOT_PROVIDER_FALLBACK: OnceLock<Option<Arc<String>>> = OnceLock::new();
static WARM_SLOT_PING_FAIL_STREAK: OnceLock<Mutex<HashMap<u8, u8>>> = OnceLock::new();

fn warm_slot_ping_streaks() -> &'static Mutex<HashMap<u8, u8>> {
    WARM_SLOT_PING_FAIL_STREAK.get_or_init(|| Mutex::new(HashMap::new()))
}

fn warm_slot_reset_ping_streak(slot_id: u8) {
    if let Ok(mut m) = warm_slot_ping_streaks().lock() {
        m.insert(slot_id, 0);
    }
}

fn warm_slot_remove_ping_streak(slot_id: u8) -> bool {
    if let Ok(mut m) = warm_slot_ping_streaks().lock() {
        return m.remove(&slot_id).is_some();
    }
    false
}

fn warm_slot_next_ping_streak(slot_id: u8) -> u8 {
    if let Ok(mut m) = warm_slot_ping_streaks().lock() {
        let prev = *m.get(&slot_id).unwrap_or(&0);
        let next = prev.saturating_add(1);
        m.insert(slot_id, next);
        return next;
    }
    1
}

fn warm_slot_tracked_ids() -> Vec<u8> {
    warm_slot_ping_streaks()
        .lock()
        .ok()
        .map(|m| m.keys().copied().collect())
        .unwrap_or_default()
}

fn default_reveal_true() -> bool {
    true
}

#[derive(Debug, Deserialize)]
struct WalletWarmSlotById {
    slot_id: u8,
}

#[derive(Debug, Deserialize)]
struct WalletWarmSlotNavigateHidden {
    slot_id: u8,
    url: String,
}

#[derive(Debug, Deserialize)]
struct WalletStdinControl {
    cmd: Option<String>,
    id: Option<u8>,
    url: Option<String>,
    navigate_trusted: Option<String>,
    #[serde(default = "default_reveal_true")]
    reveal: bool,
    warm_slot_navigate_hidden: Option<WalletWarmSlotNavigateHidden>,
    warm_slot_show: Option<WalletWarmSlotById>,
    warm_slot_hide: Option<WalletWarmSlotById>,
    warm_slot_destroy: Option<WalletWarmSlotById>,
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
fn navigate_main_trusted_app(
    app: &tauri::AppHandle,
    url: String,
    reveal: bool,
) -> Result<(), String> {
    let u = parse_allowlisted_navigation_url(&url)?;
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

fn warm_slot_label(slot_id: u8) -> Option<String> {
    if slot_id < WARM_SLOT_MVP_CAP {
        Some(format!("warm-slot-{slot_id}"))
    } else {
        None
    }
}

fn ensure_warm_slot_window(
    app: &tauri::AppHandle,
    slot_id: u8,
) -> Result<tauri::WebviewWindow, String> {
    let label = warm_slot_label(slot_id).ok_or_else(|| "invalid warm slot id".to_string())?;
    if let Some(w) = app.get_webview_window(&label) {
        return Ok(w);
    }
    let fallback = WARM_SLOT_PROVIDER_FALLBACK
        .get()
        .cloned()
        .unwrap_or(None);
    let slot_for_cb = slot_id;
    let mut wvb = tauri::WebviewWindowBuilder::new(
        app,
        label.clone(),
        WebviewUrl::App("index.html".into()),
    )
    .title("Vaughan - dApp Warm Slot")
    .inner_size(1200.0, 800.0)
    .resizable(true)
    .visible(false)
    .initialization_script_for_all_frames(PROVIDER_INIT_SCRIPT);
    if let Some(fallback_js) = fallback {
        let fb = Arc::clone(&fallback_js);
        wvb = wvb.on_page_load(move |window, payload| {
            if payload.event() != PageLoadEvent::Finished {
                return;
            }
            let url_owned = payload.url().to_string();
            match payload.url().scheme() {
                "about" | "data" | "blob" => return,
                _ => {}
            }
            if url_owned.starts_with("tauri://") || url_owned.starts_with("asset://") {
                return;
            }
            let invited = match payload.url().scheme() {
                "https" => parse_allowlisted_navigation_url(&url_owned).is_ok(),
                "http" => {
                    let host = payload.url().host_str().unwrap_or("").to_lowercase();
                    host == "localhost" || host == "127.0.0.1"
                }
                _ => false,
            };
            if invited {
                let _ = window.eval(fb.as_str());
            }
            emit_wallet_event(
                "slot_loaded",
                serde_json::json!({
                    "slot_id": slot_for_cb,
                    "url": url_owned,
                    "success": invited,
                }),
            );
        });
    }
    wvb.build().map_err(|e| e.to_string())
}

fn warm_slot_navigate_hidden(
    app: &tauri::AppHandle,
    slot_id: u8,
    url: String,
) -> Result<(), String> {
    let u = parse_allowlisted_navigation_url(&url)?;
    let w = ensure_warm_slot_window(app, slot_id)?;
    w.navigate(u).map_err(|e| e.to_string())?;
    let _ = w.hide();
    warm_slot_reset_ping_streak(slot_id);
    Ok(())
}

fn warm_slot_show(app: &tauri::AppHandle, slot_id: u8) -> Result<(), String> {
    let label = warm_slot_label(slot_id).ok_or_else(|| "invalid warm slot id".to_string())?;
    let w = app
        .get_webview_window(&label)
        .ok_or_else(|| "warm slot window missing".to_string())?;
    let _ = w.show();
    let _ = w.set_focus();
    warm_slot_reset_ping_streak(slot_id);
    emit_wallet_event("slot_claimed", serde_json::json!({ "slot_id": slot_id }));
    Ok(())
}

fn warm_slot_hide(app: &tauri::AppHandle, slot_id: u8) -> Result<(), String> {
    let label = warm_slot_label(slot_id).ok_or_else(|| "invalid warm slot id".to_string())?;
    let had_window = if let Some(w) = app.get_webview_window(&label) {
        let _ = w.hide();
        true
    } else {
        false
    };
    if had_window {
        warm_slot_reset_ping_streak(slot_id);
    }
    emit_wallet_event("slot_hidden", serde_json::json!({ "slot_id": slot_id }));
    Ok(())
}

fn warm_slot_destroy(app: &tauri::AppHandle, slot_id: u8) -> Result<(), String> {
    let label = warm_slot_label(slot_id).ok_or_else(|| "invalid warm slot id".to_string())?;
    if let Some(w) = app.get_webview_window(&label) {
        let _ = w.close();
    }
    let _ = warm_slot_remove_ping_streak(slot_id);
    emit_wallet_event("slot_destroyed", serde_json::json!({ "slot_id": slot_id }));
    Ok(())
}

fn warm_slot_health_check(app: &tauri::AppHandle) {
    let tracked_slots: Vec<u8> = warm_slot_tracked_ids();
    for slot_id in tracked_slots {
        let Some(label) = warm_slot_label(slot_id) else {
            continue;
        };
        let Some(w) = app.get_webview_window(&label) else {
            let was_tracked = warm_slot_remove_ping_streak(slot_id);
            if was_tracked {
                emit_wallet_event("slot_crashed", serde_json::json!({ "slot_id": slot_id }));
            }
            continue;
        };
        let ok = w.eval("1").is_ok();
        if ok {
            warm_slot_reset_ping_streak(slot_id);
            continue;
        }
        let streak_now = warm_slot_next_ping_streak(slot_id);
        if streak_now >= 3 {
            emit_wallet_event("slot_crashed", serde_json::json!({ "slot_id": slot_id }));
            let _ = warm_slot_destroy(app, slot_id);
        }
    }
}

#[derive(Debug, Clone)]
struct CliConfig {
    ipc_endpoint: String,
    token: String,
    initial_url: Option<String>,
}

static IPC_REQ_ID: AtomicU64 = AtomicU64::new(1);

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

    let wallet_spawned = std::env::var("VAUGHAN_WALLET_SPAWNED")
        .map(|v| v == "1")
        .unwrap_or(false);
    let wallet_warm_shell = std::env::var("VAUGHAN_WALLET_WARM_SHELL")
        .map(|v| v == "1")
        .unwrap_or(false);
    let warmup_hint_remaining_secs = std::env::var("VAUGHAN_WARMUP_HINT_REMAINING_SECS")
        .ok()
        .and_then(|s| s.parse::<u64>().ok());
    let warmup_hint_is_rocket = std::env::var("VAUGHAN_WARMUP_HINT_IS_ROCKET")
        .map(|v| v == "1")
        .unwrap_or(false);
    tracing::info!(
        target: "vaughan_ipc_browser",
        remaining_secs = ?warmup_hint_remaining_secs,
        is_rocket = warmup_hint_is_rocket,
        "Vaughan warmup hint"
    );
    // Keep the OS process alive across window-close only for the wallet's *warm shell* (the
    // long-lived child that hosts the main webview + hidden warm slots). `_new_window`
    // children (spawned per TV-on click with `VAUGHAN_WALLET_SPAWNED=1` but without
    // `VAUGHAN_WALLET_WARM_SHELL=1`) should close cleanly so we don't leak a full WebKit
    // process every time the user closes a TV-opened dApp window.
    let intercept_wallet_close = wallet_warm_shell;
    let use_trusted_chrome = external_top_level || wallet_spawned;

    tracing::info!(
        target: "vaughan_ipc_browser",
        external_top_level,
        wallet_spawned,
        warm_shell = wallet_warm_shell,
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

    tauri::Builder::default()
        .manage(ipc_pool)
        .invoke_handler(tauri::generate_handler![ipc_request, navigate_trusted_dapp])
        .setup(move |app| {
            let provider_fallback_eval: Option<Arc<String>> = if use_trusted_chrome {
                let quoted = serde_json::to_string(PROVIDER_INIT_SCRIPT)
                    .expect("serialize provider script");
                Some(Arc::new(format!(
                    "(function(){{if(window.__VAUGHAN_ETH_INJECTED__)return;try{{eval({quoted});}}catch(e){{console.error('[Vaughan] provider fallback eval failed',e);}}}})();"
                )))
            } else {
                None
            };

            let _ = WARM_SLOT_PROVIDER_FALLBACK.set(provider_fallback_eval.clone());

            let mut wv = tauri::WebviewWindowBuilder::new(app, "main", webview_url)
                .title("Vaughan - dApp Browser")
                .inner_size(1200.0, 800.0)
                .resizable(true)
                .initialization_script_for_all_frames(PROVIDER_INIT_SCRIPT);

            if wallet_warm_shell {
                wv = wv.visible(false);
            }

            let warmup_bar_eval: Option<Arc<String>> =
                if wallet_spawned && !wallet_warm_shell && warmup_hint_is_rocket {
                    warmup_hint_remaining_secs
                        .map(warmup_status_bar_script)
                        .map(Arc::new)
                } else {
                    None
                };

            if let Some(fallback_js) = provider_fallback_eval {
                let warmup_bar_eval = warmup_bar_eval.clone();
                wv = wv.on_page_load(move |window, payload| {
                    if payload.event() == PageLoadEvent::Started {
                        if let Some(ref warm_js) = warmup_bar_eval {
                            let _ = window.eval(warm_js.as_str());
                        }
                        return;
                    }
                    if payload.event() != PageLoadEvent::Finished {
                        return;
                    }
                    match payload.url().scheme() {
                        "about" | "data" | "blob" => return,
                        _ => {}
                    }
                    let _ = window.eval(fallback_js.as_str());
                    if let Some(ref warm_js) = warmup_bar_eval {
                        let _ = window.eval(warm_js.as_str());
                    }
                });
            }

            if let Some(script) = pending_js.as_ref() {
                wv = wv.initialization_script(script);
            }
            if wallet_spawned && !wallet_warm_shell && warmup_hint_is_rocket {
                let remaining = warmup_hint_remaining_secs.unwrap_or(90);
                let warm_script = warmup_status_bar_script(remaining);
                wv = wv.initialization_script(&warm_script);
            }

            #[cfg(desktop)]
            {
                let app_newwin = app.handle().clone();
                wv = wv.on_new_window(move |url, _features| {
                    let url_str = url.to_string();
                    let allow = parse_allowlisted_navigation_url(&url_str).is_ok();
                    if allow {
                        if ON_NEWWIN_NAV_BUSY.swap(true, Ordering::AcqRel) {
                        } else {
                            let app_inner = app_newwin.clone();
                            let u = url_str;
                            if let Err(e) = app_newwin.run_on_main_thread(move || {
                                let _clear_busy = ClearOnNewwinBusy;
                                if let Err(err) = navigate_main_trusted_app(&app_inner, u, true) {
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
                    }
                    NewWindowResponse::Deny
                });
            }

            // Overlap socket connect + handshake with WebKit window creation so the wallet gate
            // (`dapp_browser_ipc_handshake_seen`) clears sooner on warm start.
            if let Some(pool) = ipc_pool_for_warm.as_ref() {
                let p = Arc::clone(pool);
                tauri::async_runtime::spawn(async move {
                    p.warm_connections().await;
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

            if wallet_spawned && !wallet_warm_shell && warmup_hint_is_rocket {
                let warm_script = warmup_status_bar_script(warmup_hint_remaining_secs.unwrap_or(90));
                let handle = app.handle().clone();
                std::thread::Builder::new()
                    .name("vaughan-warmup-bar-retry".into())
                    .spawn(move || {
                        // Retry aggressively for early-load races / SPA rewrites.
                        for _ in 0..12 {
                            std::thread::sleep(Duration::from_millis(750));
                            let script = warm_script.clone();
                            let h2 = handle.clone();
                            let _ = handle.run_on_main_thread(move || {
                                if let Some(w) = h2.get_webview_window("main") {
                                    let _ = w.eval(script.as_str());
                                }
                            });
                        }
                    })
                    .ok();
            }

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
                                    if let Some(name) = cmd.cmd.as_deref() {
                                        match name {
                                            "create_webview" => {
                                                let Some(slot_id) = cmd.id else { continue; };
                                                let app_for_dispatch = ctl.clone();
                                                let app_in_closure = ctl.clone();
                                                let _ = app_for_dispatch.run_on_main_thread(move || {
                                                    if ensure_warm_slot_window(&app_in_closure, slot_id).is_ok() {
                                                        emit_wallet_event("slot_created", serde_json::json!({ "slot_id": slot_id }));
                                                    }
                                                });
                                                continue;
                                            }
                                            "navigate" => {
                                                let (Some(slot_id), Some(url)) = (cmd.id, cmd.url) else { continue; };
                                                let app_for_dispatch = ctl.clone();
                                                let app_in_closure = ctl.clone();
                                                let _ = app_for_dispatch.run_on_main_thread(move || {
                                                    let _ = warm_slot_navigate_hidden(&app_in_closure, slot_id, url);
                                                });
                                                continue;
                                            }
                                            "show" => {
                                                let Some(slot_id) = cmd.id else { continue; };
                                                let app_for_dispatch = ctl.clone();
                                                let app_in_closure = ctl.clone();
                                                let _ = app_for_dispatch.run_on_main_thread(move || {
                                                    let _ = warm_slot_show(&app_in_closure, slot_id);
                                                });
                                                continue;
                                            }
                                            "hide" => {
                                                let Some(slot_id) = cmd.id else { continue; };
                                                let app_for_dispatch = ctl.clone();
                                                let app_in_closure = ctl.clone();
                                                let _ = app_for_dispatch.run_on_main_thread(move || {
                                                    let _ = warm_slot_hide(&app_in_closure, slot_id);
                                                });
                                                continue;
                                            }
                                            "destroy" => {
                                                let Some(slot_id) = cmd.id else { continue; };
                                                let app_for_dispatch = ctl.clone();
                                                let app_in_closure = ctl.clone();
                                                let _ = app_for_dispatch.run_on_main_thread(move || {
                                                    let _ = warm_slot_destroy(&app_in_closure, slot_id);
                                                });
                                                continue;
                                            }
                                            _ => {}
                                        }
                                    }
                                    if let Some(req) = cmd.warm_slot_navigate_hidden {
                                        let app_for_dispatch = ctl.clone();
                                        let app_in_closure = ctl.clone();
                                        if let Err(e) = app_for_dispatch.run_on_main_thread(move || {
                                            if let Err(e) = warm_slot_navigate_hidden(
                                                &app_in_closure,
                                                req.slot_id,
                                                req.url,
                                            ) {
                                                tracing::warn!(
                                                    target: "vaughan_ipc_browser",
                                                    err = %e,
                                                    "warm_slot_navigate_hidden failed"
                                                );
                                            }
                                        }) {
                                            tracing::warn!(
                                                target: "vaughan_ipc_browser",
                                                err = %e,
                                                "run_on_main_thread failed for warm_slot_navigate_hidden"
                                            );
                                        }
                                        continue;
                                    }
                                    if let Some(req) = cmd.warm_slot_show {
                                        let app_for_dispatch = ctl.clone();
                                        let app_in_closure = ctl.clone();
                                        if let Err(e) = app_for_dispatch.run_on_main_thread(move || {
                                            if let Err(e) = warm_slot_show(&app_in_closure, req.slot_id)
                                            {
                                                tracing::warn!(
                                                    target: "vaughan_ipc_browser",
                                                    err = %e,
                                                    "warm_slot_show failed"
                                                );
                                            }
                                        }) {
                                            tracing::warn!(
                                                target: "vaughan_ipc_browser",
                                                err = %e,
                                                "run_on_main_thread failed for warm_slot_show"
                                            );
                                        }
                                        continue;
                                    }
                                    if let Some(req) = cmd.warm_slot_hide {
                                        let app_for_dispatch = ctl.clone();
                                        let app_in_closure = ctl.clone();
                                        if let Err(e) = app_for_dispatch.run_on_main_thread(move || {
                                            let _ = warm_slot_hide(&app_in_closure, req.slot_id);
                                        }) {
                                            tracing::warn!(
                                                target: "vaughan_ipc_browser",
                                                err = %e,
                                                "run_on_main_thread failed for warm_slot_hide"
                                            );
                                        }
                                        continue;
                                    }
                                    if let Some(req) = cmd.warm_slot_destroy {
                                        let app_for_dispatch = ctl.clone();
                                        let app_in_closure = ctl.clone();
                                        if let Err(e) = app_for_dispatch.run_on_main_thread(move || {
                                            let _ = warm_slot_destroy(&app_in_closure, req.slot_id);
                                        }) {
                                            tracing::warn!(
                                                target: "vaughan_ipc_browser",
                                                err = %e,
                                                "run_on_main_thread failed for warm_slot_destroy"
                                            );
                                        }
                                        continue;
                                    }
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
            emit_wallet_event(
                "ready",
                serde_json::json!({ "max_webviews": WARM_SLOT_MVP_CAP }),
            );

            std::thread::Builder::new()
                .name("vaughan-browser-heartbeat".into())
                .spawn(|| loop {
                    std::thread::sleep(Duration::from_secs(5));
                    emit_wallet_event("heartbeat", serde_json::json!({}));
                })
                .ok();

            let app_for_health = app.handle().clone();
            std::thread::Builder::new()
                .name("vaughan-warm-slot-health".into())
                .spawn(move || loop {
                    std::thread::sleep(Duration::from_secs(3));
                    let app_dispatch = app_for_health.clone();
                    let app_inner = app_for_health.clone();
                    let _ = app_dispatch.run_on_main_thread(move || {
                        warm_slot_health_check(&app_inner);
                    });
                })
                .ok();

            Ok(())
        })
        .build(tauri::generate_context!())
        .expect("error building tauri application")
        .run(move |app, event| {
            if let tauri::RunEvent::ExitRequested { ref api, .. } = event {
                if intercept_wallet_close {
                    api.prevent_exit();
                    return;
                }
            }
            // `CloseRequested` is not delivered through `WebviewWindow::on_window_event` on this path;
            // handle it from `RunEvent` so `prevent_close()` runs before Wry decides to tear the window down.
            // Defer `hide()` to the next main-loop turn: calling GTK hide synchronously inside the close
            // handler has caused heap corruption on Linux (glibc "corrupted double-linked list").
            if let tauri::RunEvent::WindowEvent { label, event, .. } = event {
                // `window.ethereum` is intentionally not exposed (EIP-6963 only) —
                // use the non-enumerable backreference set by `provider_inject.js`.
                const DISCONNECT_ETH: &str = "try{var p=window.__vaughanEthereum;if(p&&typeof p.disconnect==='function'){var d=p.disconnect();if(d&&typeof d.then==='function')d.catch(function(){});}}catch(e){}";

                if label == "main" {
                    if !intercept_wallet_close {
                        return;
                    }
                    if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                        api.prevent_close();
                        let handle = app.clone();
                        let _ = app.run_on_main_thread(move || {
                            if let Some(w) = handle.get_webview_window("main") {
                                let _ = w.eval(DISCONNECT_ETH);
                                let _ = w.hide();
                            }
                        });
                    }
                    return;
                }

                // Rocket warm-slot windows: hide instead of destroy so the parent can refill the slot.
                if let Some(slot_id) = label
                    .strip_prefix("warm-slot-")
                    .and_then(|s| s.parse::<u8>().ok())
                {
                    if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                        api.prevent_close();
                        let handle = app.clone();
                        let slot_label = label.to_string();
                        let _ = app.run_on_main_thread(move || {
                            if let Some(w) = handle.get_webview_window(&slot_label) {
                                let _ = w.eval(DISCONNECT_ETH);
                            }
                            let _ = warm_slot_hide(&handle, slot_id);
                        });
                    }
                }
            }
        });
}

/// Navigates the **main** webview to `url` only if it passes [`parse_allowlisted_navigation_url`].
/// Use for in-app navigation (shell or external) without respawning; Rust rejects non-listed URLs.
#[tauri::command]
fn navigate_trusted_dapp(app: tauri::AppHandle, url: String) -> Result<(), String> {
    navigate_main_trusted_app(&app, url, true)
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
