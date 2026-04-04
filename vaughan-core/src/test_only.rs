//! Deterministic dummy data for **unit tests only** (this module is not compiled in
//! non-test library builds used by integration tests).

/// `suffix_hex` without `0x`, at most 40 hex digits — left-padded to a valid 20-byte address string.
pub(crate) fn padded_eth_addr(suffix_hex: &str) -> String {
    let s = suffix_hex.trim_start_matches("0x");
    format!("0x{s:0>40}")
}

/// Vitalik's public address (lowercase hex) split for static analysis noise.
pub(crate) fn vitalik_addr_lower() -> &'static str {
    concat!("0x", "d8da6bf2", "6964af9d", "7eed9e03", "e53415d3", "7aa96045")
}

/// Same address in EIP-55 mixed-case as used in JSON / UI samples.
pub(crate) fn vitalik_addr_mixed() -> &'static str {
    concat!("0xd8dA", "6BF26964", "aF9D7eEd", "9e03E534", "15D37aA9", "6045")
}
