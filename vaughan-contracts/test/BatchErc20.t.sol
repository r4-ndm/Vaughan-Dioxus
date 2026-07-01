// SPDX-License-Identifier: MIT
pragma solidity ^0.8.7;

import "forge-std/Test.sol";
import "../src/AmbireAccount.sol";
import "../src/AmbireAccountFactory.sol";
import "../src/libs/IERC20.sol";

contract MockToken is IERC20 {
    string public name = "Mock Token";
    string public symbol = "MTK";
    uint8 public decimals = 18;
    uint256 public override totalSupply;
    mapping(address => uint256) public override balanceOf;
    mapping(address => mapping(address => uint256)) public override allowance;

    constructor() {
        _mint(msg.sender, 1_000_000 * 10**18);
    }

    function _mint(address to, uint256 amount) internal {
        totalSupply += amount;
        balanceOf[to] += amount;
        emit Transfer(address(0), to, amount);
    }

    function transfer(address to, uint256 amount) external override returns (bool) {
        require(balanceOf[msg.sender] >= amount, "ERC20: transfer amount exceeds balance");
        balanceOf[msg.sender] -= amount;
        balanceOf[to] += amount;
        emit Transfer(msg.sender, to, amount);
        return true;
    }

    function approve(address spender, uint256 amount) external override returns (bool) {
        allowance[msg.sender][spender] = amount;
        emit Approval(msg.sender, spender, amount);
        return true;
    }

    function transferFrom(address from, address to, uint256 amount) external override returns (bool) {
        require(allowance[from][msg.sender] >= amount, "ERC20: transfer amount exceeds allowance");
        require(balanceOf[from] >= amount, "ERC20: transfer amount exceeds balance");
        allowance[from][msg.sender] -= amount;
        balanceOf[from] -= amount;
        balanceOf[to] += amount;
        emit Transfer(from, to, amount);
        return true;
    }
}

contract BatchErc20Test is Test {
    AmbireAccountFactory factory;
    MockToken token;
    address payable expectedAddr;

    address owner;
    uint256 ownerPk;

    function setUp() public {
        ownerPk = 0xac0974bec39a17e36ba4a6b4d238ff944bacb478cbed5efcae784d7bf4f2ff80;
        owner = vm.addr(ownerPk);

        factory = new AmbireAccountFactory(address(0));
        token = new MockToken();

        // Build SCW deployment initCode
        address[] memory addrs = new address[](1);
        addrs[0] = owner;
        bytes memory initCode = abi.encodePacked(
            type(AmbireAccount).creationCode,
            abi.encode(addrs)
        );

        // Derive CREATE2 address
        uint256 salt = 555;
        expectedAddr = payable(address(uint160(uint256(
            keccak256(abi.encodePacked(
                bytes1(0xff), address(factory), salt, keccak256(initCode)
            ))
        ))));

        // Fund address with gas
        vm.deal(expectedAddr, 2 ether);

        // Deploy SCW
        factory.deploy(initCode, salt);

        // Transfer 1000 tokens to the SCW
        token.transfer(expectedAddr, 1000 * 10**18);
    }

    function testBatchedErc20Transfers() public {
        address recipient1 = address(0x1111);
        address recipient2 = address(0x2222);
        address recipient3 = address(0x3333);

        AmbireAccount.Transaction[] memory txns = new AmbireAccount.Transaction[](3);

        // 1. Transfer 100 tokens to recipient1
        txns[0] = AmbireAccount.Transaction({
            to: address(token),
            value: 0,
            data: abi.encodeWithSignature("transfer(address,uint256)", recipient1, 100 * 10**18)
        });

        // 2. Transfer 200 tokens to recipient2
        txns[1] = AmbireAccount.Transaction({
            to: address(token),
            value: 0,
            data: abi.encodeWithSignature("transfer(address,uint256)", recipient2, 200 * 10**18)
        });

        // 3. Transfer 300 tokens to recipient3
        txns[2] = AmbireAccount.Transaction({
            to: address(token),
            value: 0,
            data: abi.encodeWithSignature("transfer(address,uint256)", recipient3, 300 * 10**18)
        });

        uint256 currentNonce = AmbireAccount(expectedAddr).nonce();
        bytes32 hash = keccak256(abi.encode(
            expectedAddr, block.chainid, currentNonce, txns
        ));

        (uint8 v, bytes32 r, bytes32 s) = vm.sign(ownerPk, hash);
        bytes memory sig = abi.encodePacked(r, s, v, uint8(0x00));

        // Execute batch transfers
        AmbireAccount(expectedAddr).execute(txns, sig);

        // Verify balances
        assertEq(token.balanceOf(recipient1), 100 * 10**18, "Recipient 1 should hold 100 tokens");
        assertEq(token.balanceOf(recipient2), 200 * 10**18, "Recipient 2 should hold 200 tokens");
        assertEq(token.balanceOf(recipient3), 300 * 10**18, "Recipient 3 should hold 300 tokens");
        assertEq(token.balanceOf(expectedAddr), 400 * 10**18, "SCW should retain 400 tokens");
    }
}
