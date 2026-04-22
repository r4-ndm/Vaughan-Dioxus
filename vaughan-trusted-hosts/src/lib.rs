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
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    const PROVIDER_JS: &str = include_str!("../../vaughan-tauri-browser/provider_inject.js");
    const INDEX_HTML: &str = include_str!("../../vaughan-tauri-browser/index.html");
    const CAP_DEFAULT_JSON: &str =
        include_str!("../../vaughan-tauri-browser/capabilities/default.json");

    #[test]
    fn rust_allowlist_entries_appear_in_provider_inject_js() {
        for s in ALLOWED_HTTPS_HOST_SUFFIXES {
            let needle = format!("\"{s}\"");
            assert!(
                PROVIDER_JS.contains(&needle),
                "provider_inject.js must list {needle:?} (keep in sync with vaughan-trusted-hosts)"
            );
        }
    }

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
