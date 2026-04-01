//! Transaction building types and helpers (Task 7.1).

use serde::{Deserialize, Serialize};

use crate::chains::{ChainTransaction, EvmTransaction, Fee};
use crate::error::WalletError;
use alloy::eips::eip2718::Encodable2718;
use alloy::network::{EthereumWallet, TransactionBuilder};
use alloy::primitives::{TxKind, U256};
use alloy::rpc::types::eth::TransactionRequest;
use alloy::signers::local::PrivateKeySigner;
use alloy::sol_types::SolCall;
use std::str::FromStr;

/// User intent to send a transaction (chain-agnostic).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TransactionIntent {
    pub from: String,
    pub to: String,
    /// Amount in wei as decimal string for EVM.
    pub value: String,
    pub data: Option<String>,
    pub chain_id: u64,
}

/// A built transaction ready for fee estimation/signing.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BuiltTransaction {
    pub tx: ChainTransaction,
    pub fee: Option<Fee>,
}

/// Transaction service (stateless)
pub struct TransactionService;

impl TransactionService {
    pub fn new() -> Self {
        Self
    }

    /// Build a basic EVM transaction request from an intent.
    pub fn build_evm_transaction(
        &self,
        intent: TransactionIntent,
    ) -> Result<BuiltTransaction, WalletError> {
        let tx = EvmTransaction {
            from: intent.from,
            to: intent.to,
            value: intent.value,
            data: intent.data,
            gas_limit: None,
            gas_price: None,
            max_fee_per_gas: None,
            max_priority_fee_per_gas: None,
            nonce: None,
            chain_id: intent.chain_id,
        };

        Ok(BuiltTransaction {
            tx: ChainTransaction::Evm(tx),
            fee: None,
        })
    }

    /// Build an ERC-20 `transfer(to, amount)` transaction.
    ///
    /// `amount` must be in the token's smallest unit (per ERC-20 decimals).
    pub fn build_erc20_transfer(
        &self,
        from: String,
        token_address: String,
        to: String,
        amount: String,
        chain_id: u64,
    ) -> Result<BuiltTransaction, WalletError> {
        let _from_addr = from
            .parse::<alloy::primitives::Address>()
            .map_err(|_| WalletError::InvalidAddress(from.clone()))?;
        let token_addr = token_address
            .parse::<alloy::primitives::Address>()
            .map_err(|_| WalletError::InvalidAddress(token_address.clone()))?;
        let to_addr = to
            .parse::<alloy::primitives::Address>()
            .map_err(|_| WalletError::InvalidAddress(to.clone()))?;

        let amount_u256 = U256::from_str(&amount)
            .map_err(|_| WalletError::InvalidAmount(format!("Invalid token amount: {}", amount)))?;

        // Encode transfer calldata
        let call = crate::models::erc20::IERC20::transferCall {
            to: to_addr,
            amount: amount_u256,
        };
        let calldata = call.abi_encode();

        let evm_tx = EvmTransaction {
            from,
            to: format!("{:?}", token_addr),
            value: "0".into(),
            data: Some(format!("0x{}", hex::encode(calldata))),
            gas_limit: None,
            gas_price: None,
            max_fee_per_gas: None,
            max_priority_fee_per_gas: None,
            nonce: None,
            chain_id,
        };

        Ok(BuiltTransaction {
            tx: ChainTransaction::Evm(evm_tx),
            fee: None,
        })
    }

