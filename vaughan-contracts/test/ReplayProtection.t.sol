// SPDX-License-Identifier: MIT
pragma solidity ^0.8.7;

import "forge-std/Test.sol";
import "../src/AmbireAccount.sol";
import "../src/AmbireAccountFactory.sol";

contract ReplayProtectionTest is Test {
    AmbireAccountFactory factory;
    address payable expectedAddr;

    uint256 ownerPk;
    address owner;

    function setUp() public {
        ownerPk = 0xac0974bec39a17e36ba4a6b4d238ff944bacb478cbed5efcae784d7bf4f2ff80;
        owner = vm.addr(ownerPk);

        factory = new AmbireAccountFactory(address(0));

        // Build SCW deployment initCode
        address[] memory addrs = new address[](1);
        addrs[0] = owner;
        bytes memory initCode = abi.encodePacked(
            type(AmbireAccount).creationCode,
            abi.encode(addrs)
        );

        // Derive CREATE2 address
        uint256 salt = 1111;
        expectedAddr = payable(address(uint160(uint256(
            keccak256(abi.encodePacked(
                bytes1(0xff), address(factory), salt, keccak256(initCode)
            ))
        ))));

        // Fund and deploy the wallet
        vm.deal(expectedAddr, 10 ether);
        factory.deploy(initCode, salt);
    }

    // 1. Nonce Replay Prevention
    function testRejectReplayingSameTransaction() public {
        address recipient = address(0xCAFE);
        AmbireAccount.Transaction[] memory txns = new AmbireAccount.Transaction[](1);
        txns[0] = AmbireAccount.Transaction({
            to: recipient,
            value: 1 ether,
            data: ""
        });

        // Sign transaction with nonce = 0
        uint256 nonce = AmbireAccount(expectedAddr).nonce();
        assertEq(nonce, 0, "Initial nonce must be 0");

        bytes32 hash = keccak256(abi.encode(expectedAddr, block.chainid, nonce, txns));
        (uint8 v, bytes32 r, bytes32 s) = vm.sign(ownerPk, hash);
        bytes memory sig = abi.encodePacked(r, s, v, uint8(0x00));

        // Execute transaction (nonce increments to 1)
        AmbireAccount(expectedAddr).execute(txns, sig);
        assertEq(recipient.balance, 1 ether, "Recipient should receive 1 ETH");
        assertEq(AmbireAccount(expectedAddr).nonce(), 1, "Nonce should increment to 1");

        // Attempt to replay the exact same transaction and signature
        // This must revert because the current contract nonce is 1, but the signature was for nonce 0
        vm.expectRevert("INSUFFICIENT_PRIVILEGE");
        AmbireAccount(expectedAddr).execute(txns, sig);
    }

    // 2. Chain ID Replay Prevention (e.g. cross-chain signature replays)
    function testRejectCrossChainReplay() public {
        address recipient = address(0xCAFE);
        AmbireAccount.Transaction[] memory txns = new AmbireAccount.Transaction[](1);
        txns[0] = AmbireAccount.Transaction({
            to: recipient,
            value: 1 ether,
            data: ""
        });

        // Target chain ID is 1 (Ethereum Mainnet) instead of current test chain ID (31337)
        uint256 wrongChainId = 1;
        uint256 nonce = AmbireAccount(expectedAddr).nonce();

        // Sign the hash containing the wrong chain ID
        bytes32 hashWrongChain = keccak256(abi.encode(expectedAddr, wrongChainId, nonce, txns));
        (uint8 v, bytes32 r, bytes32 s) = vm.sign(ownerPk, hashWrongChain);
        bytes memory sigWrongChain = abi.encodePacked(r, s, v, uint8(0x00));

        // Attempting to execute on test chain (chainid 31337) must fail because the reconstructed hash mismatches
        vm.expectRevert("INSUFFICIENT_PRIVILEGE");
        AmbireAccount(expectedAddr).execute(txns, sigWrongChain);
    }
}
