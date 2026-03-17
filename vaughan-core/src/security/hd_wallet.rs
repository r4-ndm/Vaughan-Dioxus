//! HD Wallet (BIP-39 + BIP-32 + BIP-44) for EVM accounts.
//!
//! Derivation path (Ethereum): `m/44'/60'/0'/0/{index}`

use crate::error::WalletError;
use alloy::signers::local::PrivateKeySigner;
use bip39::{Language, Mnemonic};
use coins_bip32::{path::DerivationPath, prelude::XPriv};
use rand::{rngs::OsRng, RngCore};
use std::str::FromStr;

/// Generate a BIP-39 mnemonic phrase.
///
/// Supports 12, 15, 18, 21, or 24 words.
pub fn generate_mnemonic(word_count: usize) -> Result<String, WalletError> {
    let entropy_size = match word_count {
        12 => 16,
        15 => 20,
        18 => 24,
        21 => 28,
        24 => 32,
        _ => {
            return Err(WalletError::InvalidMnemonic(
                "Word count must be 12, 15, 18, 21, or 24".to_string(),
            ))
        }
    };

    let mut entropy = vec![0u8; entropy_size];
    OsRng.fill_bytes(&mut entropy);

    let mnemonic = Mnemonic::from_entropy_in(Language::English, &entropy)
        .map_err(|e| WalletError::InvalidMnemonic(format!("Mnemonic generation failed: {}", e)))?;

    Ok(mnemonic.to_string())
}

/// Validate a BIP-39 mnemonic.
pub fn validate_mnemonic(mnemonic: &str) -> Result<(), WalletError> {
    Mnemonic::from_str(mnemonic)
        .map_err(|e| WalletError::InvalidMnemonic(format!("Invalid mnemonic: {}", e)))?;
    Ok(())
}

/// Convert mnemonic phrase to a 64-byte seed (BIP-39).
pub fn mnemonic_to_seed(mnemonic: &str, passphrase: Option<&str>) -> Result<Vec<u8>, WalletError> {
    let mnemonic = Mnemonic::from_str(mnemonic)
        .map_err(|e| WalletError::InvalidMnemonic(format!("Invalid mnemonic: {}", e)))?;
    Ok(mnemonic.to_seed(passphrase.unwrap_or("")).to_vec())
}

/// Derive an EVM account signer from the master seed at `m/44'/60'/0'/0/{index}`.
pub fn derive_account(seed: &[u8], index: u32) -> Result<PrivateKeySigner, WalletError> {
    let master_key = XPriv::root_from_seed(seed, None)
        .map_err(|e| WalletError::KeyDerivationFailed(format!("Master key creation failed: {}", e)))?;

    let path = format!("m/44'/60'/0'/0/{}", index);
    let derivation_path = DerivationPath::from_str(&path)
        .map_err(|e| WalletError::KeyDerivationFailed(format!("Invalid derivation path: {}", e)))?;

    let derived_key = master_key
        .derive_path(&derivation_path)
        .map_err(|e| WalletError::KeyDerivationFailed(format!("Key derivation failed: {}", e)))?;

    use coins_bip32::ecdsa::SigningKey;
    let signing_key: &SigningKey = derived_key.as_ref();
    let private_key_bytes = signing_key.to_bytes();
    let private_key_hex = hex::encode(&private_key_bytes[..]);

    PrivateKeySigner::from_str(&private_key_hex)
        .map_err(|e| WalletError::KeyDerivationFailed(format!("Signer creation failed: {}", e)))
}

/// Derive multiple EVM account signers from a seed.
pub fn derive_accounts(seed: &[u8], count: u32) -> Result<Vec<PrivateKeySigner>, WalletError> {
    let mut out = Vec::with_capacity(count as usize);
    for idx in 0..count {
        out.push(derive_account(seed, idx)?);
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generate_12_words() {
        let m = generate_mnemonic(12).unwrap();
        assert_eq!(m.split_whitespace().count(), 12);
        validate_mnemonic(&m).unwrap();
    }

    #[test]
    fn seed_and_derive_signer() {
        let m = generate_mnemonic(12).unwrap();
        let seed = mnemonic_to_seed(&m, None).unwrap();
        let signer0 = derive_account(&seed, 0).unwrap();
        let signer1 = derive_account(&seed, 1).unwrap();
        // Deterministic and distinct
        assert_ne!(format!("{:?}", signer0.address()), format!("{:?}", signer1.address()));
    }
}
