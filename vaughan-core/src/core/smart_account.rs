use alloy::primitives::{Address, U256, keccak256};
use alloy::sol_types::SolValue;

/// AmbireAccount creation bytecode — populated by build.rs from compiled artifacts.
pub const AMBIRE_ACCOUNT_BYTECODE: &[u8] = include_bytes!(
    concat!(env!("OUT_DIR"), "/ambire_account_bytecode.bin")
);

/// Derive the CREATE2 smart account address and init code hash.
///
/// Returns `(smart_account_address, init_code_hash)`.
pub fn derive_smart_account_address(
    factory: Address,
    owner: Address,
    salt: U256,
    creation_bytecode: &[u8],
) -> (Address, [u8; 32]) {
    // AmbireAccount constructor: constructor(address[] memory addrs)
    let constructor_args = vec![owner].abi_encode();
    let init_code = [creation_bytecode, &constructor_args].concat();
    let init_code_hash = keccak256(&init_code);

    let mut input = Vec::with_capacity(1 + 20 + 32 + 32);
    input.push(0xff);
    input.extend_from_slice(factory.as_slice());
    input.extend_from_slice(&salt.to_be_bytes::<32>());
    input.extend_from_slice(init_code_hash.as_slice());

    let address = Address::from_slice(&keccak256(&input)[12..]);
    (address, init_code_hash.into())
}

/// Build the full init code for deploying a smart account.
pub fn build_init_code(owner: Address, creation_bytecode: &[u8]) -> Vec<u8> {
    let constructor_args = vec![owner].abi_encode();
    [creation_bytecode, &constructor_args].concat()
}

/// Generate a random salt for a new smart account.
pub fn generate_salt() -> U256 {
    U256::from_be_bytes(rand::random::<[u8; 32]>())
}
