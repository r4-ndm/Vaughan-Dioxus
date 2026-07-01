// SPDX-License-Identifier: MIT
pragma solidity ^0.8.7;

import "forge-std/Test.sol";
import "../src/AmbireAccount.sol";
import "../src/AmbireAccountFactory.sol";

contract MultisigTest is Test {
    AmbireAccountFactory factory;
    address payable expectedAddr;

    uint256 owner1Pk;
    address owner1;

    uint256 owner2Pk;
    address owner2;

    address multisigPrivKey;

    function setUp() public {
        owner1Pk = 0xac0974bec39a17e36ba4a6b4d238ff944bacb478cbed5efcae784d7bf4f2ff80;
        owner1 = vm.addr(owner1Pk);

        owner2Pk = 0x59c6995e998f97a5a0044966f0945389dc9e86dae88c7a8412f4603b6b78690d;
        owner2 = vm.addr(owner2Pk);

        factory = new AmbireAccountFactory(address(0));

        // Calculate combined multisig key address
        // Formula:
        // signer = address(0)
        // signer = keccak256(address(0), owner1)
        // signer = keccak256(signer, owner2)
        address signer = address(0);
        signer = address(uint160(uint256(keccak256(abi.encodePacked(signer, owner1)))));
        signer = address(uint160(uint256(keccak256(abi.encodePacked(signer, owner2)))));
        multisigPrivKey = signer;

        // Deploy SCW where the initial privileged key is the multisig privilege address
        address[] memory addrs = new address[](1);
        addrs[0] = multisigPrivKey;
        bytes memory initCode = abi.encodePacked(
            type(AmbireAccount).creationCode,
            abi.encode(addrs)
        );

        uint256 salt = 4444;
        expectedAddr = payable(address(uint160(uint256(
            keccak256(abi.encodePacked(
                bytes1(0xff), address(factory), salt, keccak256(initCode)
            ))
        ))));

        // Fund and deploy the wallet
        vm.deal(expectedAddr, 5 ether);
        factory.deploy(initCode, salt);
    }

    function testMultisigSignatureExecution() public {
        address recipient = address(0xCAFE);
        AmbireAccount.Transaction[] memory txns = new AmbireAccount.Transaction[](1);
        txns[0] = AmbireAccount.Transaction({
            to: recipient,
            value: 1 ether,
            data: ""
        });

        uint256 nonce = AmbireAccount(expectedAddr).nonce();
        bytes32 hash = keccak256(abi.encode(expectedAddr, block.chainid, nonce, txns));

        // Sign with owner 1
        (uint8 v1, bytes32 r1, bytes32 s1) = vm.sign(owner1Pk, hash);
        bytes memory sig1 = abi.encodePacked(r1, s1, v1, uint8(0x00)); // Mode 0x00 EIP-712

        // Sign with owner 2
        (uint8 v2, bytes32 r2, bytes32 s2) = vm.sign(owner2Pk, hash);
        bytes memory sig2 = abi.encodePacked(r2, s2, v2, uint8(0x00)); // Mode 0x00 EIP-712

        // Prepare the array of signatures
        bytes[] memory signatures = new bytes[](2);
        signatures[0] = sig1;
        signatures[1] = sig2;

        // Encode as: abi.encode(signatures) + Mode.Multisig (0x05)
        bytes memory multisigSignature = abi.encodePacked(
            abi.encode(signatures),
            uint8(0x05) // SignatureMode.Multisig
        );

        // Execute transaction using multisig signature
        AmbireAccount(expectedAddr).execute(txns, multisigSignature);

        // Verify state changes
        assertEq(recipient.balance, 1 ether, "Recipient should have received 1 ETH");
        assertEq(AmbireAccount(expectedAddr).nonce(), 1, "Nonce should increment to 1");
    }
}
