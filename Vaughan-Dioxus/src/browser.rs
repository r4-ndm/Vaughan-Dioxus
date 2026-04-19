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

use std::collections::HashMap;
use std::io::{BufRead, BufReader, Write};
use std::net::{TcpStream, ToSocketAddrs};
use std::path::PathBuf;
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, OnceLock};
use std::thread;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use url::Url;
pub use vaughan_trusted_hosts::hostname_is_whitelisted;

use crate::services::{shared_services, AppServices};
use crate::wallet_ipc::{self, WalletIpcServer};

fn warm_dapp_browser_env_enabled() -> bool {
    std::env::var("VAUGHAN_NO_WARM_DAPP_BROWSER")
        .map(|v| v != "1")
        .unwrap_or(true)
}

/// IPC endpoint + token for spawning the dApp browser (also used for lazy launch if the binary was missing at startup).
struct DappBoot {
    endpoint: String,
    token: String,
}

static WALLET_DAPP_BOOT: OnceLock<DappBoot> = OnceLock::new();
static EXTRA_BROWSER_CHILDREN: OnceLock<Mutex<Vec<Child>>> = OnceLock::new();
static DAPP_WARMUP_STARTED_AT: OnceLock<Instant> = OnceLock::new();
static LAST_PREWARM_CANDIDATES: OnceLock<Mutex<Vec<String>>> = OnceLock::new();
static LAST_PREWARM_SLOT_BY_KEY: OnceLock<Mutex<HashMap<String, u8>>> = OnceLock::new();
static HIDDEN_REWARM_RETRY_RUNNING: AtomicBool = AtomicBool::new(false);
/// Max simultaneous hidden warm WebViews in the dApp child (matches rocket cap per chain).
const WARM_SLOT_CAP: usize = 6;

/// Last `heartbeat` / `ready` from the dApp child stdout (warm pool liveness).
static DAPP_BROWSER_HEARTBEAT_AT: Mutex<Option<Instant>> = Mutex::new(None);
/// Debounce stale-heartbeat recovery so we do not destroy slots every monitor tick.
static STALE_HEARTBEAT_RECOVERY_AT: Mutex<Option<Instant>> = Mutex::new(None);
const DAPP_WARMUP_HINT_WINDOW: Duration = Duration::from_secs(150);

fn warmup_hint_remaining_secs() -> u64 {
    let started = DAPP_WARMUP_STARTED_AT.get_or_init(Instant::now);
    DAPP_WARMUP_HINT_WINDOW
        .saturating_sub(started.elapsed())
        .as_secs()
}

/// Per-dApp warm status: drives the "🚀 Loading" / "🚀 Ready" pill on the
/// dApp card. Single-warm-pool mode (no per-URL slots) only ever returns
/// `"Warming"` — claiming `"Ready"` there is dishonest because nothing was
/// actually pre-loaded for that specific URL.
pub fn dapp_warm_hint_for_url(url: &str) -> &'static str {
    if !multi_warm_pool_env_enabled() {
        return "Warming";
    }

    let key = normalize_dapp_usage_key(url);
    let slot_opt = LAST_PREWARM_SLOT_BY_KEY
        .get_or_init(|| Mutex::new(HashMap::new()))
        .lock()
        .ok()
        .and_then(|m| m.get(&key).copied());
    let Some(slot_id) = slot_opt else {
        return if warmup_hint_remaining_secs() > 0 {
            "Queued"
        } else {
            "Idle"
        };
    };

    let cap = effective_warm_slot_cap();
    let Ok(pool) = warm_pool_mutex().lock() else {
        return "Unknown";
    };
    if pool.url_in_backoff(url) {
        return "Backoff";
    }
    let idx = slot_id as usize;
    if idx >= cap || idx >= pool.slots.len() {
        return "Idle";
    }
    match &pool.slots[idx] {
        WarmSlotPhase::Empty => "Queued",
        WarmSlotPhase::Warming { .. } => "Warming",
        WarmSlotPhase::Ready { .. } => "Ready",
        WarmSlotPhase::Claimed { .. } => "Claimed",
        WarmSlotPhase::Refilling { .. } => "Refilling",
    }
}

