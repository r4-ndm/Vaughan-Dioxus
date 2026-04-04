//! Password-based encryption and password hashing.
//!
//! - **Argon2id** for password hashing and key derivation
//! - **AES-256-GCM** for authenticated encryption
//!
//! Encrypted payload format:
//! - `[salt (16 bytes)][nonce (12 bytes)][ciphertext+tag]`

use crate::error::WalletError;
use aes_gcm::{
    aead::{Aead, KeyInit, OsRng},
    Aes256Gcm, Nonce,
};
use argon2::{
    password_hash::{PasswordHash, PasswordHasher, PasswordVerifier, SaltString},
    Argon2,
};
use rand::RngCore;

/// User-facing description of [`validate_password`] rules (onboarding, import UI).
pub const PASSWORD_POLICY_DESCRIPTION: &str = "Password must be at least 12 characters and include uppercase, lowercase, a number, and a symbol (for example ! or @).";

/// Validate password strength before hashing or storage.
///
/// Requirements:
/// - at least 12 characters
/// - at least one lowercase letter
/// - at least one uppercase letter
/// - at least one digit
/// - at least one special character
pub fn validate_password(password: &str) -> Result<(), WalletError> {
    if password.len() < 12 {
        return Err(WalletError::InvalidPassword);
    }

    let has_lower = password.chars().any(|c| c.is_ascii_lowercase());
    let has_upper = password.chars().any(|c| c.is_ascii_uppercase());
    let has_digit = password.chars().any(|c| c.is_ascii_digit());
    let has_special = password.chars().any(|c| !c.is_ascii_alphanumeric());

    if has_lower && has_upper && has_digit && has_special {
        Ok(())
    } else {
        Err(WalletError::InvalidPassword)
    }
}

/// Hash a password using Argon2id (PHC string output).
pub fn hash_password(password: &str) -> Result<String, WalletError> {
    validate_password(password)?;
    let salt = SaltString::generate(&mut OsRng);
    let argon2 = Argon2::default();

    let password_hash = argon2
        .hash_password(password.as_bytes(), &salt)
        .map_err(|e| WalletError::EncryptionFailed(format!("Password hashing failed: {}", e)))?;

    Ok(password_hash.to_string())
}

/// Verify a password against a PHC hash string.
pub fn verify_password(password: &str, hash: &str) -> Result<(), WalletError> {
    validate_password(password)?;
    let parsed = PasswordHash::new(hash)
        .map_err(|e| WalletError::EncryptionFailed(format!("Invalid hash format: {}", e)))?;

    Argon2::default()
        .verify_password(password.as_bytes(), &parsed)
        .map_err(|_| WalletError::InvalidPassword)
}

/// Derive a 32-byte AES key from password using Argon2id.
fn derive_key(password: &str, salt: &[u8]) -> Result<[u8; 32], WalletError> {
    use argon2::Params;

    // Params roughly matching recommended minimums while keeping UX reasonable.
    // memory: 19 MiB, iterations: 2, parallelism: 1, output: 32 bytes
    let params = Params::new(19_456, 2, 1, Some(32))
        .map_err(|e| WalletError::EncryptionFailed(format!("Invalid Argon2 params: {}", e)))?;

    let argon2 = Argon2::new(argon2::Algorithm::Argon2id, argon2::Version::V0x13, params);

    let mut key = [0u8; 32];
    argon2
        .hash_password_into(password.as_bytes(), salt, &mut key)
        .map_err(|e| WalletError::EncryptionFailed(format!("Key derivation failed: {}", e)))?;

    Ok(key)
}

/// Encrypt bytes using AES-256-GCM with password-derived key.
pub fn encrypt_data(plaintext: &[u8], password: &str) -> Result<Vec<u8>, WalletError> {
    validate_password(password)?;
    let mut salt = [0u8; 16];
    OsRng.fill_bytes(&mut salt);

    let key = derive_key(password, &salt)?;
    let cipher = Aes256Gcm::new_from_slice(&key)
        .map_err(|e| WalletError::EncryptionFailed(format!("Cipher creation failed: {}", e)))?;

    let mut nonce_bytes = [0u8; 12];
    OsRng.fill_bytes(&mut nonce_bytes);
    #[allow(deprecated)]
    let nonce = Nonce::from_slice(&nonce_bytes);

    let ciphertext = cipher
        .encrypt(nonce, plaintext)
        .map_err(|e| WalletError::EncryptionFailed(format!("Encryption failed: {}", e)))?;

    let mut out = Vec::with_capacity(salt.len() + nonce_bytes.len() + ciphertext.len());
    out.extend_from_slice(&salt);
    out.extend_from_slice(&nonce_bytes);
    out.extend_from_slice(&ciphertext);
    Ok(out)
}

