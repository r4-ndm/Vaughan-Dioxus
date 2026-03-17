//! Alloy-backed EVM adapter foundation.
//!
//! This is Task 3.2: provider wiring + minimal balance operations.

use std::sync::Arc;
use std::str::FromStr;
use std::time::Duration;

use alloy::network::Ethereum;
use alloy::primitives::{utils::format_units, B256};
use alloy::providers::{Provider, RootProvider};
use alloy::rpc::client::RpcClient;
use alloy::transports::http::Http;
use async_trait::async_trait;
use url::Url;

use crate::chains::{Balance, ChainAdapter, ChainInfo, ChainTransaction, ChainType, Fee, TokenInfo, TxHash};
use crate::chains::evm::networks::get_network_by_chain_id;
use crate::chains::evm::utils::parse_address;
use crate::chains::evm::history::{fetch_tokentx, fetch_txlist};
use crate::error::WalletError;
use crate::models::IERC20;
use alloy::eips::eip2718::Encodable2718;
use alloy::network::{EthereumWallet, TransactionBuilder};
use alloy::primitives::{TxKind, U256};
use alloy::rpc::types::BlockNumberOrTag;
use alloy::rpc::types::eth::TransactionRequest;
use alloy::signers::local::PrivateKeySigner;

pub type AlloyProvider = RootProvider<Ethereum>;

/// EVM adapter built on Alloy's HTTP provider.
pub struct EvmAdapter {
    provider: Arc<AlloyProvider>,
    signer: Option<PrivateKeySigner>,
    rpc_url: String,
    chain_id: u64,
    network_name: String,
    balance_cache: moka::future::Cache<String, Balance>,
    gas_price_cache: moka::future::Cache<u64, String>,
    nonce_cache: moka::future::Cache<String, u64>,
}

impl EvmAdapter {
    /// Create an EVM adapter for the given RPC URL and chain id.
    ///
    /// `network_name` is used only for display until Task 3.3 introduces
    /// canonical network configs.
    pub async fn new(rpc_url: &str, chain_id: u64, network_name: impl Into<String>) -> Result<Self, WalletError> {
        let url = Url::parse(rpc_url).map_err(|e| WalletError::NetworkError(e.to_string()))?;
        let transport = Http::new(url);
        let client = RpcClient::new(transport, true);
        let provider = RootProvider::<Ethereum>::new(client);

        Ok(Self {
            provider: Arc::new(provider),
            signer: None,
            rpc_url: rpc_url.to_string(),
            chain_id,
            network_name: network_name.into(),
            balance_cache: moka::future::Cache::builder().time_to_live(Duration::from_secs(10)).build(),
            gas_price_cache: moka::future::Cache::builder().time_to_live(Duration::from_secs(15)).build(),
            nonce_cache: moka::future::Cache::builder().time_to_live(Duration::from_secs(5)).build(),
        })
    }

    /// Create an adapter with a local signer for signing and sending raw transactions.
    pub async fn with_signer(
        rpc_url: &str,
        chain_id: u64,
        network_name: impl Into<String>,
        signer: PrivateKeySigner,
    ) -> Result<Self, WalletError> {
        let mut this = Self::new(rpc_url, chain_id, network_name).await?;
        this.signer = Some(signer);
        Ok(this)
    }

    pub fn provider(&self) -> Arc<AlloyProvider> {
        self.provider.clone()
    }

    pub fn chain_id(&self) -> u64 {
        self.chain_id
    }

    async fn get_gas_price_cached(&self) -> Result<String, WalletError> {
        if let Some(cached) = self.gas_price_cache.get(&self.chain_id).await {
            return Ok(cached);
        }
        let gas_price = self
            .provider
            .get_gas_price()
            .await
            .map_err(|e| WalletError::RpcError(e.to_string()))?;
        let gas_price = gas_price.to_string();
        self.gas_price_cache.insert(self.chain_id, gas_price.clone()).await;
        Ok(gas_price)
    }
}

#[async_trait]
impl ChainAdapter for EvmAdapter {
    async fn get_balance(&self, address: &str) -> Result<Balance, WalletError> {
        if let Some(cached) = self.balance_cache.get(address).await {
            return Ok(cached);
        }
        let addr = parse_address(address)?;

        let raw = self
            .provider
            .get_balance(addr)
            .await
            .map_err(|e| WalletError::RpcError(e.to_string()))?;

        let (symbol, name, decimals) = if let Some(net) = get_network_by_chain_id(self.chain_id) {
            (net.native_symbol, net.native_name, net.decimals)
        } else {
            ("ETH".into(), "Ethereum".into(), 18)
        };
        let formatted = format_units(raw, decimals).unwrap_or_else(|_| "0.0".to_string());

        let token = TokenInfo {
            symbol,
            name,
            decimals,
            contract_address: None,
        };

        let bal = Balance {
            token,
            raw: raw.to_string(),
            formatted,
            usd_value: None,
        };

        self.balance_cache.insert(address.to_string(), bal.clone()).await;
        Ok(bal)
    }

