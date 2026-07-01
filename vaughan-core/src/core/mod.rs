//! Chain-agnostic wallet business logic: WalletState, accounts, transactions, persistence.

pub mod account;
pub mod history;
pub mod network;
pub mod persistence;
pub mod signing;
pub mod token;
pub mod transaction;
pub mod wallet;
pub mod smart_account;
pub mod ambire_abi;
pub mod scw_transaction;

pub use account::{Account, AccountId, AccountType, SmartAccountInfo};
pub use signing::{
    address_to_hex, load_active_signer, load_signer_for_account, load_signer_for_address,
    parse_optional_u64_decimal,
};
pub use history::HistoryService;
pub use network::{NetworkConfig, NetworkInfo, NetworkService};
pub use persistence::{
    vaughan_state_json_path, NativeDappInstallRecord, PersistedState, PersistenceHandle, StateManager,
};
pub use token::TokenManager;
pub use transaction::TransactionService;
pub use wallet::WalletState;
pub use smart_account::{derive_smart_account_address, build_init_code, generate_salt, AMBIRE_ACCOUNT_BYTECODE};
pub use scw_transaction::{build_execute_hash, build_signed_execute, build_signed_deploy_and_execute, wrap_scw_as_chain_transaction, get_smart_account_nonce, is_account_deployed};
