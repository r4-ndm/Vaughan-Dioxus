//! Canonical HTTPS host suffix allowlist for trusted dApp navigation (wallet + Tauri browser).
//!
//! **Browser parity:** [`ALLOWED_HTTPS_HOST_SUFFIXES`] is mirrored in
//! `vaughan-tauri-browser/provider_inject.js` (`TRUSTED_HOST_SUFFIXES`) and
//! `vaughan-tauri-browser/index.html` (`trustedWalletBridgeOrigin`). The Tauri
//! browser capability allowlist in `capabilities/default.json` must also stay in
//! sync. Unit tests assert every Rust entry appears in all three files.

/// HTTPS host suffixes: exact match or subdomain (`app.uniswap.org` → `uniswap.org`).
pub const ALLOWED_HTTPS_HOST_SUFFIXES: &[&str] = &[
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
    // PulseX static / worker chunks (IPFS gateway) used alongside `*.pulsex.com`.
    "pulsex.mypinata.cloud",
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

use std::collections::HashSet;
use std::sync::{OnceLock, RwLock};
use url::Url;

static CUSTOM_ALLOWED_HOSTS: OnceLock<RwLock<HashSet<String>>> = OnceLock::new();

/// Add a host dynamically to the dynamic allowed hosts registry.
pub fn add_custom_allowed_host(host: String) {
    let lock = CUSTOM_ALLOWED_HOSTS.get_or_init(|| RwLock::new(HashSet::new()));
    if let Ok(mut write_guard) = lock.write() {
        write_guard.insert(host.trim().to_lowercase());
    }
}

/// Remove a host dynamically from the dynamic allowed hosts registry.
pub fn remove_custom_allowed_host(host: &str) {
    let lock = CUSTOM_ALLOWED_HOSTS.get_or_init(|| RwLock::new(HashSet::new()));
    if let Ok(mut write_guard) = lock.write() {
        write_guard.remove(&host.trim().to_lowercase());
    }
}


/// Reset the entire set of dynamically whitelisted hostnames.
pub fn reset_custom_allowed_hosts(hosts: Vec<String>) {
    let lock = CUSTOM_ALLOWED_HOSTS.get_or_init(|| RwLock::new(HashSet::new()));
    if let Ok(mut write_guard) = lock.write() {
        write_guard.clear();
        for h in hosts {
            write_guard.insert(h.trim().to_lowercase());
        }
    }
}

/// Retrieve the current set of dynamically whitelisted hostnames.
pub fn get_custom_allowed_hosts() -> HashSet<String> {
    let lock = CUSTOM_ALLOWED_HOSTS.get_or_init(|| RwLock::new(HashSet::new()));
    if let Ok(read_guard) = lock.read() {
        read_guard.clone()
    } else {
        HashSet::new()
    }
}

/// True if `host` is `localhost` / `127.0.0.1` or matches an allowlisted HTTPS suffix.
pub fn hostname_is_whitelisted(host: &str) -> bool {
    let h = host.trim().trim_end_matches('.').to_lowercase();
    if matches!(h.as_str(), "localhost" | "127.0.0.1") {
        return true;
    }
    for suffix in ALLOWED_HTTPS_HOST_SUFFIXES {
        if h == *suffix || h.ends_with(&format!(".{suffix}")) {
            return true;
        }
    }
    // Check dynamic whitelisted hosts as well.
    let custom = get_custom_allowed_hosts();
    for suffix in &custom {
        if h == *suffix || h.ends_with(&format!(".{suffix}")) {
            return true;
        }
    }
    false
}

/// Validate a URL for trusted dApp navigation (wallet + Tauri browser parity).
///
/// Allows **https** on allowlisted hosts, or **http** on localhost / 127.0.0.1 only.
pub fn validate_navigation_url(url_str: &str) -> Result<String, String> {
    let u = Url::parse(url_str.trim()).map_err(|e| e.to_string())?;
    validate_parsed_navigation_url(&u)?;
    Ok(u.to_string())
}

/// Parse and validate a navigation URL, returning the parsed [`Url`].
pub fn parse_navigation_url(url_str: &str) -> Result<Url, String> {
    let u = Url::parse(url_str.trim()).map_err(|e| e.to_string())?;
    validate_parsed_navigation_url(&u)?;
    Ok(u)
}

fn validate_parsed_navigation_url(u: &Url) -> Result<(), String> {
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
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    const INDEX_HTML: &str = include_str!("../../vaughan-tauri-browser/index.html");
    const CAP_DEFAULT_JSON: &str =
        include_str!("../../vaughan-tauri-browser/capabilities/default.json");

    #[test]
    fn rust_allowlist_entries_appear_in_index_html() {
        for s in ALLOWED_HTTPS_HOST_SUFFIXES {
            let needle = format!("\"{s}\"");
            assert!(
                INDEX_HTML.contains(&needle),
                "index.html must list {needle:?} (keep in sync with vaughan-trusted-hosts)"
            );
        }
    }

    #[test]
    fn rust_allowlist_entries_appear_in_capabilities_default_json() {
        for s in ALLOWED_HTTPS_HOST_SUFFIXES {
            let exact = format!("\"https://{s}/*\"");
            let sub = format!("\"https://*.{s}/*\"");
            assert!(
                CAP_DEFAULT_JSON.contains(&exact),
                "capabilities/default.json must list {exact:?} (keep in sync with vaughan-trusted-hosts)"
            );
            assert!(
                CAP_DEFAULT_JSON.contains(&sub),
                "capabilities/default.json must list {sub:?} (keep in sync with vaughan-trusted-hosts)"
            );
        }
    }
}