fn is_fast_dapp_selected(url: &str) -> bool {
    let key = normalize_dapp_usage_key(url);
    let services = shared_services();
    let snapshot = services.persistence.snapshot();
    let prefs = snapshot.preferences.unwrap_or_default();
    prefs
        .fast_dapps_by_chain_v1
        .values()
        .any(|list| list.iter().any(|k| normalize_dapp_usage_key(k) == key))
}

/// Per-URL warm pool of hidden WebKit windows for starred dApps. Default ON;
/// set `VAUGHAN_MULTI_WARM_POOL=0` to fall back to a single shared warm shell.
fn multi_warm_pool_env_enabled() -> bool {
    std::env::var("VAUGHAN_MULTI_WARM_POOL")
        .map(|v| v != "0")
        .unwrap_or(true)
}

/// Soft cap on warm slots from available RAM (Linux `MemAvailable`); other OS → full cap.
fn effective_warm_slot_cap() -> usize {
    #[cfg(target_os = "linux")]
    {
        if let Ok(s) = std::fs::read_to_string("/proc/meminfo") {
            for line in s.lines() {
                let Some(rest) = line.strip_prefix("MemAvailable:") else {
                    continue;
                };
                let kb: u64 = rest
                    .split_whitespace()
                    .next()
                    .and_then(|n| n.parse().ok())
                    .unwrap_or(0);
                let mb = kb / 1024;
                if mb < 256 {
                    return 1;
                }
                if mb < 512 {
                    return 3;
                }
            }
        }
    }
    WARM_SLOT_CAP
}

fn touch_dapp_browser_heartbeat() {
    if let Ok(mut g) = DAPP_BROWSER_HEARTBEAT_AT.lock() {
        *g = Some(Instant::now());
    }
}

fn maybe_recover_stale_dapp_browser_heartbeat(inner: &mut BrowserInner, cap: usize) {
    let Some(last) = DAPP_BROWSER_HEARTBEAT_AT
        .lock()
        .ok()
        .and_then(|g| *g)
    else {
        return;
    };
    if last.elapsed() <= Duration::from_secs(45) {
        return;
    }
    let mut do_recovery = false;
    if let Ok(mut r) = STALE_HEARTBEAT_RECOVERY_AT.lock() {
        let due = r
            .map(|t| t.elapsed() > Duration::from_secs(60))
            .unwrap_or(true);
        if due {
            *r = Some(Instant::now());
            do_recovery = true;
        }
    }
    if !do_recovery {
        return;
    }
    // If the browser child is still alive, treat missing heartbeat as transient stdout silence
    // (or event-thread delay) and avoid destructive pool clears.
    if inner.child_is_alive() && inner.control_stdin.is_some() {
        if let Ok(mut g) = DAPP_BROWSER_HEARTBEAT_AT.lock() {
            *g = Some(Instant::now());
        }
        return;
    }
    for id in 0..cap {
        let _ = inner.try_send_warm_slot_destroy(id as u8);
    }
    warm_pool_reset_all_empty();
}

#[derive(Clone, Debug)]
enum WarmSlotPhase {
    Empty,
    Warming {
        url: String,
        since: Instant,
    },
    Ready {
        url: String,
    },
    Claimed {
        url: String,
    },
    Refilling {
        url: String,
    },
}

fn warm_pool_set_slot(pool: &mut WarmPool, slot_id: usize, next: WarmSlotPhase) {
    pool.slots[slot_id] = next;
}

struct WarmPool {
    slots: Vec<WarmSlotPhase>,
    /// Host-key → do not retry warming until this instant (exponential backoff).
    failed_until: HashMap<String, Instant>,
    fail_streak: HashMap<String, u32>,
}

impl WarmPool {
    fn record_url_failure(&mut self, url: &str) {
        let key = normalize_dapp_usage_key(url);
        let streak = self.fail_streak.entry(key.clone()).or_insert(0);
        *streak = (*streak).saturating_add(1);
        let pow = (*streak).min(10);
        let secs = 5u64.saturating_mul(1u64 << pow).min(3600);
        self.failed_until
            .insert(key, Instant::now() + Duration::from_secs(secs));
    }

