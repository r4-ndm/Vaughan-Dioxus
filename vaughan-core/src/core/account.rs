//! Account model and account-related helpers.
//!
//! Task 6.1: Define the core `Account` model used by wallet state.

use alloy::primitives::Address;
use alloy::signers::local::PrivateKeySigner;
use secrecy::ExposeSecret;
use serde::{Deserialize, Serialize};
use std::str::FromStr;
use tokio::sync::RwLock;
use uuid::Uuid;

use crate::error::WalletError;
use crate::security::{derive_account, mnemonic_to_seed, validate_mnemonic, KeyringService};

/// Stable account identifier.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct AccountId(pub Uuid);

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

/// Manages accounts and secure key storage.
///
/// This keeps secrets in the OS keychain via `KeyringService`.
pub struct AccountManager {
    keyring: KeyringService,
    accounts: RwLock<Vec<Account>>,
    active: RwLock<Option<AccountId>>,
}

impl AccountManager {
    pub fn new(service_name: &str) -> Result<Self, WalletError> {
        Ok(Self {
            keyring: KeyringService::new(service_name)?,
            accounts: RwLock::new(Vec::new()),
            active: RwLock::new(None),
        })
    }

    /// Store the wallet seed mnemonic in keychain.
    pub fn store_wallet_mnemonic(&self, password: &str, mnemonic: &str) -> Result<(), WalletError> {
        validate_mnemonic(mnemonic)?;
        self.keyring.store_secret("vaughan_seed", mnemonic, password)
    }

    /// Export the wallet seed mnemonic from keychain.
    pub fn export_wallet_mnemonic(&self, password: &str) -> Result<String, WalletError> {
        Ok(self
            .keyring
            .retrieve_secret("vaughan_seed", password)?
            .expose_secret()
            .to_string())
    }

    /// Create an HD account from the stored mnemonic at `index`.
    pub async fn create_hd_account(&self, password: &str, index: u32, name: String) -> Result<Account, WalletError> {
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
    pub fn export_private_key(&self, password: &str, address: Address) -> Result<String, WalletError> {
        let key_id = format!("account_{:?}", address);
        Ok(self
            .keyring
            .retrieve_secret(&key_id, password)?
            .expose_secret()
            .to_string())
    }

    pub async fn add_account(&self, account: Account) {
        self.accounts.write().await.push(account.clone());
        if self.active.read().await.is_none() {
            *self.active.write().await = Some(account.id);
        }
    }

    pub async fn list_accounts(&self) -> Vec<Account> {
        self.accounts.read().await.clone()
    }

    pub async fn rename_account(&self, id: AccountId, new_name: String) -> Result<(), WalletError> {
        let mut accounts = self.accounts.write().await;
        let acct = accounts.iter_mut().find(|a| a.id == id).ok_or_else(|| WalletError::AccountNotFound(id.0.to_string()))?;
        acct.name = new_name;
        Ok(())
    }

    pub async fn delete_account(&self, id: AccountId) -> Result<(), WalletError> {
        let mut accounts = self.accounts.write().await;
        let pos = accounts.iter().position(|a| a.id == id).ok_or_else(|| WalletError::AccountNotFound(id.0.to_string()))?;
        let removed = accounts.remove(pos);

        // Clean up imported key material.
        if removed.account_type == AccountType::Imported {
            let key_id = format!("account_{:?}", removed.address);
            let _ = self.keyring.delete_secret(&key_id);
        }

        // Fix active account if needed.
        let mut active = self.active.write().await;
        if active.as_ref() == Some(&id) {
            *active = accounts.first().map(|a| a.id);
        }
        Ok(())
    }

    pub async fn set_active(&self, id: AccountId) -> Result<(), WalletError> {
        let accounts = self.accounts.read().await;
        if !accounts.iter().any(|a| a.id == id) {
            return Err(WalletError::AccountNotFound(id.0.to_string()));
        }
        drop(accounts);
        *self.active.write().await = Some(id);
        Ok(())
    }

    pub async fn active_account(&self) -> Option<Account> {
        let accounts = self.accounts.read().await;
        let active = self.active.read().await;
        let id = active.as_ref()?;
        accounts.iter().find(|a| &a.id == id).cloned()
    }
}

