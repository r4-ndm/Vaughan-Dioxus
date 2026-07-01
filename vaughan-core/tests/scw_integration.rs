use std::process::{Child, Command};
use std::str::FromStr;
use std::time::Duration;
use alloy::primitives::{Address, Bytes, U256, TxKind};
use alloy::providers::{Provider, ProviderBuilder};
use alloy::signers::local::PrivateKeySigner;
use alloy::network::EthereumWallet;
use alloy::rpc::types::eth::TransactionRequest;
use tokio::time::sleep;
use vaughan_core::core::ambire_abi::AmbireAccount;
use vaughan_core::core::smart_account::{build_init_code, AMBIRE_ACCOUNT_BYTECODE};
use vaughan_core::core::scw_transaction::{
    build_signed_deploy_and_execute, build_signed_execute, get_smart_account_nonce,
    is_account_deployed,
};

struct AnvilInstance {
    child: Child,
    port: u16,
}

impl AnvilInstance {
    fn new(port: u16) -> Self {
        println!("Spawning anvil on port {}...", port);
        let child = Command::new("/home/r5/.foundry/bin/anvil")
            .arg("--port")
            .arg(port.to_string())
            .spawn()
            .expect("failed to spawn anvil");
        Self { child, port }
    }
}

impl Drop for AnvilInstance {
    fn drop(&mut self) {
        println!("Killing anvil on port {}...", self.port);
        let _ = self.child.kill();
    }
}

