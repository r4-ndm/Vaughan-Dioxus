// SPDX-License-Identifier: MIT
pragma solidity ^0.8.7;

import "forge-std/Test.sol";
import "../src/AmbireAccount.sol";
import "../src/AmbireAccountFactory.sol";

contract AccountTest is Test {
    AmbireAccountFactory factory;
    address owner;
    uint256 ownerPk;

    function setUp() public {
        ownerPk = 0xac0974bec39a17e36ba4a6b4d238ff944bacb478cbed5efcae784d7bf4f2ff80;
        owner = vm.addr(ownerPk);
        // Factory constructor requires allowedToDrain
        factory = new AmbireAccountFactory(address(0));
    }

    function testEIP1271Signature() public {
        // Build init code: creationCode + abi.encode(address[])
        address[] memory addrs = new address[](1);
        addrs[0] = owner;
        bytes memory initCode = abi.encodePacked(
            type(AmbireAccount).creationCode,
            abi.encode(addrs)
        );

        uint256 salt = 1;

        // deploy() returns void — compute address via CREATE2 formula
        address payable expectedAddr = payable(address(uint160(uint256(
            keccak256(abi.encodePacked(
                bytes1(0xff),
                address(factory),
                salt,
                keccak256(initCode)
            ))
        ))));
        factory.deploy(initCode, salt);

        // Verify account is deployed
        uint256 codeSize;
        assembly { codeSize := extcodesize(expectedAddr) }
        assertGt(codeSize, 0, "Account not deployed");

        // Verify owner has privilege
        bytes32 priv = AmbireAccount(expectedAddr).privileges(owner);
        assertEq(priv, bytes32(uint256(1)), "Owner should have privilege");

        // Sign a message and verify EIP-1271
        bytes32 messageHash = keccak256("test message");
        (uint8 v, bytes32 r, bytes32 s) = vm.sign(ownerPk, messageHash);
        // 66-byte signature: r(32) + s(32) + v(1) + mode(1)
        // mode 0x00 = SignatureMode.EIP712
        bytes memory signature = abi.encodePacked(r, s, v, uint8(0x00));

        bytes4 magic = AmbireAccount(expectedAddr).isValidSignature(
            messageHash, signature
        );
        assertEq(magic, bytes4(0x1626ba7e), "EIP-1271 validation failed");
    }

    function testExecuteWithSignature() public {
        address[] memory addrs = new address[](1);
        addrs[0] = owner;
        bytes memory initCode = abi.encodePacked(
            type(AmbireAccount).creationCode,
            abi.encode(addrs)
        );

        uint256 salt = 2;
        address payable expectedAddr = payable(address(uint160(uint256(
            keccak256(abi.encodePacked(
                bytes1(0xff), address(factory), salt, keccak256(initCode)
            ))
        ))));
        factory.deploy(initCode, salt);

        // Fund the account
        vm.deal(expectedAddr, 1 ether);

        // Build a batch: send 0.1 ETH to a recipient
        address recipient = address(0xBEEF);
        AmbireAccount.Transaction[] memory txns = new AmbireAccount.Transaction[](1);
        txns[0] = AmbireAccount.Transaction({
            to: recipient,
            value: 0.1 ether,
            data: ""
        });

        // Hash: keccak256(abi.encode(address(this), chainId, nonce, txns))
        uint256 currentNonce = AmbireAccount(expectedAddr).nonce();
        assertEq(currentNonce, 0, "Initial nonce should be 0");

        bytes32 hash = keccak256(abi.encode(
            expectedAddr, block.chainid, currentNonce, txns
        ));

        (uint8 v, bytes32 r, bytes32 s) = vm.sign(ownerPk, hash);
        bytes memory sig = abi.encodePacked(r, s, v, uint8(0x00));

        AmbireAccount(expectedAddr).execute(txns, sig);

        assertEq(recipient.balance, 0.1 ether, "Recipient should have 0.1 ETH");
        assertEq(AmbireAccount(expectedAddr).nonce(), 1, "Nonce should increment");
    }

    function testDeployAndExecute() public {
        address[] memory addrs = new address[](1);
        addrs[0] = owner;
        bytes memory initCode = abi.encodePacked(
            type(AmbireAccount).creationCode,
            abi.encode(addrs)
        );

        uint256 salt = 3;
        address payable expectedAddr = payable(address(uint160(uint256(
            keccak256(abi.encodePacked(
                bytes1(0xff), address(factory), salt, keccak256(initCode)
            ))
        ))));

        // Fund the counterfactual address BEFORE deployment
        vm.deal(expectedAddr, 1 ether);

        // Build batch
        address recipient = address(0xCAFE);
        AmbireAccount.Transaction[] memory txns = new AmbireAccount.Transaction[](1);
        txns[0] = AmbireAccount.Transaction({
            to: recipient,
            value: 0.5 ether,
            data: ""
        });

        // Nonce is 0 (not deployed yet)
        bytes32 hash = keccak256(abi.encode(
            expectedAddr, block.chainid, uint256(0), txns
        ));
        (uint8 v, bytes32 r, bytes32 s) = vm.sign(ownerPk, hash);
        bytes memory sig = abi.encodePacked(r, s, v, uint8(0x00));

        // deployAndExecute atomically deploys and executes
        factory.deployAndExecute(initCode, salt, txns, sig);

        assertEq(recipient.balance, 0.5 ether);
        assertEq(AmbireAccount(expectedAddr).nonce(), 1);
    }
}