    fn clear_url_failure(&mut self, url: &str) {
        let key = normalize_dapp_usage_key(url);
        self.fail_streak.remove(&key);
        self.failed_until.remove(&key);
    }

    fn url_in_backoff(&self, url: &str) -> bool {
        let key = normalize_dapp_usage_key(url);
        self.failed_until
            .get(&key)
            .map(|t| Instant::now() < *t)
            .unwrap_or(false)
    }

    fn prune_expired_backoffs(&mut self) {
        let now = Instant::now();
        self.failed_until.retain(|_, until| *until > now);
    }
}

static WARM_POOL: OnceLock<Mutex<WarmPool>> = OnceLock::new();

fn warm_pool_mutex() -> &'static Mutex<WarmPool> {
    WARM_POOL.get_or_init(|| {
        Mutex::new(WarmPool {
            slots: (0..WARM_SLOT_CAP).map(|_| WarmSlotPhase::Empty).collect(),
            failed_until: HashMap::new(),
            fail_streak: HashMap::new(),
        })
    })
}

fn warm_pool_reset_all_empty() {
    if let Ok(mut p) = warm_pool_mutex().lock() {
        for s in &mut p.slots {
            *s = WarmSlotPhase::Empty;
        }
        p.failed_until.clear();
        p.fail_streak.clear();
    }
}

fn warm_pool_apply_child_event(event: &str, data: &serde_json::Value) {
    let Some(slot_id) = data.get("slot_id").and_then(|v| v.as_u64()).map(|v| v as usize) else {
        return;
    };
    if slot_id >= WARM_SLOT_CAP {
        return;
    }
    let cap = effective_warm_slot_cap();
    if slot_id >= cap {
        return;
    }
    let Ok(mut pool) = warm_pool_mutex().lock() else {
        return;
    };
    match event {
        "slot_loaded" => {
            let success = data.get("success").and_then(|v| v.as_bool()).unwrap_or(false);
            let Some(url) = data.get("url").and_then(|s| s.as_str()).map(|s| s.to_string()) else {
                return;
            };
            if success {
                pool.clear_url_failure(&url);
                warm_pool_set_slot(&mut pool, slot_id, WarmSlotPhase::Ready { url: url.clone() });
            } else {
                pool.record_url_failure(&url);
                warm_pool_set_slot(&mut pool, slot_id, WarmSlotPhase::Empty);
            }
        }
        // Legacy: child used to emit this immediately after navigate; still accept.
        "slot_ready" => {
            if let Some(url) = data.get("url").and_then(|s| s.as_str()).map(|s| s.to_string()) {
                pool.clear_url_failure(&url);
                warm_pool_set_slot(&mut pool, slot_id, WarmSlotPhase::Ready { url: url.clone() });
            }
        }
        "slot_claimed" => {
            let url = match &pool.slots[slot_id] {
                WarmSlotPhase::Ready { url } | WarmSlotPhase::Claimed { url } => url.clone(),
                _ => return,
            };
            warm_pool_set_slot(&mut pool, slot_id, WarmSlotPhase::Claimed { url });
        }
        "slot_hidden" => {
            let next = match &pool.slots[slot_id] {
                WarmSlotPhase::Claimed { url } => WarmSlotPhase::Refilling { url: url.clone() },
                _ => WarmSlotPhase::Empty,
            };
            pool.slots[slot_id] = next;
        }
        "slot_destroyed" => {
            warm_pool_set_slot(&mut pool, slot_id, WarmSlotPhase::Empty);
        }
        "slot_crashed" => {
            let url_for_fail = match &pool.slots[slot_id] {
                WarmSlotPhase::Warming { url, .. }
                | WarmSlotPhase::Ready { url }
                | WarmSlotPhase::Claimed { url }
                | WarmSlotPhase::Refilling { url } => Some(url.clone()),
                WarmSlotPhase::Empty => None,
            };
            if let Some(u) = url_for_fail {
                pool.record_url_failure(&u);
            }
            warm_pool_set_slot(&mut pool, slot_id, WarmSlotPhase::Empty);
        }
        _ => {}
    }
}

