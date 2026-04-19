//! Chain-agnostic wallet business logic: WalletState, accounts, transactions, persistence.

pub mod account;
pub mod history;
pub mod network;
pub mod persistence;
pub mod token;
pub mod transaction;
pub mod wallet;

pub use account::{Account, AccountId, AccountType};
pub use history::HistoryService;
pub use network::{NetworkConfig, NetworkInfo, NetworkService};
pub use persistence::{
    vaughan_state_json_path, NativeDappInstallRecord, PersistedState, PersistenceHandle, StateManager,
};
pub use token::{TokenManager, TrackedToken};
pub use transaction::TransactionService;
pub use wallet::WalletState;
