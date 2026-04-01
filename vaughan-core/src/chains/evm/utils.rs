//! EVM-specific helpers.
//!
//! Task 3.10: address validation and formatting utilities.

use alloy::primitives::Address;
use std::str::FromStr;

use crate::error::WalletError;

/// Parse and validate an EVM address.
pub fn parse_address(address: &str) -> Result<Address, WalletError> {
    Address::from_str(address).map_err(|_| WalletError::InvalidAddress(address.to_string()))
}

/// Returns `true` if `address` is a valid EVM address string.
pub fn is_valid_address(address: &str) -> bool {
    parse_address(address).is_ok()
}

/// Truncate an address for UI display, e.g. `0x1234...abcd`.
pub fn truncate_address(address: &str, prefix_len: usize, suffix_len: usize) -> String {
    if address.len() <= prefix_len + suffix_len + 3 {
        return address.to_string();
    }
    let prefix = &address[..prefix_len];
    let suffix = &address[address.len() - suffix_len..];
    format!("{prefix}...{suffix}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_is_valid_address() {
        assert!(is_valid_address(
            "0xd8dA6BF26964aF9D7eEd9e03E53415D37aA96045"
        ));
        assert!(!is_valid_address("invalid"));
        assert!(!is_valid_address("0xinvalid"));
    }

    #[test]
    fn test_truncate_address() {
        let addr = "0xd8dA6BF26964aF9D7eEd9e03E53415D37aA96045";
        assert_eq!(truncate_address(addr, 6, 4), "0xd8dA...6045");
    }
}