fn warm_pool_ready_slot_for_url(full_url: &str) -> Option<u8> {
    let key = normalize_dapp_usage_key(full_url);
    let pool = warm_pool_mutex().lock().ok()?;
    let cap = effective_warm_slot_cap();
    for (i, phase) in pool.slots.iter().enumerate().take(cap) {
        if let WarmSlotPhase::Ready { url } = phase {
            if normalize_dapp_usage_key(url) == key {
                return Some(i as u8);
            }
        }
    }
    None
}

fn warm_pool_reconcile_tick(inner: &mut BrowserInner) {
    if !multi_warm_pool_env_enabled() {
        return;
    }
    let cap = effective_warm_slot_cap();
    maybe_recover_stale_dapp_browser_heartbeat(inner, cap);

    let candidates = LAST_PREWARM_CANDIDATES
        .get_or_init(|| Mutex::new(Vec::new()))
        .lock()
        .map(|c| c.clone())
        .unwrap_or_default();

    let Ok(mut pool) = warm_pool_mutex().lock() else {
        return;
    };
    pool.prune_expired_backoffs();
    // Heavy dApps can legitimately take >30s to reach first Finished load event while hidden.
    // Shorter timeouts cause destructive clear/recreate loops (`reconcile_clear_warming_timeout`).
    let warm_timeout = Duration::from_secs(90);

    for slot_id in 0..cap {
        let id = slot_id as u8;
        let desired = candidates.get(slot_id).cloned();
        let phase = pool.slots[slot_id].clone();
        match phase {
            WarmSlotPhase::Empty => {
                if let Some(url) = desired {
                    if validate_whitelisted_dapp_url(&url).is_ok()
                        && !pool.url_in_backoff(&url)
                        && inner.try_send_warm_slot_create(id).is_ok()
                        && inner
                            .try_send_warm_slot_navigate_hidden(id, &url)
                            .is_ok()
                    {
                        warm_pool_set_slot(
                            &mut pool,
                            slot_id,
                            WarmSlotPhase::Warming {
                                url,
                                since: Instant::now(),
                            },
                        );
                    }
                }
            }
            WarmSlotPhase::Warming { ref url, since } => {
                if since.elapsed() > warm_timeout {
                    pool.record_url_failure(url);
                    let _ = inner.try_send_warm_slot_destroy(id);
                    warm_pool_set_slot(&mut pool, slot_id, WarmSlotPhase::Empty);
                } else if let Some(ref d) = desired {
                    if normalize_dapp_usage_key(d) != normalize_dapp_usage_key(url) {
                        let _ = inner.try_send_warm_slot_destroy(id);
                        warm_pool_set_slot(&mut pool, slot_id, WarmSlotPhase::Empty);
                    }
                } else {
                    let _ = inner.try_send_warm_slot_destroy(id);
                    warm_pool_set_slot(&mut pool, slot_id, WarmSlotPhase::Empty);
                }
            }
            WarmSlotPhase::Ready { ref url } => {
                if let Some(ref d) = desired {
                    if normalize_dapp_usage_key(d) != normalize_dapp_usage_key(url) {
                        let _ = inner.try_send_warm_slot_destroy(id);
                        warm_pool_set_slot(&mut pool, slot_id, WarmSlotPhase::Empty);
                    }
                } else {
                    let _ = inner.try_send_warm_slot_destroy(id);
                    warm_pool_set_slot(&mut pool, slot_id, WarmSlotPhase::Empty);
                }
            }
            WarmSlotPhase::Claimed { .. } => {}
            WarmSlotPhase::Refilling { ref url } => {
                if let Some(ref d) = desired {
                    let desired_key = normalize_dapp_usage_key(d);
                    let desired_already_warming = pool
                        .slots
                        .iter()
                        .enumerate()
                        .any(|(idx, phase)| {
                            if idx == slot_id {
                                return false;
                            }
                            match phase {
                                WarmSlotPhase::Warming { url, .. }
                                | WarmSlotPhase::Ready { url }
                                | WarmSlotPhase::Refilling { url } => {
                                    normalize_dapp_usage_key(url) == desired_key
                                }
                                WarmSlotPhase::Empty | WarmSlotPhase::Claimed { .. } => false,
                            }
                        });
                    if normalize_dapp_usage_key(d) == normalize_dapp_usage_key(url)
                        && !desired_already_warming
                        && validate_whitelisted_dapp_url(d).is_ok()
                        && !pool.url_in_backoff(d)
                        && inner.try_send_warm_slot_create(id).is_ok()
                        && inner.try_send_warm_slot_navigate_hidden(id, d).is_ok()
                    {
                        warm_pool_set_slot(
                            &mut pool,
                            slot_id,
                            WarmSlotPhase::Warming {
                                url: d.clone(),
                                since: Instant::now(),
                            },
                        );
                    } else {
                        warm_pool_set_slot(&mut pool, slot_id, WarmSlotPhase::Empty);
                    }
                } else {
                    warm_pool_set_slot(&mut pool, slot_id, WarmSlotPhase::Empty);
                }
            }
        }
    }
    drop(pool);

    if let Ok(mut m) = LAST_PREWARM_SLOT_BY_KEY
        .get_or_init(|| Mutex::new(HashMap::new()))
        .lock()
    {
        m.clear();
        for (i, u) in candidates.iter().enumerate().take(cap) {
            m.insert(normalize_dapp_usage_key(u), i as u8);
        }
    }
}

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
    let mut selected: Vec<String> = TRUSTED_DAPP_ENTRIES
        .iter()
        .filter(|e| trusted_dapp_visible_on_chain(e, active_chain_id))
        .filter_map(|e| validate_whitelisted_dapp_url(e.url).ok())
        .filter(|url| {
            let key = normalize_dapp_usage_key(url);
            fast_keys.contains(&key)
        })
        .take(limit)
        .collect();
    if selected.is_empty() {
        selected = TRUSTED_DAPP_ENTRIES
            .iter()
            .filter(|e| trusted_dapp_visible_on_chain(e, active_chain_id))
            .filter_map(|e| validate_whitelisted_dapp_url(e.url).ok())
            .take(limit)
            .collect();
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

pub fn prewarm_top_trusted_dapps_for_chain(limit: usize, active_chain_id: u64) {
    let capped = limit.clamp(1, 6);
    let candidates = compute_top_trusted_candidates_for_chain(capped, active_chain_id);
    prewarm_candidate_urls(candidates);
}

fn prewarm_candidate_urls(candidates: Vec<String>) {
    if candidates.is_empty() {
        return;
    }
    if let Ok(mut last) = LAST_PREWARM_CANDIDATES
        .get_or_init(|| Mutex::new(Vec::new()))
        .lock()
    {
        *last = candidates.clone();
    }
    thread::spawn(move || {
        let mut join_tcp = Vec::with_capacity(candidates.len());
        for url in &candidates {
            let url = url.clone();
            join_tcp.push(thread::spawn(move || preconnect_dapp_origin(&url)));
        }
        for h in join_tcp {
            let _ = h.join();
        }

        // Warm the WebKit document cache for rocket-selected picks while hidden.
        let _ = wallet_ipc::wait_dapp_browser_ipc_handshake(Duration::from_secs(4));
        if multi_warm_pool_env_enabled() {
            // Fast-start: kick slot create+navigate immediately for current candidates instead of
            // waiting for the next monitor reconcile tick.
            try_kick_multiwarm_candidates_now(&candidates);
            return;
        }
        let warmed = try_hidden_prewarm_candidates(&candidates);
        if !warmed {
            schedule_hidden_rewarm_retry_worker();
        }
    });
}

fn try_kick_multiwarm_candidates_now(candidates: &[String]) -> usize {
    let Some(state) = BROWSER_STATE.get() else {
        return 0;
    };
    let Ok(mut inner) = state.lock() else {
        return 0;
    };
    if inner.child.is_none() {
        let _ = inner.spawn(None);
    }
    if inner.child.is_none() || inner.control_stdin.is_none() {
        return 0;
    }
    let cap = effective_warm_slot_cap().min(WARM_SLOT_CAP);
    let mut kicked = 0usize;
    for (slot_id, url) in candidates.iter().take(cap).enumerate() {
        if validate_whitelisted_dapp_url(url).is_err() {
            continue;
        }
        if inner.try_send_warm_slot_create(slot_id as u8).is_ok()
            && inner
                .try_send_warm_slot_navigate_hidden(slot_id as u8, url)
                .is_ok()
        {
            kicked += 1;
        }
    }
    kicked
}

fn try_hidden_prewarm_candidates(candidates: &[String]) -> bool {
    let Some(state) = BROWSER_STATE.get() else {
        return false;
    };
    let mut warmed_any = false;
    for url in candidates {
        if let Ok(mut inner) = state.lock() {
            if inner.try_send_navigate_trusted(url, false).is_ok() {
                warmed_any = true;
            }
        }
        thread::sleep(Duration::from_millis(325));
    }
    warmed_any
}

fn schedule_hidden_rewarm_retry_worker() {
    if HIDDEN_REWARM_RETRY_RUNNING.swap(true, Ordering::AcqRel) {
        return;
    }
    thread::spawn(|| {
        for _ in 0..120 {
            let candidates = LAST_PREWARM_CANDIDATES
                .get_or_init(|| Mutex::new(Vec::new()))
                .lock()
                .map(|c| c.clone())
                .unwrap_or_default();
            if candidates.is_empty() {
                break;
            }
            if try_hidden_prewarm_candidates(&candidates) {
                break;
            }
            thread::sleep(Duration::from_secs(2));
        }
        HIDDEN_REWARM_RETRY_RUNNING.store(false, Ordering::Release);
    });
}

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
    fn try_send_control_line(&mut self, value: serde_json::Value) -> Result<(), String> {
        if !self.child_is_alive() {
            return Err("dApp browser process is not running".to_string());
        }
        let Some(stdin) = self.control_stdin.as_mut() else {
            return Err(
                "dApp browser has no control stdin (warm process may need a cold restart)".to_string(),
            );
        };
        let line = value.to_string();
        writeln!(stdin, "{line}").map_err(|e| e.to_string())?;
        stdin.flush().map_err(|e| e.to_string())?;
        Ok(())
    }

    /// Check whether the tracked browser child is still running. Reaps exited children so we don't
    /// hold a waited-on `Child` handle. Does **not** spawn a replacement — that is the monitor
    /// thread's job so the new process has time to initialise before the next dApp click.
    fn child_is_alive(&mut self) -> bool {
        let Some(mut c) = self.child.take() else {
            return false;
        };
        match c.try_wait() {
            Ok(None) => {
                self.child = Some(c);
                true
            }
            Ok(Some(status)) => {
                self.control_stdin = None;
                if status.success() {
                    self.last_url = None;
                }
                false
            }
            Err(e) => {
                // Do **not** drop `control_stdin` here: `try_wait` can fail transiently (e.g. EINTR).
                // Clearing the pipe while the child is still running made the next `try_send_navigate_trusted`
                // fail and forced a cold `--url` respawn — slow first dApp and killed the warm shell.
                tracing::debug!(
                    target: "vaughan_browser",
                    err = %e,
                    "dApp browser try_wait() error; assuming child still running"
                );
                self.child = Some(c);
                true
            }
        }
    }

    /// Sends a navigate command to a running warm browser. Fails if the process exited or the pipe broke.
    fn try_send_navigate_trusted(&mut self, url: &str, reveal: bool) -> Result<(), String> {
        let payload = if reveal {
            serde_json::json!({ "navigate_trusted": url })
        } else {
            serde_json::json!({ "navigate_trusted": url, "reveal": false })
        };
        self.try_send_control_line(payload)
    }

    fn try_send_warm_slot_navigate_hidden(&mut self, slot_id: u8, url: &str) -> Result<(), String> {
        self.try_send_control_line(serde_json::json!({
            "cmd": "navigate",
            "id": slot_id,
            "url": url,
        }))
    }

    fn try_send_warm_slot_create(&mut self, slot_id: u8) -> Result<(), String> {
        self.try_send_control_line(serde_json::json!({
            "cmd": "create_webview",
            "id": slot_id
        }))
    }

    fn try_send_warm_slot_show(&mut self, slot_id: u8) -> Result<(), String> {
        self.try_send_control_line(serde_json::json!({
            "cmd": "show",
            "id": slot_id
        }))
    }

    fn try_send_warm_slot_destroy(&mut self, slot_id: u8) -> Result<(), String> {
        self.try_send_control_line(serde_json::json!({
            "cmd": "destroy",
            "id": slot_id
        }))
    }

    /// Wallet spawn: piped stdin + env markers. Warm shell (`url` None) starts hidden until first navigate.
    fn spawn(&mut self, url: Option<&str>) -> Result<(), String> {
        wallet_ipc::reset_dapp_browser_ipc_handshake_gate();
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
            .stdout(Stdio::piped())
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
        if let Some(stdout) = child.stdout.take() {
            std::thread::Builder::new()
                .name("vaughan-browser-events".into())
                .spawn(move || {
                    let mut reader = BufReader::new(stdout);
                    let mut line = String::new();
                    loop {
                        line.clear();
                        let Ok(n) = reader.read_line(&mut line) else {
                            break;
                        };
                        if n == 0 {
                            break;
                        }
                        let t = line.trim();
                        if t.is_empty() {
                            continue;
                        }
                        let Ok(ev) = serde_json::from_str::<serde_json::Value>(t) else {
                            continue;
                        };
                        let Some(event_name) = ev.get("event").and_then(|v| v.as_str()) else {
                            continue;
                        };
                        let data = ev
                            .get("data")
                            .cloned()
                            .unwrap_or_else(|| serde_json::json!({}));
                        // Any valid child event proves the process/output loop is still alive.
                        touch_dapp_browser_heartbeat();
                        match event_name {
                            "slot_ready" | "slot_loaded" | "slot_claimed" | "slot_hidden"
                            | "slot_destroyed" | "slot_crashed" => {
                                warm_pool_apply_child_event(event_name, &data);
                            }
                            "heartbeat" | "ready" => {}
                            _ => {}
                        }
                    }
                })
                .ok();
        }
        self.child = Some(child);
        if let Ok(mut hb) = DAPP_BROWSER_HEARTBEAT_AT.lock() {
            *hb = Some(Instant::now());
        }
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

/// Opens a trusted dApp by spawning a fresh browser child process (window).
/// This is the only open path used by the UI today; every click gets its own window
/// and its own WebKit process.
pub fn open_trusted_dapp_url_new_window(url_str: &str) -> Result<(), String> {
    let full = validate_whitelisted_dapp_url(url_str)?;
    let boot = WALLET_DAPP_BOOT
        .get()
        .ok_or("Wallet IPC is not running; restart the wallet.")?;
    let bin = resolve_browser_executable().ok_or_else(|| {
        "dApp browser not found. From the repo root run:\n  cargo build -p vaughan-tauri-browser\n\
         (or build the whole workspace), then click again."
            .to_string()
    })?;

    let mut cmd = Command::new(&bin);
    let warmup_remaining_secs = warmup_hint_remaining_secs();
    let is_rocket = is_fast_dapp_selected(&full);
    cmd.env("VAUGHAN_WALLET_SPAWNED", "1")
        .env(
            "VAUGHAN_WARMUP_HINT_REMAINING_SECS",
            warmup_remaining_secs.to_string(),
        )
        .env("VAUGHAN_WARMUP_HINT_IS_ROCKET", if is_rocket { "1" } else { "0" })
        .arg("--ipc")
        .arg(&boot.endpoint)
        .arg("--token")
        .arg(&boot.token)
        .arg("--url")
        .arg(&full)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::inherit());

    let child = cmd.spawn().map_err(|e| e.to_string())?;
    let extras = EXTRA_BROWSER_CHILDREN.get_or_init(|| Mutex::new(Vec::new()));
    if let Ok(mut children) = extras.lock() {
        children.retain_mut(|c| matches!(c.try_wait(), Ok(None) | Err(_)));
        children.push(child);
    }
    Ok(())
}

/// Opens a trusted dApp preferring an already-warmed slot window when available.
/// Falls back to spawning a fresh new window when no warm-ready slot can be shown.
pub fn open_trusted_dapp_url_prefer_warm_window(url_str: &str) -> Result<(), String> {
    let full = validate_whitelisted_dapp_url(url_str)?;
    if !multi_warm_pool_env_enabled() {
        return open_trusted_dapp_url_new_window(&full);
    }

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
    let Some(state) = BROWSER_STATE.get() else {
        return open_trusted_dapp_url_new_window(&full);
    };
    let Ok(mut inner) = state.lock() else {
        return open_trusted_dapp_url_new_window(&full);
    };
    inner.bin = bin;
    if inner.child.is_none() {
        let _ = inner.spawn(None);
    }
    let mapped_slot = LAST_PREWARM_SLOT_BY_KEY
        .get_or_init(|| Mutex::new(HashMap::new()))
        .lock()
        .ok()
        .and_then(|m| m.get(&normalize_dapp_usage_key(&full)).copied());
    let Some(slot_id) = warm_pool_ready_slot_for_url(&full) else {
        // Kick an immediate warm attempt for this mapped rocket URL so the next click
        // can hit a ready slot even if this click falls back to cold open.
        if let Some(id) = mapped_slot {
            let _ = inner.try_send_warm_slot_create(id);
            let _ = inner.try_send_warm_slot_navigate_hidden(id, &full);
        }
        return open_trusted_dapp_url_new_window(&full);
    };
    match inner.try_send_warm_slot_show(slot_id) {
        Ok(()) => {
            inner.last_url = Some(full);
            Ok(())
        }
        Err(_) => {
            let _ = inner.try_send_warm_slot_destroy(slot_id);
            open_trusted_dapp_url_new_window(url_str)
        }
    }
}

/// Starts wallet IPC for the dApp browser and optionally **warms** a hidden browser process (shell only)
/// so the first dApp open avoids process + WebKit cold start.
/// On drop, stops the health monitor and terminates any running browser child.
pub struct BrowserProcessGuard {
    /// Holds the wallet IPC server until this guard drops (stops the accept loop).
    ipc_server: Option<WalletIpcServer>,
    browser_monitor_stop: Arc<AtomicBool>,
    browser_monitor: Option<thread::JoinHandle<()>>,
}

impl BrowserProcessGuard {
    pub fn launch_if_available(services: AppServices) -> Self {
        let _ = DAPP_WARMUP_STARTED_AT.get_or_init(Instant::now);
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
                tracing::error!(
                    target: "vaughan_browser",
                    err = %err,
                    "failed to start wallet IPC server"
                );
                None
            }
        };

        let browser_bin = resolve_browser_executable();
        if browser_bin.is_none() {
            tracing::warn!(
                target: "vaughan_browser",
                "dApp browser executable not found (expected next to the wallet or under target/debug). \
                 Build it with: cargo build -p vaughan-tauri-browser"
            );
        }

        if ipc_server.is_some() && warm_dapp_browser_env_enabled() {
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
                                Ok(()) => {
                                    tracing::info!(
                                        target: "vaughan_browser",
                                        "dApp browser warm process started (hidden until first trusted dApp)"
                                    );
                                }
                                Err(e) => {
                                    tracing::error!(
                                        target: "vaughan_browser",
                                        err = %e,
                                        "dApp browser warm spawn failed"
                                    );
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
                    // Keep this fairly tight so warm slots converge to Ready quickly.
                    thread::sleep(Duration::from_millis(250));
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
                                if let Ok(mut hb) = DAPP_BROWSER_HEARTBEAT_AT.lock() {
                                    *hb = None;
                                }
                                if status.success() {
                                    inner.last_url = None;
                                    warm_pool_reset_all_empty();
                                    if warm_dapp_browser_env_enabled() && inner.bin.exists() {
                                        let _ = inner.spawn(None);
                                    }
                                }
                            }
                            Ok(None) => {
                                inner.child = Some(c);
                            }
                            Err(_) => {
                                // Keep tracking the child if `try_wait` failed transiently.
                                inner.child = Some(c);
                            }
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
                    if multi_warm_pool_env_enabled()
                        && inner.child.is_some()
                        && inner.control_stdin.is_some()
                    {
                        warm_pool_reconcile_tick(&mut inner);
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
        if let Some(extras) = EXTRA_BROWSER_CHILDREN.get() {
            if let Ok(mut children) = extras.lock() {
                for mut c in children.drain(..) {
                    let _ = c.kill();
                    let _ = c.wait();
                }
            }
        }
        drop(self.ipc_server.take());
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
