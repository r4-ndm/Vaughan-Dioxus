// SPDX-License-Identifier: MIT
pragma solidity ^0.8.7;

import "forge-std/Test.sol";
import "../src/AmbireAccount.sol";
import "../src/AmbireAccountFactory.sol";

interface IERC721Receiver {
    function onERC721Received(address operator, address from, uint256 tokenId, bytes calldata data) external returns (bytes4);
}

contract MockNFT {
    mapping(uint256 => address) public ownerOf;

    constructor() {}

    function mint(address to, uint256 tokenId) external {
        ownerOf[tokenId] = to;
    }

    function safeTransferFrom(address from, address to, uint256 tokenId) external {
        require(ownerOf[tokenId] == from, "NOT_OWNER");
        ownerOf[tokenId] = to;
        
        // Check if recipient is a contract and calls onERC721Received
        uint256 size;
        assembly { size := extcodesize(to) }
        if (size > 0) {
            try IERC721Receiver(to).onERC721Received(msg.sender, from, tokenId, "") returns (bytes4 retval) {
                require(retval == IERC721Receiver.onERC721Received.selector, "BAD_RETVAL");
            } catch {
                revert("REVERT_ON_TRANSFER");
            }
        }
    }
}

contract NftReceiverTest is Test {
    AmbireAccountFactory factory;
    address payable expectedAddr;
    MockNFT nft;

    address owner;
    uint256 ownerPk;

    function setUp() public {
        ownerPk = 0xac0974bec39a17e36ba4a6b4d238ff944bacb478cbed5efcae784d7bf4f2ff80;
        owner = vm.addr(ownerPk);

        factory = new AmbireAccountFactory(address(0));
        nft = new MockNFT();

        // Build SCW deployment initCode
        address[] memory addrs = new address[](1);
        addrs[0] = owner;
        bytes memory initCode = abi.encodePacked(
            type(AmbireAccount).creationCode,
            abi.encode(addrs)
        );

        // Derive counterfactual address
        uint256 salt = 33333;
        expectedAddr = payable(address(uint160(uint256(
            keccak256(abi.encodePacked(
                bytes1(0xff), address(factory), salt, keccak256(initCode)
            ))
        ))));

        // Fund and deploy the wallet
        vm.deal(expectedAddr, 2 ether);
        factory.deploy(initCode, salt);
    }

    function testNftSafeTransferReceiving() public {
        // Mint NFT to owner EOA
        uint256 tokenId = 999;
        nft.mint(owner, tokenId);
        assertEq(nft.ownerOf(tokenId), owner, "Owner EOA should own the NFT initially");

        // Owner EOA safely transfers NFT to the Smart Account Wallet
        vm.prank(owner);
        nft.safeTransferFrom(owner, expectedAddr, tokenId);

        // Verify that the transfer succeeded (which means the onERC721Received hook was executed and returned the correct selector)
        assertEq(nft.ownerOf(tokenId), expectedAddr, "Smart account wallet should successfully receive the NFT");
    }
}
