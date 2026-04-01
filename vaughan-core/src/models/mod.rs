//! Shared data models (account, balance, transaction, etc.).

pub mod erc20;
pub mod wallet;

pub use erc20::IERC20;
pub use wallet::{Account, AccountType};
