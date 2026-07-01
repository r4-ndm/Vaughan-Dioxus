// SPDX-License-Identifier: MIT
pragma solidity ^0.8.7;

import "forge-std/Test.sol";
import "../src/AmbireAccount.sol";
import "../src/AmbireAccountFactory.sol";

contract SecurityTest is Test {
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

        // Derive counterfactual address
        uint256 salt = 888;
        expectedAddr = payable(address(uint160(uint256(
            keccak256(abi.encodePacked(
                bytes1(0xff), address(factory), salt, keccak256(initCode)
            ))
        ))));

        // Fund and deploy the wallet
        vm.deal(expectedAddr, 5 ether);
        factory.deploy(initCode, salt);
    }

    // 1. Rejection of Unprivileged Signers
    function testRejectUnprivilegedSigner() public {
        uint256 attackerPk = 0xbadbadbadbadbadbadbadbadbadbadbadbadbadbadbadbadbadbadbadbadbad1;
        address attacker = vm.addr(attackerPk);

        AmbireAccount.Transaction[] memory txns = new AmbireAccount.Transaction[](1);
        txns[0] = AmbireAccount.Transaction({
            to: address(0xCAFE),
            value: 1 ether,
            data: ""
        });

        uint256 nonce = AmbireAccount(expectedAddr).nonce();
        bytes32 hash = keccak256(abi.encode(expectedAddr, block.chainid, nonce, txns));
        
        // Attacker signs the transaction hash
        (uint8 v, bytes32 r, bytes32 s) = vm.sign(attackerPk, hash);
        bytes memory sig = abi.encodePacked(r, s, v, uint8(0x00));

        // Attacker attempts to execute the transaction
        vm.expectRevert("INSUFFICIENT_PRIVILEGE");
        AmbireAccount(expectedAddr).execute(txns, sig);
    }

    // 2. Direct Call Rejection (EVM msg.sender checks)
    function testRejectDirectExecution() public {
        AmbireAccount.Transaction[] memory txns = new AmbireAccount.Transaction[](1);
        txns[0] = AmbireAccount.Transaction({
            to: address(0xCAFE),
            value: 1 ether,
            data: ""
        });

        // Direct call to executeBySender by an unauthorized EOA (e.g. address(0x123)) should revert
        vm.prank(address(0x123));
        vm.expectRevert("INSUFFICIENT_PRIVILEGE");
        AmbireAccount(expectedAddr).executeBySender(txns);

        // Direct call to executeBySelf must only be callable by the identity wallet itself
        vm.prank(address(0x123));
        vm.expectRevert("ONLY_IDENTITY_CAN_CALL");
        AmbireAccount(expectedAddr).executeBySelf(txns);
    }

    // 3. Invalid Signature Length / Format Rejection
    function testRejectInvalidSignatureLength() public {
        AmbireAccount.Transaction[] memory txns = new AmbireAccount.Transaction[](1);
        txns[0] = AmbireAccount.Transaction({
            to: address(0xCAFE),
            value: 1 ether,
            data: ""
        });

        uint256 nonce = AmbireAccount(expectedAddr).nonce();
        bytes32 hash = keccak256(abi.encode(expectedAddr, block.chainid, nonce, txns));
        (uint8 v, bytes32 r, bytes32 s) = vm.sign(ownerPk, hash);

        // A signature with length 0 should trigger arithmetic underflow/overflow panic
        bytes memory emptySig = "";
        vm.expectRevert(stdError.arithmeticError);
        AmbireAccount(expectedAddr).execute(txns, emptySig);

        // A signature with length 65 has its last byte (v) interpreted as the mode.
        // Since v is 27 or 28, it exceeds LastUnused (6), reverting with SV_SIGMODE.
        bytes memory shortSig = abi.encodePacked(r, s, v);
        vm.expectRevert("SV_SIGMODE");
        AmbireAccount(expectedAddr).execute(txns, shortSig);
    }

    // 4. Invalid Signature Mode Rejection
    function testRejectInvalidSignatureMode() public {
        AmbireAccount.Transaction[] memory txns = new AmbireAccount.Transaction[](1);
        txns[0] = AmbireAccount.Transaction({
            to: address(0xCAFE),
            value: 1 ether,
            data: ""
        });

        uint256 nonce = AmbireAccount(expectedAddr).nonce();
        bytes32 hash = keccak256(abi.encode(expectedAddr, block.chainid, nonce, txns));
        (uint8 v, bytes32 r, bytes32 s) = vm.sign(ownerPk, hash);

        // Mode 0x06 is out of bounds (SignatureMode.LastUnused is 0x06)
        bytes memory badModeSig = abi.encodePacked(r, s, v, uint8(0x06));
        vm.expectRevert("SV_SIGMODE");
        AmbireAccount(expectedAddr).execute(txns, badModeSig);
    }
}
