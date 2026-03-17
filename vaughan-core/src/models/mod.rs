//! Shared data models (account, balance, transaction, etc.).

pub mod wallet;
pub mod erc20;

pub use wallet::{Account, AccountType};
pub use erc20::IERC20;
