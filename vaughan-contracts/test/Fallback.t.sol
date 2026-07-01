// SPDX-License-Identifier: MIT
pragma solidity ^0.8.7;

import "forge-std/Test.sol";
import "../src/AmbireAccount.sol";
import "../src/AmbireAccountFactory.sol";

contract FallbackTest is Test {
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
        uint256 salt = 12345;
        expectedAddr = payable(address(uint160(uint256(
            keccak256(abi.encodePacked(
                bytes1(0xff), address(factory), salt, keccak256(initCode)
            ))
        ))));
    }

    // 1. Counterfactual Funding (sending ETH to counterfactual address before deployment)
    function testCounterfactualFunding() public {
        // SCW is NOT yet deployed
        address addr = expectedAddr;
        uint256 size;
        assembly { size := extcodesize(addr) }
        assertEq(size, 0, "Wallet should not be deployed yet");

        // Fund the counterfactual address
        vm.deal(expectedAddr, 3 ether);
        assertEq(expectedAddr.balance, 3 ether, "Counterfactual address should hold 3 ETH");

        // Build SCW deployment initCode
        address[] memory addrs = new address[](1);
        addrs[0] = owner;
        bytes memory initCode = abi.encodePacked(
            type(AmbireAccount).creationCode,
            abi.encode(addrs)
        );

        // Deploy the contract
        factory.deploy(initCode, 12345);

        // Verify it is deployed and still holds the 3 ETH
        assembly { size := extcodesize(addr) }
        assertGt(size, 0, "Wallet should be deployed now");
        assertEq(expectedAddr.balance, 3 ether, "Deployed wallet should retain the 3 ETH");
    }

    // 2. Fallback and Native Asset Receivers (sending ETH after deployment)
    function testNativeReceive() public {
        // Deploy the contract first
        address[] memory addrs = new address[](1);
        addrs[0] = owner;
        bytes memory initCode = abi.encodePacked(
            type(AmbireAccount).creationCode,
            abi.encode(addrs)
        );
        factory.deploy(initCode, 12345);

        // Verify balance is 0
        assertEq(expectedAddr.balance, 0, "Initial balance should be 0");

        // Send 1 ETH using transfer/send (msg.data length == 0)
        (bool success, ) = expectedAddr.call{value: 1 ether}("");
        assertTrue(success, "Sending ETH should succeed");
        assertEq(expectedAddr.balance, 1 ether, "Balance should be 1 ETH");

        // Send 1 ETH with message data (which triggers fallback)
        (success, ) = expectedAddr.call{value: 1 ether}("0x12345678");
        // Note: The fallback handler is not set, so fallback does not forward, but fallback itself is payable
        assertTrue(success, "Fallback call with ETH should succeed");
        assertEq(expectedAddr.balance, 2 ether, "Balance should be 2 ETH");
    }
}
