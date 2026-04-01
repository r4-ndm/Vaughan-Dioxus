//! Account model and account-related helpers.
//!
//! Task 6.1: Define the core `Account` model used by wallet state.
//!
//! Account list and active id live in [`PersistenceHandle`] and are saved as part of the full
//! [`PersistedState`](crate::core::persistence::PersistedState) document (single-writer model).

use alloy::primitives::Address;
use alloy::signers::local::PrivateKeySigner;
use secrecy::ExposeSecret;
use serde::{Deserialize, Serialize};
use std::str::FromStr;
use std::sync::Arc;
use uuid::Uuid;

use crate::core::persistence::PersistenceHandle;
use crate::error::WalletError;
use crate::security::{derive_account, mnemonic_to_seed, validate_mnemonic, KeyringService};

/// Stable account identifier.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct AccountId(pub Uuid);

#[allow(clippy::new_without_default)] // `Default` would imply a stable id; new IDs are always random.
impl AccountId {
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }
}

/// Account type (software-derived vs imported). Hardware will be added later.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum AccountType {
    Hd,
    Imported,
}

/// Wallet account (EVM-only for now; multi-chain addresses will be added later).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Account {
    pub id: AccountId,
    pub name: String,
    pub address: Address,
    pub account_type: AccountType,
    /// Derivation index (for HD accounts)
    pub index: Option<u32>,
}

/// Keyring id for the single BIP-39 master mnemonic (all HD accounts derive from it).
pub const MASTER_MNEMONIC_KEYRING_ID: &str = "vaughan_seed";

/// Manages accounts and secure key storage.
///
/// This keeps secrets in the OS keychain via `KeyringService`.
/// Non-secret account metadata is stored in [`PersistenceHandle`] and flushed as a full `state.json` write.
pub struct AccountManager {
    keyring: KeyringService,
    persistence: Arc<PersistenceHandle>,
}

impl AccountManager {
    pub fn new(
        service_name: &str,
        persistence: Arc<PersistenceHandle>,
    ) -> Result<Self, WalletError> {
        Ok(Self {
            keyring: KeyringService::new(service_name)?,
            persistence,
        })
    }

    /// True if a master mnemonic has been stored (first-launch onboarding done or restored wallet).
    pub fn has_master_wallet(&self) -> bool {
        self.keyring.has_secret(MASTER_MNEMONIC_KEYRING_ID)
    }

    /// Store the wallet seed mnemonic in keychain.
    pub fn store_wallet_mnemonic(&self, password: &str, mnemonic: &str) -> Result<(), WalletError> {
        validate_mnemonic(mnemonic)?;
        self.keyring
            .store_secret(MASTER_MNEMONIC_KEYRING_ID, mnemonic, password)
    }

    /// Export the wallet seed mnemonic from keychain.
    pub fn export_wallet_mnemonic(&self, password: &str) -> Result<String, WalletError> {
        Ok(self
            .keyring
            .retrieve_secret(MASTER_MNEMONIC_KEYRING_ID, password)?
            .expose_secret()
            .to_string())
    }

    /// Confirms `password` decrypts the stored master mnemonic without returning it (e.g. startup unlock).
    pub fn verify_master_password(&self, password: &str) -> Result<(), WalletError> {
        if !self.has_master_wallet() {
            return Ok(());
        }
        let _ = self
            .keyring
            .retrieve_secret(MASTER_MNEMONIC_KEYRING_ID, password)?;
        Ok(())
    }

    /// First launch: encrypt master mnemonic and create the master HD account at `m/44'/60'/0'/0/0`.
    pub async fn create_master_wallet(
        &self,
        password: &str,
        mnemonic: &str,
    ) -> Result<Account, WalletError> {
        if self.has_master_wallet() {
            return Err(WalletError::WalletAlreadyExists);
        }
        validate_mnemonic(mnemonic)?;
        self.store_wallet_mnemonic(password, mnemonic)?;
        self.create_hd_account(password, 0, "Master wallet".into())
            .await
    }

    /// Next free BIP44 account index for HD derivation (1+ after master at 0).
    pub async fn next_hd_derivation_index(&self) -> u32 {
        let accounts = self.list_accounts().await;
        accounts
            .iter()
            .filter(|a| matches!(a.account_type, AccountType::Hd))
            .filter_map(|a| a.index)
            .max()
            .map(|m| m + 1)
            .unwrap_or(1)
    }

    /// Add another HD account from the stored master seed (wallet password only; no new seed phrase).
    pub async fn add_hd_derived_account(
        &self,
        password: &str,
        name: String,
    ) -> Result<Account, WalletError> {
        let idx = self.next_hd_derivation_index().await;
        self.create_hd_account(password, idx, name).await
    }

    /// Restore or replace the master wallet from a recovery phrase. Clears accounts and removes imported key material from the keyring.
    pub async fn replace_master_from_mnemonic(
        &self,
        password: &str,
        mnemonic: &str,
    ) -> Result<Account, WalletError> {
        validate_mnemonic(mnemonic)?;
        let existing = self.list_accounts().await;
        for a in &existing {
            if a.account_type == AccountType::Imported {
                let key_id = format!("account_{:?}", a.address);
                let _ = self.keyring.delete_secret(&key_id);
            }
        }
        if let Err(e) = self
            .persistence
            .update_and_save(|st| {
                st.accounts.clear();
                st.active_account = None;
            })
            .await
        {
            tracing::warn!(target: "vaughan_core", "Failed to persist after replace (clear): {}", e);
        }
        self.store_wallet_mnemonic(password, mnemonic)?;
        self.create_hd_account(password, 0, "Master wallet".into())
            .await
    }

