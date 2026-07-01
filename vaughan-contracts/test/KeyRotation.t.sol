// SPDX-License-Identifier: MIT
pragma solidity ^0.8.7;

import "forge-std/Test.sol";
import "../src/AmbireAccount.sol";
import "../src/AmbireAccountFactory.sol";

contract KeyRotationTest is Test {
    AmbireAccountFactory factory;
    address payable expectedAddr;

    uint256 ownerPk;
    address owner;

    uint256 backupPk;
    address backup;

    function setUp() public {
        ownerPk = 0xac0974bec39a17e36ba4a6b4d238ff944bacb478cbed5efcae784d7bf4f2ff80;
        owner = vm.addr(ownerPk);

        backupPk = 0x59c6995e998f97a5a0044966f0945389dc9e86dae88c7a8412f4603b6b78690d;
        backup = vm.addr(backupPk);

        factory = new AmbireAccountFactory(address(0));

        // Build SCW deployment initCode
        address[] memory addrs = new address[](1);
        addrs[0] = owner;
        bytes memory initCode = abi.encodePacked(
            type(AmbireAccount).creationCode,
            abi.encode(addrs)
        );

        // Derive counterfactual address
        uint256 salt = 777;
        expectedAddr = payable(address(uint160(uint256(
            keccak256(abi.encodePacked(
                bytes1(0xff), address(factory), salt, keccak256(initCode)
            ))
        ))));

        // Fund and deploy the wallet
        vm.deal(expectedAddr, 5 ether);
        factory.deploy(initCode, salt);
    }

    function testKeyRotationFlow() public {
        // Verify initial privilege state
        assertEq(AmbireAccount(expectedAddr).privileges(owner), bytes32(uint256(1)), "Owner should have privilege");
        assertEq(AmbireAccount(expectedAddr).privileges(backup), bytes32(uint256(0)), "Backup should not have privilege");

        // --- 1. Owner authorizes Backup ---
        // Prepare execution to setAddrPrivilege(backup, 1)
        AmbireAccount.Transaction[] memory txns = new AmbireAccount.Transaction[](1);
        txns[0] = AmbireAccount.Transaction({
            to: expectedAddr,
            value: 0,
            data: abi.encodeWithSignature("setAddrPrivilege(address,bytes32)", backup, bytes32(uint256(1)))
        });

        uint256 nonce = AmbireAccount(expectedAddr).nonce();
        bytes32 hashAdd = keccak256(abi.encode(expectedAddr, block.chainid, nonce, txns));
        (uint8 v, bytes32 r, bytes32 s) = vm.sign(ownerPk, hashAdd);
        bytes memory sigAdd = abi.encodePacked(r, s, v, uint8(0x00));

        // Execute the privilege modification
        AmbireAccount(expectedAddr).execute(txns, sigAdd);

        // Verify both are now privileged owners
        assertEq(AmbireAccount(expectedAddr).privileges(owner), bytes32(uint256(1)), "Owner should still have privilege");
        assertEq(AmbireAccount(expectedAddr).privileges(backup), bytes32(uint256(1)), "Backup should now have privilege");

        // --- 2. Verify Backup can execute transactions ---
        address recipient = address(0xCAFE);
        AmbireAccount.Transaction[] memory txnsBackup = new AmbireAccount.Transaction[](1);
        txnsBackup[0] = AmbireAccount.Transaction({
            to: recipient,
            value: 1 ether,
            data: ""
        });

        nonce = AmbireAccount(expectedAddr).nonce();
        bytes32 hashBackup = keccak256(abi.encode(expectedAddr, block.chainid, nonce, txnsBackup));
        (v, r, s) = vm.sign(backupPk, hashBackup);
        bytes memory sigBackup = abi.encodePacked(r, s, v, uint8(0x00));

        // Backup executes the transaction successfully
        AmbireAccount(expectedAddr).execute(txnsBackup, sigBackup);
        assertEq(recipient.balance, 1 ether, "Recipient should have received 1 ETH");

        // --- 3. Backup revokes original Owner's privilege ---
        AmbireAccount.Transaction[] memory txnsRevoke = new AmbireAccount.Transaction[](1);
        txnsRevoke[0] = AmbireAccount.Transaction({
            to: expectedAddr,
            value: 0,
            data: abi.encodeWithSignature("setAddrPrivilege(address,bytes32)", owner, bytes32(uint256(0)))
        });

        nonce = AmbireAccount(expectedAddr).nonce();
        bytes32 hashRevoke = keccak256(abi.encode(expectedAddr, block.chainid, nonce, txnsRevoke));
        (v, r, s) = vm.sign(backupPk, hashRevoke);
        bytes memory sigRevoke = abi.encodePacked(r, s, v, uint8(0x00));

        // Backup executes the revocation
        AmbireAccount(expectedAddr).execute(txnsRevoke, sigRevoke);

        // Verify original Owner is revoked, Backup remains owner
        assertEq(AmbireAccount(expectedAddr).privileges(owner), bytes32(uint256(0)), "Owner should be revoked");
        assertEq(AmbireAccount(expectedAddr).privileges(backup), bytes32(uint256(1)), "Backup should still have privilege");

        // --- 4. Verify original Owner can no longer execute transactions ---
        AmbireAccount.Transaction[] memory txnsFailed = new AmbireAccount.Transaction[](1);
        txnsFailed[0] = AmbireAccount.Transaction({
            to: recipient,
            value: 1 ether,
            data: ""
        });

        nonce = AmbireAccount(expectedAddr).nonce();
        bytes32 hashFailed = keccak256(abi.encode(expectedAddr, block.chainid, nonce, txnsFailed));
        (v, r, s) = vm.sign(ownerPk, hashFailed);
        bytes memory sigFailed = abi.encodePacked(r, s, v, uint8(0x00));

        // Attempting to execute with original owner's signature must revert
        vm.expectRevert("INSUFFICIENT_PRIVILEGE");
        AmbireAccount(expectedAddr).execute(txnsFailed, sigFailed);
    }
}
