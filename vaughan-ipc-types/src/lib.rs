//! Shared IPC types between Dioxus wallet and Tauri dApp browser.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Handshake {
    pub version: u32,
    pub token: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", content = "payload")]
pub enum IpcRequest {
    GetAccounts,
    SignTransaction(SignTxPayload),
    GetNetworkInfo,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SignTxPayload {
    pub from: String,
    pub to: String,
    pub value: String,
    pub chain_id: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", content = "payload")]
pub enum IpcResponse {
    Accounts(Vec<AccountInfo>),
    SignedTransaction(String),
    NetworkInfo(NetworkInfo),
    Error { code: u32, message: String },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AccountInfo {
    pub address: String,
    pub name: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NetworkInfo {
    pub chain_id: u64,
    pub name: String,
}