    /// Create an HD account from the stored mnemonic at `index`.
    pub async fn create_hd_account(
        &self,
        password: &str,
        index: u32,
        name: String,
    ) -> Result<Account, WalletError> {
        let mnemonic = self.keyring.retrieve_secret("vaughan_seed", password)?;
        let seed = mnemonic_to_seed(mnemonic.expose_secret(), None)?;
        let signer = derive_account(&seed, index)?;
        let address = signer.address();

        let acct = Account {
            id: AccountId::new(),
            name,
            address,
            account_type: AccountType::Hd,
            index: Some(index),
        };
        self.add_account(acct.clone()).await;
        Ok(acct)
    }

    /// Import an account from a private key string (hex or 0x-hex), storing it encrypted in keychain.
    pub async fn import_private_key_account(
        &self,
        password: &str,
        private_key: &str,
        name: String,
    ) -> Result<Account, WalletError> {
        let pk = private_key.trim_start_matches("0x");
        let signer = PrivateKeySigner::from_str(pk)
            .map_err(|e| WalletError::InvalidPrivateKey(e.to_string()))?;
        let address = signer.address();

        let key_id = format!("account_{:?}", address);
        self.keyring.store_secret(&key_id, pk, password)?;

        let acct = Account {
            id: AccountId::new(),
            name,
            address,
            account_type: AccountType::Imported,
            index: None,
        };
        self.add_account(acct.clone()).await;
        Ok(acct)
    }

    /// Export an imported account private key (requires password).
    pub fn export_private_key(
        &self,
        password: &str,
        address: Address,
    ) -> Result<String, WalletError> {
        let key_id = format!("account_{:?}", address);
        Ok(self
            .keyring
            .retrieve_secret(&key_id, password)?
            .expose_secret()
            .to_string())
    }

    pub async fn add_account(&self, account: Account) {
        if let Err(e) = self
            .persistence
            .update_and_save(|st| {
                st.accounts.push(account.clone());
                if st.active_account.is_none() {
                    st.active_account = Some(account.id);
                }
            })
            .await
        {
            tracing::warn!(target: "vaughan_core", "Failed to persist accounts: {}", e);
        }
    }

    pub async fn list_accounts(&self) -> Vec<Account> {
        self.persistence
            .state
            .read()
            .expect("persist state poisoned")
            .accounts
            .clone()
    }

    pub async fn rename_account(&self, id: AccountId, new_name: String) -> Result<(), WalletError> {
        {
            let mut st = self
                .persistence
                .state
                .write()
                .expect("persist state poisoned");
            let acct = st
                .accounts
                .iter_mut()
                .find(|a| a.id == id)
                .ok_or_else(|| WalletError::AccountNotFound(id.0.to_string()))?;
            acct.name = new_name;
        }
        if let Err(e) = self.persistence.save_disk().await {
            tracing::warn!(target: "vaughan_core", "Failed to persist accounts after rename: {}", e);
        }
        Ok(())
    }

    pub async fn delete_account(&self, id: AccountId) -> Result<(), WalletError> {
        let removed = {
            let mut st = self
                .persistence
                .state
                .write()
                .expect("persist state poisoned");
            let pos = st
                .accounts
                .iter()
                .position(|a| a.id == id)
                .ok_or_else(|| WalletError::AccountNotFound(id.0.to_string()))?;
            let removed = st.accounts.remove(pos);
            if st.active_account == Some(id) {
                st.active_account = st.accounts.first().map(|a| a.id);
            }
            removed
        };

        if removed.account_type == AccountType::Imported {
            let key_id = format!("account_{:?}", removed.address);
            let _ = self.keyring.delete_secret(&key_id);
        }

        if let Err(e) = self.persistence.save_disk().await {
            tracing::warn!(target: "vaughan_core", "Failed to persist accounts after delete: {}", e);
        }
        Ok(())
    }

    pub async fn set_active(&self, id: AccountId) -> Result<(), WalletError> {
        {
            let st = self
                .persistence
                .state
                .read()
                .expect("persist state poisoned");
            if !st.accounts.iter().any(|a| a.id == id) {
                return Err(WalletError::AccountNotFound(id.0.to_string()));
            }
        }
        {
            let mut st = self
                .persistence
                .state
                .write()
                .expect("persist state poisoned");
            st.active_account = Some(id);
        }
        if let Err(e) = self.persistence.save_disk().await {
            tracing::warn!(target: "vaughan_core", "Failed to persist accounts after set_active: {}", e);
        }
        Ok(())
    }

    pub async fn active_account(&self) -> Option<Account> {
        let st = self
            .persistence
            .state
            .read()
            .expect("persist state poisoned");
        let id = st.active_account?;
        st.accounts.iter().find(|a| a.id == id).cloned()
    }

    /// Remove all accounts from persisted state, delete imported keys and master mnemonic from the keyring.
    /// Ignores keyring delete errors for missing entries.
    pub async fn wipe_all_wallet_data(&self) -> Result<(), WalletError> {
        let accounts = self.list_accounts().await;
        for a in &accounts {
            if a.account_type == AccountType::Imported {
                let key_id = format!("account_{:?}", a.address);
                let _ = self.keyring.delete_secret(&key_id);
            }
        }
        let _ = self.keyring.delete_secret(MASTER_MNEMONIC_KEYRING_ID);
        self.persistence
            .update_and_save(|st| {
                st.accounts.clear();
                st.active_account = None;
            })
            .await?;
        Ok(())
    }
}
