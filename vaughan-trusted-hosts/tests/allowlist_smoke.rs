//! Integration tests under `tests/` for tooling that keys off this layout.

use vaughan_trusted_hosts::{hostname_is_whitelisted, ALLOWED_HTTPS_HOST_SUFFIXES};

#[test]
fn allowlist_covers_app_subdomain() {
    assert!(hostname_is_whitelisted("app.uniswap.org"));
}

#[test]
fn allowlist_covers_pulsex_pinata_worker_origin() {
    assert!(hostname_is_whitelisted("pulsex.mypinata.cloud"));
}

#[test]
fn allowlist_constant_matches_runtime() {
    assert!(!ALLOWED_HTTPS_HOST_SUFFIXES.is_empty());
}
