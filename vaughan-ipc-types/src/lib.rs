//! Shared IPC types between Dioxus wallet and Tauri dApp browser.
//!
//! This crate is intentionally dependency-light. Validation is basic and is meant
//! to prevent obvious malformed messages at the IPC boundary.

use serde::{Deserialize, Serialize};

/// Current IPC protocol version.
pub const IPC_VERSION: u32 = 1;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Handshake {
    pub version: u32,
    pub token: String,
}

impl Handshake {
    pub fn validate(&self) -> Result<(), ValidationError> {
        if self.version != IPC_VERSION {
            return Err(ValidationError::UnsupportedVersion(self.version));
        }
        if self.token.trim().is_empty() {
            return Err(ValidationError::InvalidToken);
        }
        Ok(())
    }
}

/// Request envelope with a correlation id so multiple in-flight requests can be multiplexed.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IpcEnvelope<T> {
    pub id: u64,
    pub body: T,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", content = "payload")]
pub enum IpcRequest {
    GetAccounts,
    SignTransaction(SignTxPayload),
    SignMessage(SignMessagePayload),
    SignTypedData(SignTypedDataPayload),
    SwitchChain(SwitchChainPayload),
    GetNetworkInfo,
}

impl IpcRequest {
    pub fn validate(&self) -> Result<(), ValidationError> {
        match self {
            IpcRequest::GetAccounts | IpcRequest::GetNetworkInfo => Ok(()),
            IpcRequest::SignTransaction(p) => p.validate(),
            IpcRequest::SignMessage(p) => p.validate(),
            IpcRequest::SignTypedData(p) => p.validate(),
            IpcRequest::SwitchChain(p) => p.validate(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SignTxPayload {
    pub from: String,
    pub to: String,
    pub value: String,
    pub chain_id: u64,
}

impl SignTxPayload {
    pub fn validate(&self) -> Result<(), ValidationError> {
        validate_evm_address(&self.from)?;
        validate_evm_address(&self.to)?;
        validate_decimal_u256ish(&self.value)?;
        validate_chain_id(self.chain_id)?;
        Ok(())
    }
}

/// Sign an arbitrary message (EIP-191 personal_sign style).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SignMessagePayload {
    pub address: String,
    /// Hex string (`0x...`) or UTF-8 string depending on caller conventions.
    pub message: String,
    pub chain_id: u64,
}

impl SignMessagePayload {
    pub fn validate(&self) -> Result<(), ValidationError> {
        validate_evm_address(&self.address)?;
        if self.message.trim().is_empty() {
            return Err(ValidationError::InvalidMessage);
        }
        validate_chain_id(self.chain_id)?;
        Ok(())
    }
}

/// Sign typed data (EIP-712) payload.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SignTypedDataPayload {
    pub address: String,
    /// JSON string representing typed data (wallet will parse/validate further).
    pub typed_data_json: String,
    pub chain_id: u64,
}

impl SignTypedDataPayload {
    pub fn validate(&self) -> Result<(), ValidationError> {
        validate_evm_address(&self.address)?;
        if self.typed_data_json.trim().is_empty() {
            return Err(ValidationError::InvalidTypedData);
        }
        validate_chain_id(self.chain_id)?;
        Ok(())
    }
}

/// Request chain/network change (EIP-3326 / wallet_switchEthereumChain style).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SwitchChainPayload {
    pub chain_id: u64,
}

impl SwitchChainPayload {
    pub fn validate(&self) -> Result<(), ValidationError> {
        validate_chain_id(self.chain_id)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", content = "payload")]
pub enum IpcResponse {
    Accounts(Vec<AccountInfo>),
    SignedTransaction(String),
    SignedMessage(String),
    SignedTypedData(String),
    NetworkInfo(NetworkInfo),
    Error { code: u32, message: String },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AccountInfo {
    pub address: String,
    pub name: Option<String>,
}

impl AccountInfo {
    pub fn validate(&self) -> Result<(), ValidationError> {
        validate_evm_address(&self.address)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NetworkInfo {
    pub chain_id: u64,
    pub name: String,
}

impl NetworkInfo {
    pub fn validate(&self) -> Result<(), ValidationError> {
        validate_chain_id(self.chain_id)?;
        if self.name.trim().is_empty() {
            return Err(ValidationError::InvalidNetworkName);
        }
        Ok(())
    }
}

/// Validation errors for IPC messages.
#[derive(Debug, Clone, thiserror::Error, PartialEq, Eq)]
pub enum ValidationError {
    #[error("unsupported IPC version: {0}")]
    UnsupportedVersion(u32),
    #[error("invalid token")]
    InvalidToken,
    #[error("invalid EVM address")]
    InvalidAddress,
    #[error("invalid decimal value")]
    InvalidValue,
    #[error("invalid chain id")]
    InvalidChainId,
    #[error("invalid message")]
    InvalidMessage,
    #[error("invalid typed data")]
    InvalidTypedData,
    #[error("invalid network name")]
    InvalidNetworkName,
}

fn validate_chain_id(chain_id: u64) -> Result<(), ValidationError> {
    if chain_id == 0 {
        Err(ValidationError::InvalidChainId)
    } else {
        Ok(())
    }
}

fn validate_evm_address(addr: &str) -> Result<(), ValidationError> {
    let a = addr.trim();
    if a.len() != 42 || !a.starts_with("0x") {
        return Err(ValidationError::InvalidAddress);
    }
    if a.as_bytes()[2..].iter().all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f' | b'A'..=b'F')) {
        Ok(())
    } else {
        Err(ValidationError::InvalidAddress)
    }
}

fn validate_decimal_u256ish(value: &str) -> Result<(), ValidationError> {
    let v = value.trim();
    if v.is_empty() {
        return Err(ValidationError::InvalidValue);
    }
    if !v.as_bytes().iter().all(|b| matches!(b, b'0'..=b'9')) {
        return Err(ValidationError::InvalidValue);
    }
    // We accept decimal values that fit within a typical U256 digit budget.
    // (Max U256 is ~78 decimal digits; we use 78 as a pragmatic upper bound.)
    if v.len() > 78 {
        return Err(ValidationError::InvalidValue);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn serde_roundtrip_request() {
        let req = IpcEnvelope {
            id: 1,
            body: IpcRequest::SignTransaction(SignTxPayload {
                from: "0x0000000000000000000000000000000000000001".into(),
                to: "0x0000000000000000000000000000000000000002".into(),
                value: "1".into(),
                chain_id: 1,
            }),
        };
        let s = serde_json::to_string(&req).unwrap();
        let back: IpcEnvelope<IpcRequest> = serde_json::from_str(&s).unwrap();
        assert_eq!(back.id, 1);
        back.body.validate().unwrap();
    }

    #[test]
    fn serde_roundtrip_typed_data_response() {
        let resp = IpcResponse::SignedTypedData("0xsig".into());
        let s = serde_json::to_string(&resp).unwrap();
        let back: IpcResponse = serde_json::from_str(&s).unwrap();
        match back {
            IpcResponse::SignedTypedData(s) => assert_eq!(s, "0xsig"),
            _ => panic!("unexpected response type"),
        }
    }

    #[test]
    fn validation_rejects_bad_address() {
        let p = SignTxPayload {
            from: "0x123".into(),
            to: "0x0000000000000000000000000000000000000002".into(),
            value: "1".into(),
            chain_id: 1,
        };
        assert_eq!(p.validate().unwrap_err(), ValidationError::InvalidAddress);
    }

    #[test]
    fn handshake_validation() {
        let h = Handshake { version: IPC_VERSION, token: "abc".into() };
        h.validate().unwrap();
        let bad = Handshake { version: 999, token: "abc".into() };
        assert!(matches!(bad.validate(), Err(ValidationError::UnsupportedVersion(999))));
    }
}
