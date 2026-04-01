//! OS keychain integration (defense-in-depth).
//!
//! We store encrypted secrets in the OS keychain:
//! - Windows Credential Manager
//! - macOS Keychain
//! - Linux Secret Service (where available)
//!
//! Secrets are encrypted with the user's password before being stored.

use crate::error::WalletError;
use crate::security::encryption::{decrypt_data, encrypt_data};
use base64::Engine;
use keyring::Entry;
use secrecy::Secret;

/// Keyring-backed secret storage.
pub struct KeyringService {
    service_name: String,
}

impl KeyringService {
    pub fn new(service_name: impl Into<String>) -> Result<Self, WalletError> {
        Ok(Self {
            service_name: service_name.into(),
        })
    }

    /// Store an encrypted secret value under `key_id`.
    pub fn store_secret(
        &self,
        key_id: &str,
        secret: &str,
        password: &str,
    ) -> Result<(), WalletError> {
        let encrypted = encrypt_data(secret.as_bytes(), password)?;
        let encoded = base64::engine::general_purpose::STANDARD.encode(&encrypted);

        let entry = Entry::new(&self.service_name, key_id).map_err(|e| {
            WalletError::KeyringError(format!("Keyring entry creation failed: {}", e))
        })?;

        entry
            .set_password(&encoded)
            .map_err(|e| WalletError::KeyringError(format!("Failed to store secret: {}", e)))?;

        Ok(())
    }

    /// Retrieve and decrypt a secret value under `key_id`.
    pub fn retrieve_secret(
        &self,
        key_id: &str,
        password: &str,
    ) -> Result<Secret<String>, WalletError> {
        let entry = Entry::new(&self.service_name, key_id).map_err(|e| {
            WalletError::KeyringError(format!("Keyring entry creation failed: {}", e))
        })?;

        let encoded = entry
            .get_password()
            .map_err(|e| WalletError::KeyringError(format!("Failed to retrieve secret: {}", e)))?;

        let encrypted = base64::engine::general_purpose::STANDARD
            .decode(encoded)
            .map_err(|e| WalletError::DecryptionFailed(format!("Base64 decode failed: {}", e)))?;

        let plaintext = decrypt_data(&encrypted, password)?;
        let s = String::from_utf8(plaintext)
            .map_err(|e| WalletError::DecryptionFailed(format!("Invalid UTF-8: {}", e)))?;

        Ok(Secret::new(s))
    }

    /// Whether a keyring entry exists (password not required; does not decrypt).
    pub fn has_secret(&self, key_id: &str) -> bool {
        match Entry::new(&self.service_name, key_id) {
            Ok(entry) => entry.get_password().is_ok(),
            Err(_) => false,
        }
    }

    /// Delete a stored secret.
    pub fn delete_secret(&self, key_id: &str) -> Result<(), WalletError> {
        let entry = Entry::new(&self.service_name, key_id).map_err(|e| {
            WalletError::KeyringError(format!("Keyring entry creation failed: {}", e))
        })?;

        entry
            .delete_password()
            .map_err(|e| WalletError::KeyringError(format!("Failed to delete secret: {}", e)))?;

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use secrecy::ExposeSecret;

    // OS keychains can be unavailable in some environments (CI, locked-down desktops).
    // This is a smoke test you can run locally when needed.
    #[test]
    #[ignore]
    fn store_retrieve_delete_roundtrip() {
        let svc = KeyringService::new("vaughan-core-test").unwrap();
        let key = "test_keyring_item";
        let password = "test_password_123";
        let secret = "super_secret_value";

        svc.store_secret(key, secret, password).unwrap();
        let got = svc.retrieve_secret(key, password).unwrap();
        assert_eq!(got.expose_secret(), secret);
        svc.delete_secret(key).unwrap();
    }
}