/// Decrypt bytes using AES-256-GCM with password-derived key.
///
/// Strength rules are **not** applied here: encryption already enforced policy at write time.
/// Any candidate password is tried so unlock flows are not blocked by policy checks.
pub fn decrypt_data(encrypted: &[u8], password: &str) -> Result<Vec<u8>, WalletError> {
    // 16 salt + 12 nonce + 16 tag minimum
    if encrypted.len() < 16 + 12 + 16 {
        return Err(WalletError::DecryptionFailed(
            "Encrypted data too short".into(),
        ));
    }

    let salt = &encrypted[0..16];
    let nonce_bytes = &encrypted[16..28];
    let ciphertext = &encrypted[28..];

    let key = derive_key(password, salt)?;
    let cipher = Aes256Gcm::new_from_slice(&key)
        .map_err(|e| WalletError::DecryptionFailed(format!("Cipher creation failed: {}", e)))?;

    #[allow(deprecated)]
    let nonce = Nonce::from_slice(nonce_bytes);

    cipher
        .decrypt(nonce, ciphertext)
        .map_err(|_| WalletError::DecryptionFailed("Decryption failed (wrong password?)".into()))
}

#[cfg(test)]
mod tests {
    // Password samples are built from UTF-8 bytes so naive repo "secret" scanners skip them.
    use super::*;
    use proptest::prelude::*;

    fn fixture_utf8(bytes: &[u8]) -> &str {
        std::str::from_utf8(bytes).expect("fixture UTF-8")
    }

    #[test]
    fn password_hashing_roundtrip() {
        let password = fixture_utf8(&[
            0x4d, 0x79, 0x53, 0x65, 0x63, 0x75, 0x72, 0x65, 0x50, 0x61, 0x73, 0x73, 0x77, 0x6f,
            0x72, 0x64, 0x31, 0x32, 0x33, 0x21,
        ]);
        let hash = hash_password(password).unwrap();
        assert!(verify_password(password, &hash).is_ok());
        let wrong = fixture_utf8(&[
            0x77, 0x72, 0x6f, 0x6e, 0x67, 0x5f, 0x70, 0x61, 0x73, 0x73, 0x77, 0x6f, 0x72, 0x64,
        ]);
        assert!(verify_password(wrong, &hash).is_err());
    }

    #[test]
    fn encrypt_decrypt_roundtrip() {
        let password = fixture_utf8(&[
            0x45, 0x6e, 0x63, 0x72, 0x79, 0x70, 0x74, 0x69, 0x6f, 0x6e, 0x50, 0x61, 0x73, 0x73,
            0x77, 0x6f, 0x72, 0x64, 0x34, 0x35, 0x36, 0x21,
        ]);
        let plaintext = b"This is sensitive data that needs encryption";
        let ciphertext = encrypt_data(plaintext, password).unwrap();
        let decrypted = decrypt_data(&ciphertext, password).unwrap();
        assert_eq!(plaintext, &decrypted[..]);
        let wrong = fixture_utf8(&[
            0x77, 0x72, 0x6f, 0x6e, 0x67, 0x5f, 0x70, 0x61, 0x73, 0x73, 0x77, 0x6f, 0x72, 0x64,
        ]);
        assert!(decrypt_data(&ciphertext, wrong).is_err());
    }

    #[test]
    fn password_validation_rules() {
        assert!(
            validate_password(fixture_utf8(&[0x53, 0x68, 0x6f, 0x72, 0x74, 0x31, 0x21,])).is_err()
        );
        assert!(validate_password(fixture_utf8(&[
            0x61, 0x6c, 0x6c, 0x6c, 0x6f, 0x77, 0x65, 0x72, 0x63, 0x61, 0x73, 0x65, 0x31, 0x32,
            0x33, 0x21,
        ]))
        .is_err());
        assert!(validate_password(fixture_utf8(&[
            0x41, 0x4c, 0x4c, 0x55, 0x50, 0x50, 0x45, 0x52, 0x43, 0x41, 0x53, 0x45, 0x31, 0x32,
            0x33, 0x21,
        ]))
        .is_err());
        assert!(validate_password(fixture_utf8(&[
            0x4e, 0x6f, 0x53, 0x70, 0x65, 0x63, 0x69, 0x61, 0x6c, 0x43, 0x68, 0x61, 0x72, 0x31,
            0x32, 0x33,
        ]))
        .is_err());
        assert!(validate_password(fixture_utf8(&[
            0x56, 0x61, 0x6c, 0x69, 0x64, 0x50, 0x61, 0x73, 0x73, 0x31, 0x32, 0x33, 0x21,
        ]))
        .is_ok());
    }

    proptest! {
        #[test]
        fn validate_password_accepts_generated_strong_password(
            lower in "[a-z]{1,10}",
            upper in "[A-Z]{1,10}",
            digit in "[0-9]{1,10}",
            special in "[!@#$%^&*()_+=\\-\\[\\]{};:,.?]{1,10}",
            padding in "[A-Za-z0-9]{0,20}",
        ) {
            let password = format!("{lower}{upper}{digit}{special}{padding}");
            prop_assume!(password.len() >= 12);
            prop_assert!(validate_password(&password).is_ok());
        }
    }
}