    /// Sign an EVM transaction and return a raw 0x-prefixed 2718-encoded tx.
    ///
    /// Requires gas limit and nonce to be present (wallet should estimate/fetch them first).
    pub async fn sign_evm_transaction(
        &self,
        signer: &PrivateKeySigner,
        tx: &EvmTransaction,
    ) -> Result<String, WalletError> {
        let from = tx
            .from
            .parse()
            .map_err(|_| WalletError::InvalidAddress(tx.from.clone()))?;
        let to = tx
            .to
            .parse()
            .map_err(|_| WalletError::InvalidAddress(tx.to.clone()))?;

        let value = U256::from_str(&tx.value)
            .map_err(|_| WalletError::InvalidAmount(format!("Invalid wei value: {}", tx.value)))?;

        let gas = tx
            .gas_limit
            .ok_or_else(|| WalletError::InvalidTransaction("Missing gas_limit".into()))?;
        let nonce = tx
            .nonce
            .ok_or_else(|| WalletError::InvalidTransaction("Missing nonce".into()))?;

        let mut req = TransactionRequest {
            from: Some(from),
            to: Some(TxKind::Call(to)),
            value: Some(value),
            gas: Some(gas),
            nonce: Some(nonce),
            chain_id: Some(tx.chain_id),
            ..Default::default()
        };

        if let Some(gas_price) = tx.gas_price.as_deref() {
            let gp = U256::from_str(gas_price).map_err(|_| {
                WalletError::InvalidAmount(format!("Invalid gas_price: {}", gas_price))
            })?;
            req.gas_price = Some(gp.to::<u128>());
        }
        if let Some(max_fee) = tx.max_fee_per_gas.as_deref() {
            let mf = U256::from_str(max_fee).map_err(|_| {
                WalletError::InvalidAmount(format!("Invalid max_fee_per_gas: {}", max_fee))
            })?;
            req.max_fee_per_gas = Some(mf.to::<u128>());
        }
        if let Some(prio) = tx.max_priority_fee_per_gas.as_deref() {
            let pf = U256::from_str(prio).map_err(|_| {
                WalletError::InvalidAmount(format!("Invalid max_priority_fee_per_gas: {}", prio))
            })?;
            req.max_priority_fee_per_gas = Some(pf.to::<u128>());
        }

        if let Some(data_hex) = tx.data.as_deref() {
            let bytes = hex::decode(data_hex.trim_start_matches("0x"))
                .map_err(|_| WalletError::InvalidTransaction("Invalid hex data".into()))?;
            req.input.input = Some(bytes.into());
        }

        let wallet = EthereumWallet::from(signer.clone());
        let envelope = req
            .build(&wallet)
            .await
            .map_err(|e| WalletError::SigningFailed(e.to_string()))?;

        Ok(format!("0x{}", hex::encode(envelope.encoded_2718())))
    }

    /// Fetch the current account nonce for EVM chains via a chain adapter.
    pub async fn get_nonce(
        &self,
        adapter: &dyn crate::chains::ChainAdapter,
        from: &str,
    ) -> Result<u64, WalletError> {
        adapter.get_nonce(from).await
    }

    /// Broadcast a transaction using the chain adapter.
    pub async fn broadcast(
        &self,
        adapter: &dyn crate::chains::ChainAdapter,
        tx: ChainTransaction,
    ) -> Result<crate::chains::TxHash, WalletError> {
        adapter.send_transaction(tx).await
    }
}

impl Default for TransactionService {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn build_evm_transaction_roundtrip_intent() {
        let svc = TransactionService::new();
        let intent = TransactionIntent {
            from: "0x0000000000000000000000000000000000000001".into(),
            to: "0x0000000000000000000000000000000000000002".into(),
            value: "1000000000000000000".into(),
            data: None,
            chain_id: 1,
        };
        let built = svc.build_evm_transaction(intent.clone()).unwrap();
        match built.tx {
            ChainTransaction::Evm(e) => {
                assert_eq!(e.from, intent.from);
                assert_eq!(e.to, intent.to);
                assert_eq!(e.value, intent.value);
                assert_eq!(e.chain_id, intent.chain_id);
            }
        }
    }

    #[test]
    fn build_erc20_transfer_encodes_calldata() {
        let svc = TransactionService::new();
        let from = "0x0000000000000000000000000000000000000001".to_string();
        let token = "0x0000000000000000000000000000000000000003".to_string();
        let to = "0x0000000000000000000000000000000000000004".to_string();
        let amount = "42".to_string();
        let built = svc
            .build_erc20_transfer(from.clone(), token.clone(), to.clone(), amount.clone(), 1)
            .unwrap();
        match built.tx {
            ChainTransaction::Evm(e) => {
                assert_eq!(e.from, from);
                assert!(e.to.contains("0000000000000000000000000000000000000003"));
                assert_eq!(e.value, "0");
                assert!(e.data.as_deref().unwrap().starts_with("0x"));
            }
        }
    }

    #[test]
    fn build_erc20_transfer_rejects_bad_amount() {
        let svc = TransactionService::new();
        let from = "0x0000000000000000000000000000000000000001".to_string();
        let token = "0x0000000000000000000000000000000000000003".to_string();
        let to = "0x0000000000000000000000000000000000000004".to_string();
        let amount = "not_a_number".to_string();
        let err = svc
            .build_erc20_transfer(from, token, to, amount, 1)
            .expect_err("should fail on invalid amount");
        matches!(err, WalletError::InvalidAmount(_));
    }
}
