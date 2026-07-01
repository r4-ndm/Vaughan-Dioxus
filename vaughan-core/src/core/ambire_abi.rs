use alloy::sol;

sol! {
    #[allow(missing_docs)]
    #[derive(Debug)]
    #[sol(rpc)]
    contract AmbireAccount {
        struct Transaction {
            address to;
            uint256 value;
            bytes data;
        }

        // execute(txns, signature) — nonce is managed internally (public, not payable)
        function execute(Transaction[] calldata txns, bytes calldata signature) public;

        // Public state variable getter
        function nonce() external view returns (uint256);

        // EIP-1271
        function isValidSignature(bytes32 hash, bytes calldata signature)
            external view returns (bytes4);

        // Privilege check
        function privileges(address addr) external view returns (bytes32);
    }

    #[allow(missing_docs)]
    #[derive(Debug)]
    #[sol(rpc)]
    contract AmbireAccountFactory {
        // Both return void — compute address client-side via CREATE2
        function deploy(bytes calldata code, uint256 salt) external;

        function deployAndExecute(
            bytes calldata code,
            uint256 salt,
            AmbireAccount.Transaction[] calldata txns,
            bytes calldata signature
        ) external;
    }
}
