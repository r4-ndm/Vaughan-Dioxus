use alloy::primitives::{keccak256, Address, Bytes, U256};
use alloy::providers::Provider;
use alloy::signers::local::PrivateKeySigner;
use alloy::signers::Signer;
use alloy::sol_types::SolValue;

use crate::chains::{ChainTransaction, EvmTransaction};
use crate::core::ambire_abi::{AmbireAccount, AmbireAccountFactory};
use crate::error::WalletError;

/// Build the signing hash for AmbireAccount.execute().
///
/// Formula: `keccak256(abi.encode(accountAddress, chainId, nonce, txns))`
pub fn build_execute_hash(
    smart_account_addr: Address,
    chain_id: u64,
    nonce: U256,
    txns: &[AmbireAccount::Transaction],
) -> alloy::primitives::B256 {
    // abi.encode(address, uint256, uint256, Transaction[])
    let encoded = (
        smart_account_addr,
        U256::from(chain_id),
        nonce,
        txns.to_vec(),
    ).abi_encode_sequence();
    keccak256(&encoded)
}

/// Sign and build execute() calldata for a deployed smart account.
///
/// Returns the ABI-encoded calldata for `AmbireAccount.execute(txns, signature)`.
pub async fn build_signed_execute(
    signer: &PrivateKeySigner,
    smart_account_addr: Address,
    txns: Vec<AmbireAccount::Transaction>,
    nonce: U256,
    chain_id: u64,
) -> Result<Bytes, WalletError> {
    let hash = build_execute_hash(smart_account_addr, chain_id, nonce, &txns);

    let sig = signer
        .sign_hash(&hash)
        .await
        .map_err(|e| WalletError::SigningFailed(e.to_string()))?;

    // 66-byte signature: r(32) + s(32) + v(1) + mode(1)
    let mut sig_bytes = sig.as_bytes().to_vec(); // 65 bytes
    sig_bytes.push(0x00); // SignatureMode::EIP712

    let call = AmbireAccount::executeCall {
        txns,
        signature: sig_bytes.into(),
    };

    Ok(Bytes::from(alloy::sol_types::SolCall::abi_encode(&call)))
}

/// Sign and build deployAndExecute() calldata for a NOT-yet-deployed smart account.
///
/// Returns the ABI-encoded calldata for
/// `AmbireAccountFactory.deployAndExecute(code, salt, txns, signature)`.
pub async fn build_signed_deploy_and_execute(
    signer: &PrivateKeySigner,
    smart_account_addr: Address,
    init_code: Vec<u8>,
    salt: U256,
    txns: Vec<AmbireAccount::Transaction>,
    chain_id: u64,
) -> Result<Bytes, WalletError> {
    // Hash uses nonce=0 (account not deployed yet)
    let hash = build_execute_hash(smart_account_addr, chain_id, U256::ZERO, &txns);

    let sig = signer
        .sign_hash(&hash)
        .await
        .map_err(|e| WalletError::SigningFailed(e.to_string()))?;

    let mut sig_bytes = sig.as_bytes().to_vec();
    sig_bytes.push(0x00);

    let call = AmbireAccountFactory::deployAndExecuteCall {
        code: init_code.into(),
        salt,
        txns,
        signature: sig_bytes.into(),
    };

    Ok(Bytes::from(alloy::sol_types::SolCall::abi_encode(&call)))
}

/// Wrap SCW calldata into a ChainTransaction::Evm suitable for the existing adapter.
///
/// `target` is the smart account address (for execute) or factory address (for deployAndExecute).
pub fn wrap_scw_as_chain_transaction(
    signer_eoa: Address,
    target: Address,
    calldata: &Bytes,
    chain_id: u64,
) -> ChainTransaction {
    ChainTransaction::Evm(EvmTransaction {
        from: format!("{signer_eoa:?}"),
        to: format!("{target:?}"),
        value: "0".into(),
        data: Some(format!("0x{}", hex::encode(calldata))),
        gas_limit: None,
        gas_price: None,
        max_fee_per_gas: None,
        max_priority_fee_per_gas: None,
        nonce: None,
        chain_id,
    })
}

/// Fetch the on-chain nonce for a smart account.
/// Returns 0 if the account is not yet deployed.
pub async fn get_smart_account_nonce(
    account_addr: Address,
    provider: &impl Provider,
) -> Result<U256, WalletError> {
    let code = provider
        .get_code_at(account_addr)
        .await
        .map_err(|e| WalletError::RpcError(e.to_string()))?;

    if code.is_empty() {
        return Ok(U256::ZERO);
    }

    let contract = AmbireAccount::new(account_addr, provider);
    contract
        .nonce()
        .call()
        .await
        .map_err(|e| WalletError::RpcError(e.to_string()))
}

/// Check if a smart account is deployed on-chain.
pub async fn is_account_deployed(
    account_addr: Address,
    provider: &impl Provider,
) -> Result<bool, WalletError> {
    let code = provider
        .get_code_at(account_addr)
        .await
        .map_err(|e| WalletError::RpcError(e.to_string()))?;
    Ok(!code.is_empty())
}
