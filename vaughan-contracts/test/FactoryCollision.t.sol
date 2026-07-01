// SPDX-License-Identifier: MIT
pragma solidity ^0.8.7;

import "forge-std/Test.sol";
import "../src/AmbireAccount.sol";
import "../src/AmbireAccountFactory.sol";

contract FactoryCollisionTest is Test {
    AmbireAccountFactory factory;
    
    address owner;
    uint256 ownerPk;
    bytes initCode;
    uint256 salt = 123456;

    function setUp() public {
        ownerPk = 0xac0974bec39a17e36ba4a6b4d238ff944bacb478cbed5efcae784d7bf4f2ff80;
        owner = vm.addr(ownerPk);

        factory = new AmbireAccountFactory(address(0));

        // Build SCW deployment initCode
        address[] memory addrs = new address[](1);
        addrs[0] = owner;
        initCode = abi.encodePacked(
            type(AmbireAccount).creationCode,
            abi.encode(addrs)
        );
    }

    // 1. Test duplicate deploy() calls
    function testDuplicateDeployCalls() public {
        // Deploy first time
        factory.deploy(initCode, salt);

        // Derive expected address
        address expectedAddr = address(uint160(uint256(
            keccak256(abi.encodePacked(
                bytes1(0xff), address(factory), salt, keccak256(initCode)
            ))
        )));

        uint256 size;
        assembly { size := extcodesize(expectedAddr) }
        assertGt(size, 0, "Account should be deployed");

        // Deploy second time with exact same parameters
        // This should run successfully and NOT revert (since deploySafe bypasses create2 if extcodesize > 0)
        factory.deploy(initCode, salt);

        // Verify the contract size remains valid
        assembly { size := extcodesize(expectedAddr) }
        assertGt(size, 0, "Account should still exist");
    }

    // 2. Test deployAndExecute() on an already deployed wallet
    // (Bypasses deploy and runs execution on current nonce)
    function testDeployAndExecuteOnAlreadyDeployedWallet() public {
        // Derive expected address
        address payable expectedAddr = payable(address(uint160(uint256(
            keccak256(abi.encodePacked(
                bytes1(0xff), address(factory), salt, keccak256(initCode)
            ))
        ))));

        // Fund the wallet
        vm.deal(expectedAddr, 5 ether);

        // Deploy it beforehand using deploy()
        factory.deploy(initCode, salt);

        // Now compile execution transaction: send 1 ETH to recipient
        address recipient = address(0xCAFE);
        AmbireAccount.Transaction[] memory txns = new AmbireAccount.Transaction[](1);
        txns[0] = AmbireAccount.Transaction({
            to: recipient,
            value: 1 ether,
            data: ""
        });

        // The current nonce is 0 (since no execute was run yet)
        uint256 currentNonce = AmbireAccount(expectedAddr).nonce();
        assertEq(currentNonce, 0);

        bytes32 hash = keccak256(abi.encode(expectedAddr, block.chainid, currentNonce, txns));
        (uint8 v, bytes32 r, bytes32 s) = vm.sign(ownerPk, hash);
        bytes memory sig = abi.encodePacked(r, s, v, uint8(0x00));

        // Call deployAndExecute on factory
        // It will see that the contract is already deployed, bypass redeployment, and successfully execute txns!
        factory.deployAndExecute(initCode, salt, txns, sig);

        // Verify balances and nonce state
        assertEq(recipient.balance, 1 ether, "Recipient should receive 1 ETH");
        assertEq(AmbireAccount(expectedAddr).nonce(), 1, "Nonce should increment to 1");
    }
}
