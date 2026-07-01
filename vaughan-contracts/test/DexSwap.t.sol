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

contract MockDEX {
    MockToken public token;
    uint256 public rate = 100; // 1 ETH = 100 MTK

    constructor(MockToken _token) payable {
        token = _token;
    }

    // Swap ETH to Token
    function swapExactETHForTokens() external payable {
        require(msg.value > 0, "DEX: msg.value must be > 0");
        uint256 tokenAmount = msg.value * rate;
        require(token.balanceOf(address(this)) >= tokenAmount, "DEX: insufficient token balance");
        token.transfer(msg.sender, tokenAmount);
    }

    // Swap Token to ETH
    function swapExactTokensForETH(uint256 tokenAmount) external {
        require(tokenAmount > 0, "DEX: tokenAmount must be > 0");
        uint256 ethAmount = tokenAmount / rate;
        require(address(this).balance >= ethAmount, "DEX: insufficient ETH balance");
        token.transferFrom(msg.sender, address(this), tokenAmount);
        payable(msg.sender).transfer(ethAmount);
    }

    receive() external payable {}
}

contract DexSwapTest is Test {
    AmbireAccountFactory factory;
    MockToken token;
    MockDEX dex;
    address payable expectedAddr;

    address owner;
    uint256 ownerPk;

    function setUp() public {
        ownerPk = 0xac0974bec39a17e36ba4a6b4d238ff944bacb478cbed5efcae784d7bf4f2ff80;
        owner = vm.addr(ownerPk);

        // Deploy Factory
        factory = new AmbireAccountFactory(address(0));

        // Deploy Token and DEX
        token = new MockToken();
        dex = new MockDEX{value: 10 ether}(token);

        // Distribute tokens to DEX
        token.transfer(address(dex), 100_000 * 10**18);

        // Build SCW deployment initCode
        address[] memory addrs = new address[](1);
        addrs[0] = owner;
        bytes memory initCode = abi.encodePacked(
            type(AmbireAccount).creationCode,
            abi.encode(addrs)
        );

        // Derivate CREATE2 counterfactual address
        uint256 salt = 999;
        expectedAddr = payable(address(uint160(uint256(
            keccak256(abi.encodePacked(
                bytes1(0xff), address(factory), salt, keccak256(initCode)
            ))
        ))));

        // Fund and Deploy the Smart Contract Wallet
        vm.deal(expectedAddr, 2 ether);
        factory.deploy(initCode, salt);
    }

    function testDexBuyAndSellAtomicBatch() public {
        // --- 1. Swapping ETH -> MTK (DEX Buy) ---
        // Let's buy tokens using 0.5 ETH from the Smart Account Wallet.
        AmbireAccount.Transaction[] memory buyTxns = new AmbireAccount.Transaction[](1);
        buyTxns[0] = AmbireAccount.Transaction({
            to: address(dex),
            value: 0.5 ether,
            data: abi.encodeWithSignature("swapExactETHForTokens()")
        });

        uint256 currentNonce = AmbireAccount(expectedAddr).nonce();
        bytes32 hashBuy = keccak256(abi.encode(
            expectedAddr, block.chainid, currentNonce, buyTxns
        ));

        (uint8 v, bytes32 r, bytes32 s) = vm.sign(ownerPk, hashBuy);
        bytes memory sigBuy = abi.encodePacked(r, s, v, uint8(0x00));

        // Execute swap
        AmbireAccount(expectedAddr).execute(buyTxns, sigBuy);

        // Verify balances
        uint256 expectedTokens = 0.5 ether * dex.rate();
        assertEq(token.balanceOf(expectedAddr), expectedTokens, "Should have received 50 tokens");
        assertEq(expectedAddr.balance, 1.5 ether, "Should have spent 0.5 ETH");

        // --- 2. Swapping MTK -> ETH using an ATOMIC BATCH (DEX Sell) ---
        // An EOA would require 2 separate transactions (approve first, then swap).
        // But our Smart Account Wallet can batch them atomically!
        AmbireAccount.Transaction[] memory sellTxns = new AmbireAccount.Transaction[](2);
        
        // Step 1: Approve DEX to transfer our Mock Tokens
        sellTxns[0] = AmbireAccount.Transaction({
            to: address(token),
            value: 0,
            data: abi.encodeWithSignature("approve(address,uint256)", address(dex), expectedTokens)
        });

        // Step 2: Call DEX to swap tokens back to ETH
        sellTxns[1] = AmbireAccount.Transaction({
            to: address(dex),
            value: 0,
            data: abi.encodeWithSignature("swapExactTokensForETH(uint256)", expectedTokens)
        });

        currentNonce = AmbireAccount(expectedAddr).nonce();
        bytes32 hashSell = keccak256(abi.encode(
            expectedAddr, block.chainid, currentNonce, sellTxns
        ));

        (v, r, s) = vm.sign(ownerPk, hashSell);
        bytes memory sigSell = abi.encodePacked(r, s, v, uint8(0x00));

        // Execute batch atomic swap back
        AmbireAccount(expectedAddr).execute(sellTxns, sigSell);

        // Verify balances after swap back
        assertEq(token.balanceOf(expectedAddr), 0, "All tokens should be sold");
        assertEq(expectedAddr.balance, 2.0 ether, "Should have received 0.5 ETH back");
    }
}
