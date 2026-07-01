// SPDX-License-Identifier: MIT
pragma solidity ^0.8.7;

import "forge-std/Test.sol";
import "../src/AmbireAccount.sol";
import "../src/AmbireAccountFactory.sol";

contract NestedAccountsTest is Test {
    AmbireAccountFactory factory;
    
    // Smart Account A (owned by EOA owner)
    address payable accountA;
    // Smart Account B (owned by Smart Account A)
    address payable accountB;

    uint256 ownerPk;
    address owner;

    function setUp() public {
        ownerPk = 0xac0974bec39a17e36ba4a6b4d238ff944bacb478cbed5efcae784d7bf4f2ff80;
        owner = vm.addr(ownerPk);

        factory = new AmbireAccountFactory(address(0));

        // 1. Deploy Smart Account A (owned by EOA owner)
        address[] memory addrsA = new address[](1);
        addrsA[0] = owner;
        bytes memory initCodeA = abi.encodePacked(
            type(AmbireAccount).creationCode,
            abi.encode(addrsA)
        );
        uint256 saltA = 2222;
        accountA = payable(address(uint160(uint256(
            keccak256(abi.encodePacked(
                bytes1(0xff), address(factory), saltA, keccak256(initCodeA)
            ))
        ))));
        factory.deploy(initCodeA, saltA);

        // 2. Deploy Smart Account B (owned by Smart Account A)
        address[] memory addrsB = new address[](1);
        addrsB[0] = accountA; // Smart Account A is the owner of Smart Account B
        bytes memory initCodeB = abi.encodePacked(
            type(AmbireAccount).creationCode,
            abi.encode(addrsB)
        );
        uint256 saltB = 3333;
        accountB = payable(address(uint160(uint256(
            keccak256(abi.encodePacked(
                bytes1(0xff), address(factory), saltB, keccak256(initCodeB)
            ))
        ))));
        factory.deploy(initCodeB, saltB);

        // Fund Account B with native ETH
        vm.deal(accountB, 10 ether);
    }

    function testNestedSignatureExecution() public {
        address recipient = address(0xCAFE);
        AmbireAccount.Transaction[] memory txns = new AmbireAccount.Transaction[](1);
        txns[0] = AmbireAccount.Transaction({
            to: recipient,
            value: 1 ether,
            data: ""
        });

        // Current nonce of Account B
        uint256 nonceB = AmbireAccount(accountB).nonce();
        assertEq(nonceB, 0, "Account B nonce should be 0");

        // The hash calculated by Account B
        bytes32 hashB = keccak256(abi.encode(accountB, block.chainid, nonceB, txns));

        // 1. Create the signature from the EOA owner for Account A
        // Because Account A will be asked to validate hashB, we sign hashB using Account A's owner key
        (uint8 v, bytes32 r, bytes32 s) = vm.sign(ownerPk, hashB);
        // Signature for Account A's internal validation (SignatureMode.EIP712 = 0x00)
        bytes memory innerSig = abi.encodePacked(r, s, v, uint8(0x00));

        // 2. Wrap it in a SmartWallet signature for Account B
        // Structure: [innerSignature] + [walletAddress (32 bytes)] + [SignatureMode.SmartWallet (0x02)]
        bytes memory wrappedSig = abi.encodePacked(
            innerSig,
            bytes32(uint256(uint160(address(accountA)))),
            uint8(0x02) // SignatureMode.SmartWallet
        );

        // Verify that Account B accepts the signature and executes the transaction
        AmbireAccount(accountB).execute(txns, wrappedSig);

        // Verify balances and nonce state
        assertEq(recipient.balance, 1 ether, "Recipient should receive 1 ETH from B");
        assertEq(AmbireAccount(accountB).nonce(), 1, "Account B nonce should increment");
    }
}