#[tokio::test]
async fn test_smart_account_deployment_and_execution() {
    let anvil_port = 8599;
    let _anvil = AnvilInstance::new(anvil_port);

    // Wait a moment for anvil to boot up
    sleep(Duration::from_secs(2)).await;

    let rpc_url = format!("http://127.0.0.1:{}", anvil_port);
    let provider = ProviderBuilder::new().connect_http(rpc_url.parse().unwrap());

    // Anvil default account #0 (deployer/funder)
    // Private key: 0xac0974bec39a17e36ba4a6b4d238ff944bacb478cbed5efcae784d7bf4f2ff80
    let dev_signer = PrivateKeySigner::from_str("ac0974bec39a17e36ba4a6b4d238ff944bacb478cbed5efcae784d7bf4f2ff80").unwrap();
    let dev_address = dev_signer.address();

    // Deploy AmbireAccountFactory
    // Load bytecode from compiled artifact
    let factory_json_str = std::fs::read_to_string("../vaughan-contracts/out/AmbireAccountFactory.sol/AmbireAccountFactory.json")
        .or_else(|_| std::fs::read_to_string("vaughan-contracts/out/AmbireAccountFactory.sol/AmbireAccountFactory.json"))
        .or_else(|_| std::fs::read_to_string("./vaughan-contracts/out/AmbireAccountFactory.sol/AmbireAccountFactory.json"))
        .expect("could not read factory artifact");
    let factory_json: serde_json::Value = serde_json::from_str(&factory_json_str).unwrap();
    let factory_bytecode_hex = factory_json["bytecode"]["object"].as_str().unwrap().trim_start_matches("0x");
    let factory_bytecode = hex::decode(factory_bytecode_hex).unwrap();

    // The factory constructor takes (address allowedToDrain) - let's pass a dummy address
    let dummy_drain = Address::repeat_byte(0x0a);
    let constructor_args = alloy::sol_types::SolValue::abi_encode(&dummy_drain);
    let mut deploy_code = factory_bytecode.clone();
    deploy_code.extend_from_slice(&constructor_args);

    let wallet = EthereumWallet::from(dev_signer.clone());
    let provider_with_wallet = ProviderBuilder::new()
        .wallet(wallet)
        .connect_http(format!("http://127.0.0.1:{}", anvil_port).parse().unwrap());

    let tx_req = TransactionRequest {
        from: Some(dev_address),
        to: Some(TxKind::Create),
        input: alloy::rpc::types::eth::TransactionInput {
            input: Some(Bytes::from(deploy_code)),
            data: None,
        },
        ..Default::default()
    };

    let pending_deploy = provider_with_wallet.send_transaction(tx_req).await.unwrap();
    let receipt = pending_deploy.get_receipt().await.unwrap();
    let factory_address = receipt.contract_address.expect("factory deployment failed");
    println!("Deployed AmbireAccountFactory at: {:?}", factory_address);

    // 1. Derive Smart Account Address (CREATE2)
    // EOA Owner: Anvil account #1
    // Private key: 0x59c6995e998f97a5a0044966f0945389dc9e86dae88c7a8412f4603b6b78690d
    let owner_signer = PrivateKeySigner::from_str("59c6995e998f97a5a0044966f0945389dc9e86dae88c7a8412f4603b6b78690d").unwrap();
    let owner_address = owner_signer.address();
    let salt = U256::from(123456789u64);

    let (scw_address, _init_code_hash) = vaughan_core::core::smart_account::derive_smart_account_address(
        factory_address,
        owner_address,
        salt,
        AMBIRE_ACCOUNT_BYTECODE,
    );
    println!("Derived Smart Account Address: {:?}", scw_address);

    // Fund the Smart Account Address on-chain so it can pay for transfer value / deploy cost
    let fund_tx = TransactionRequest {
        from: Some(dev_address),
        to: Some(TxKind::Call(scw_address)),
        value: Some(U256::from(2_000_000_000_000_000_000u128)), // 2 ETH
        ..Default::default()
    };
    provider_with_wallet.send_transaction(fund_tx).await.unwrap().get_receipt().await.unwrap();

    let balance = provider.get_balance(scw_address).await.unwrap();
    assert_eq!(balance, U256::from(2_000_000_000_000_000_000u128));

    // Confirm that the account is NOT deployed yet
    let is_deployed = is_account_deployed(scw_address, &provider).await.unwrap();
    assert!(!is_deployed);

    // 2. Perform `deployAndExecute` (Counterfactual deployment + transaction batch execution)
    // Transaction batch contains one transaction: transfer 1 ETH to dummy recipient
    let recipient = Address::repeat_byte(0xbb);
    let inner_txn = AmbireAccount::Transaction {
        to: recipient,
        value: U256::from(1_000_000_000_000_000_000u128), // 1 ETH
        data: Bytes::new(),
    };

    let init_code = build_init_code(owner_address, AMBIRE_ACCOUNT_BYTECODE);
    let chain_id = 31337; // Anvil standard chain id

    let calldata = build_signed_deploy_and_execute(
        &owner_signer,
        scw_address,
        init_code,
        salt,
        vec![inner_txn.clone()],
        chain_id,
    )
    .await
    .unwrap();

    // Send transaction (outer call to Factory)
    let outer_tx = TransactionRequest {
        from: Some(owner_address),
        to: Some(TxKind::Call(factory_address)),
        input: alloy::rpc::types::eth::TransactionInput {
            input: Some(calldata),
            data: None,
        },
        ..Default::default()
    };

    let owner_wallet = EthereumWallet::from(owner_signer.clone());
    let provider_with_owner = ProviderBuilder::new()
        .wallet(owner_wallet)
        .connect_http(format!("http://127.0.0.1:{}", anvil_port).parse().unwrap());

    // Fund the owner EOA so it can pay gas for the deployAndExecute call
    let fund_owner_tx = TransactionRequest {
        from: Some(dev_address),
        to: Some(TxKind::Call(owner_address)),
        value: Some(U256::from(10_000_000_000_000_000_000u128)), // 10 ETH
        ..Default::default()
    };
    provider_with_wallet.send_transaction(fund_owner_tx).await.unwrap().get_receipt().await.unwrap();

    let pending_scw_tx = provider_with_owner.send_transaction(outer_tx).await.unwrap();
    pending_scw_tx.get_receipt().await.unwrap();

    // Verify smart account is now deployed
    let is_deployed_after = is_account_deployed(scw_address, &provider).await.unwrap();
    assert!(is_deployed_after);

    // Verify recipient received the 1 ETH
    let recipient_balance = provider.get_balance(recipient).await.unwrap();
    assert_eq!(recipient_balance, U256::from(1_000_000_000_000_000_000u128));

    // 3. Perform standard `execute` transaction on the already deployed smart account
    let inner_txn_2 = AmbireAccount::Transaction {
        to: recipient,
        value: U256::from(500_000_000_000_000_000u128), // 0.5 ETH
        data: Bytes::new(),
    };

    let nonce = get_smart_account_nonce(scw_address, &provider).await.unwrap();
    let calldata_execute = build_signed_execute(
        &owner_signer,
        scw_address,
        vec![inner_txn_2],
        nonce,
        chain_id,
    )
    .await
    .unwrap();

    let outer_tx_execute = TransactionRequest {
        from: Some(owner_address),
        to: Some(TxKind::Call(scw_address)),
        input: alloy::rpc::types::eth::TransactionInput {
            input: Some(calldata_execute),
            data: None,
        },
        ..Default::default()
    };

    let pending_execute_tx = provider_with_owner.send_transaction(outer_tx_execute).await.unwrap();
    pending_execute_tx.get_receipt().await.unwrap();

    // Verify recipient received the additional 0.5 ETH
    let recipient_balance_after = provider.get_balance(recipient).await.unwrap();
    assert_eq!(recipient_balance_after, U256::from(1_500_000_000_000_000_000u128));

    println!("All integration test assertions passed successfully!");
}
