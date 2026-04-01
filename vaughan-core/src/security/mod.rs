//! Security: keyring, encryption, HD wallet (BIP-39/BIP-32).

pub mod encryption;
pub mod hd_wallet;
pub mod keyring_service;
pub mod rate_limit;

pub use encryption::{
    decrypt_data, encrypt_data, hash_password, validate_password, verify_password,
    PASSWORD_POLICY_DESCRIPTION,
};
pub use hd_wallet::{
    derive_account, derive_accounts, generate_mnemonic, mnemonic_to_seed, validate_mnemonic,
};
pub use keyring_service::KeyringService;
pub use rate_limit::AuthRateLimiter;