    async fn get_token_balance(&self, _token_address: &str, _wallet_address: &str) -> Result<Balance, WalletError> {
        let token_addr = parse_address(_token_address)?;
        let wallet_addr = parse_address(_wallet_address)?;

        let contract = IERC20::new(token_addr, self.provider.clone());

        let balance = contract
            .balanceOf(wallet_addr)
            .call()
            .await
            .map_err(|e| WalletError::RpcError(e.to_string()))?;

        // Metadata calls can fail for non-standard tokens; default conservatively.
        let symbol = contract.symbol().call().await.unwrap_or_else(|_| "TOKEN".into());
        let name = contract.name().call().await.unwrap_or_else(|_| "Token".into());
        let decimals = contract.decimals().call().await.unwrap_or(18);

        let formatted = format_units(balance, decimals).unwrap_or_else(|_| "0.0".to_string());

        let token = TokenInfo {
            symbol,
            name,
            decimals,
            contract_address: Some(_token_address.to_string()),
        };

        Ok(Balance {
            token,
            raw: balance.to_string(),
            formatted,
            usd_value: None,
        })
    }

    async fn estimate_fee(&self, tx: &ChainTransaction) -> Result<Fee, WalletError> {
        let evm_tx = match tx {
            ChainTransaction::Evm(inner) => inner,
        };

        let from = parse_address(&evm_tx.from)?;
        let to = parse_address(&evm_tx.to)?;
        let value = U256::from_str(&evm_tx.value)
            .map_err(|_| WalletError::InvalidAmount(format!("Invalid wei value: {}", evm_tx.value)))?;

        let mut req = TransactionRequest::default();
        req.from = Some(from);
        req.to = Some(TxKind::Call(to));
        req.value = Some(value);

        if let Some(data_hex) = evm_tx.data.as_deref() {
            let input_bytes = hex::decode(data_hex.trim_start_matches("0x"))
                .map_err(|_| WalletError::InvalidTransaction("Invalid hex data".into()))?;
            req.input.input = Some(input_bytes.into());
        }

        // Gas limit estimation (network-dependent).
        let gas_limit = if let Some(gl) = evm_tx.gas_limit {
            gl
        } else {
            self.provider
                .estimate_gas(req.clone())
                .await
                .map_err(|e| WalletError::GasEstimationFailed(e.to_string()))?
        };

        // EIP-1559 heuristic:
        // - If base_fee_per_gas exists (post-London), set:
        //   max_priority_fee_per_gas = 1.5 gwei
        //   max_fee_per_gas = base_fee * 2 + priority
        // - Else fallback to legacy gas_price and treat it as max_fee_per_gas.
        let priority_fee = U256::from(1_500_000_000u64); // 1.5 gwei

        let latest = self
            .provider
            .get_block_by_number(BlockNumberOrTag::Latest)
            .await
            .map_err(|e| WalletError::RpcError(e.to_string()))?;

        let (max_fee_per_gas, max_priority_fee_per_gas) = match latest.and_then(|b| b.header.base_fee_per_gas) {
            Some(base_fee) => {
                let base_fee = U256::from(base_fee);
                let max_fee = base_fee.saturating_mul(U256::from(2u64)).saturating_add(priority_fee);
                (Some(max_fee.to_string()), Some(priority_fee.to_string()))
            }
            None => {
                let gas_price = self.get_gas_price_cached().await?;
                (Some(gas_price), None)
            }
        };

        Ok(Fee {
            gas_limit,
            max_fee_per_gas,
            max_priority_fee_per_gas,
        })
    }

    async fn get_nonce(&self, address: &str) -> Result<u64, WalletError> {
        if let Some(cached) = self.nonce_cache.get(address).await {
            return Ok(cached);
        }
        let addr = parse_address(address)?;
        let nonce = self.provider
            .get_transaction_count(addr)
            .await
            .map_err(|e| WalletError::RpcError(e.to_string()))?;
        self.nonce_cache.insert(address.to_string(), nonce).await;
        Ok(nonce)
    }

