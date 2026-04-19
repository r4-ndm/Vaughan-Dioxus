//! Transaction history retrieval for EVM chains via Etherscan-compatible APIs.
//!
//! This is a pragmatic approach: standard Ethereum JSON-RPC does not provide
//! account history without an indexer, so we use explorer APIs when available.

use serde::Deserialize;

use crate::chains::evm::networks::EvmNetworkConfig;
use crate::chains::{TxRecord, TxStatus};
use crate::error::WalletError;

#[derive(Debug, Deserialize)]
struct ExplorerResponse<T> {
    status: String,
    message: String,
    result: T,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ExplorerTx {
    hash: String,
    from: String,
    to: String,
    value: String,
    time_stamp: String,
    block_number: String,
    gas_used: Option<String>,
    is_error: Option<String>,
    // ERC20 fields (present in tokentx)
    token_symbol: Option<String>,
    contract_address: Option<String>,
    token_decimal: Option<String>,
}

fn parse_u64(s: &str) -> Option<u64> {
    s.parse::<u64>().ok()
}

fn parse_status(is_error: Option<&str>) -> TxStatus {
    match is_error {
        Some("1") => TxStatus::Failed,
        Some("0") => TxStatus::Confirmed,
        _ => TxStatus::Confirmed,
    }
}

pub async fn fetch_txlist(
    net: &EvmNetworkConfig,
    address: &str,
    limit: u32,
) -> Result<Vec<TxRecord>, WalletError> {
    let api = net
        .explorer_api_url
        .as_deref()
        .ok_or_else(|| WalletError::Other("Explorer API not configured for this network".into()))?;

    let url = format!(
        "{api}?module=account&action=txlist&address={address}&startblock=0&endblock=99999999&sort=desc&page=1&offset={limit}"
    );

    let resp = reqwest::get(&url)
        .await
        .map_err(|e| WalletError::NetworkError(e.to_string()))?;

    let body: ExplorerResponse<serde_json::Value> = resp
        .json()
        .await
        .map_err(|e| WalletError::RpcError(e.to_string()))?;

    // Etherscan returns `result` as either an array or a string on error.
    let _message = body.message;
    if body.status != "1" {
        return Ok(vec![]);
    }

    let txs: Vec<ExplorerTx> = serde_json::from_value(body.result)
        .map_err(|e| WalletError::RpcError(format!("Explorer parse failed: {}", e)))?;

    Ok(txs
        .into_iter()
        .map(|t| TxRecord {
            hash: t.hash,
            from: t.from,
            to: t.to,
            value: t.value,
            status: parse_status(t.is_error.as_deref()),
            block_number: parse_u64(&t.block_number),
            timestamp: parse_u64(&t.time_stamp),
            gas_used: t.gas_used.as_deref().and_then(parse_u64),
            token_symbol: None,
            token_address: None,
            is_token_transfer: false,
            token_decimals: None,
        })
        .collect())
}

pub async fn fetch_tokentx(
    net: &EvmNetworkConfig,
    address: &str,
    limit: u32,
) -> Result<Vec<TxRecord>, WalletError> {
    let api = net
        .explorer_api_url
        .as_deref()
        .ok_or_else(|| WalletError::Other("Explorer API not configured for this network".into()))?;

    let url = format!(
        "{api}?module=account&action=tokentx&address={address}&startblock=0&endblock=99999999&sort=desc&page=1&offset={limit}"
    );

    let resp = reqwest::get(&url)
        .await
        .map_err(|e| WalletError::NetworkError(e.to_string()))?;

    let body: ExplorerResponse<serde_json::Value> = resp
        .json()
        .await
        .map_err(|e| WalletError::RpcError(e.to_string()))?;

    let _message = body.message;
    if body.status != "1" {
        return Ok(vec![]);
    }

    let txs: Vec<ExplorerTx> = serde_json::from_value(body.result)
        .map_err(|e| WalletError::RpcError(format!("Explorer parse failed: {}", e)))?;

    Ok(txs
        .into_iter()
        .map(|t| TxRecord {
            hash: t.hash,
            from: t.from,
            to: t.to,
            value: t.value,
            status: parse_status(t.is_error.as_deref()),
            block_number: parse_u64(&t.block_number),
            timestamp: parse_u64(&t.time_stamp),
            gas_used: t.gas_used.as_deref().and_then(parse_u64),
            token_symbol: t.token_symbol,
            token_address: t.contract_address,
            is_token_transfer: true,
            token_decimals: t
                .token_decimal
                .as_deref()
                .and_then(|s| s.parse::<u8>().ok()),
        })
        .collect())
}