    async fn send_transaction(&self, tx: ChainTransaction) -> Result<TxHash, WalletError> {
        let signer = self
            .signer
            .as_ref()
            .ok_or_else(|| WalletError::SigningFailed("No signer configured for EvmAdapter".into()))?;

        let evm_tx = match tx {
            ChainTransaction::Evm(inner) => inner,
        };

        let from = parse_address(&evm_tx.from)?;
        let to = parse_address(&evm_tx.to)?;

        let value = U256::from_str(&evm_tx.value)
            .map_err(|_| WalletError::InvalidAmount(format!("Invalid wei value: {}", evm_tx.value)))?;

        let mut req = TransactionRequest::default();
        req.from = Some(from);
        req.to = Some(TxKind::Call(to));
        req.value = Some(value);
        req.chain_id = Some(evm_tx.chain_id);
        req.nonce = evm_tx.nonce;
        req.gas = evm_tx.gas_limit;

        if let Some(gas_price) = evm_tx.gas_price.as_deref() {
            let gas_price = U256::from_str(gas_price)
                .map_err(|_| WalletError::InvalidAmount(format!("Invalid gas_price wei: {}", gas_price)))?;
            // Alloy TransactionRequest uses u128 for gas_price in this path.
            req.gas_price = Some(gas_price.to::<u128>());
        }

        // EIP-1559 fields (if caller provided them) override legacy gas_price.
        if let Some(max_fee) = evm_tx.max_fee_per_gas.as_deref() {
            let max_fee = U256::from_str(max_fee)
                .map_err(|_| WalletError::InvalidAmount(format!("Invalid max_fee_per_gas: {}", max_fee)))?;
            req.max_fee_per_gas = Some(max_fee.to::<u128>());
        }
        if let Some(prio) = evm_tx.max_priority_fee_per_gas.as_deref() {
            let prio = U256::from_str(prio)
                .map_err(|_| WalletError::InvalidAmount(format!("Invalid max_priority_fee_per_gas: {}", prio)))?;
            req.max_priority_fee_per_gas = Some(prio.to::<u128>());
        }

        if let Some(data_hex) = evm_tx.data.as_deref() {
            let input_bytes =
                hex::decode(data_hex.trim_start_matches("0x")).map_err(|_| WalletError::InvalidTransaction("Invalid hex data".into()))?;
            req.input.input = Some(input_bytes.into());
        }

        let wallet = EthereumWallet::from(signer.clone());
        let envelope = req
            .build(&wallet)
            .await
            .map_err(|e| WalletError::SigningFailed(e.to_string()))?;

        let raw = envelope.encoded_2718();
        let pending = self
            .provider
            .send_raw_transaction(&raw)
            .await
            .map_err(|e| WalletError::TransactionFailed(e.to_string()))?;

        Ok(TxHash(format!("{:?}", pending.tx_hash())))
    }

    async fn get_tx_status(&self, tx_hash: &str) -> Result<crate::chains::TxStatus, WalletError> {
        let h = tx_hash.trim_start_matches("0x");
        let bytes = hex::decode(h).map_err(|_| WalletError::InvalidTransaction("Invalid tx hash hex".into()))?;
        if bytes.len() != 32 {
            return Err(WalletError::InvalidTransaction("Tx hash must be 32 bytes".into()));
        }
        let mut arr = [0u8; 32];
        arr.copy_from_slice(&bytes);
        let b = B256::from(arr);

        let receipt = self
            .provider
            .get_transaction_receipt(b)
            .await
            .map_err(|e| WalletError::RpcError(e.to_string()))?;

        match receipt {
            None => Ok(crate::chains::TxStatus::Pending),
            Some(r) => {
                // Alloy receipt exposes a boolean status helper.
                let ok = r.status();
                Ok(if ok { crate::chains::TxStatus::Confirmed } else { crate::chains::TxStatus::Failed })
            }
        }
    }

    async fn get_transaction_history(&self, address: &str, limit: u32) -> Result<Vec<crate::chains::TxRecord>, WalletError> {
        let net = get_network_by_chain_id(self.chain_id)
            .ok_or_else(|| WalletError::Other("No built-in explorer config for this chain id".into()))?;
        fetch_txlist(&net, address, limit).await
    }

    async fn get_token_transfer_history(&self, address: &str, limit: u32) -> Result<Vec<crate::chains::TxRecord>, WalletError> {
        let net = get_network_by_chain_id(self.chain_id)
            .ok_or_else(|| WalletError::Other("No built-in explorer config for this chain id".into()))?;
        fetch_tokentx(&net, address, limit).await
    }

    fn validate_address(&self, address: &str) -> Result<(), WalletError> {
        parse_address(address).map(|_| ())
    }

    fn chain_info(&self) -> ChainInfo {
        if let Some(net) = get_network_by_chain_id(self.chain_id) {
            ChainInfo {
                chain_type: ChainType::Evm,
                chain_id: net.chain_id,
                name: net.name,
                rpc_url: net.rpc_url,
            }
        } else {
            ChainInfo {
                chain_type: ChainType::Evm,
                chain_id: self.chain_id,
                name: self.network_name.clone(),
                rpc_url: self.rpc_url.clone(),
            }
        }
    }

    fn chain_type(&self) -> ChainType {
        ChainType::Evm
    }
}

