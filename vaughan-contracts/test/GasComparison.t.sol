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

contract GasComparisonTest is Test {
    AmbireAccountFactory factory;
    MockToken token;
    address payable expectedAddr;

    uint256 ownerPk;
    address owner;

    address recipient1 = address(0x1111);
    address recipient2 = address(0x2222);
    address recipient3 = address(0x3333);

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
        uint256 salt = 9999;
        expectedAddr = payable(address(uint160(uint256(
            keccak256(abi.encodePacked(
                bytes1(0xff), address(factory), salt, keccak256(initCode)
            ))
        ))));

        // Fund address with gas
        vm.deal(expectedAddr, 5 ether);
        vm.deal(owner, 5 ether);

        // Deploy SCW
        factory.deploy(initCode, salt);

        // Transfer tokens to the SCW and the owner EOA
        token.transfer(expectedAddr, 1000 * 10**18);
        token.transfer(owner, 1000 * 10**18);
    }

    function testGasComparison() public {
        // --- 1. Measure EOA individual transactions ---
        uint256 startGas = gasleft();
        
        vm.prank(owner);
        token.transfer(recipient1, 100 * 10**18);
        uint256 gasTx1 = startGas - gasleft();

        startGas = gasleft();
        vm.prank(owner);
        token.transfer(recipient2, 200 * 10**18);
        uint256 gasTx2 = startGas - gasleft();

        startGas = gasleft();
        vm.prank(owner);
        token.transfer(recipient3, 300 * 10**18);
        uint256 gasTx3 = startGas - gasleft();

        // Total EOA execution gas (note: this excludes the 21,000 base transaction fee per transaction,
        // which adds another 3 * 21,000 = 63,000 gas on-chain!)
        uint256 totalEoaExecutionGas = gasTx1 + gasTx2 + gasTx3;
        uint256 totalEoaTotalGas = totalEoaExecutionGas + (21_000 * 3);

        // --- 2. Measure Smart Account Batched Transaction ---
        AmbireAccount.Transaction[] memory txns = new AmbireAccount.Transaction[](3);
        txns[0] = AmbireAccount.Transaction({
            to: address(token),
            value: 0,
            data: abi.encodeWithSignature("transfer(address,uint256)", recipient1, 100 * 10**18)
        });
        txns[1] = AmbireAccount.Transaction({
            to: address(token),
            value: 0,
            data: abi.encodeWithSignature("transfer(address,uint256)", recipient2, 200 * 10**18)
        });
        txns[2] = AmbireAccount.Transaction({
            to: address(token),
            value: 0,
            data: abi.encodeWithSignature("transfer(address,uint256)", recipient3, 300 * 10**18)
        });

        uint256 currentNonce = AmbireAccount(expectedAddr).nonce();
        bytes32 hash = keccak256(abi.encode(expectedAddr, block.chainid, currentNonce, txns));
        (uint8 v, bytes32 r, bytes32 s) = vm.sign(ownerPk, hash);
        bytes memory sig = abi.encodePacked(r, s, v, uint8(0x00));

        startGas = gasleft();
        AmbireAccount(expectedAddr).execute(txns, sig);
        uint256 scwExecutionGas = startGas - gasleft();
        
        // Total SCW gas on-chain (includes only 1 base transaction fee = 21,000 gas!)
        uint256 scwTotalGas = scwExecutionGas + 21_000;

        // Log results
        console.log("EOA Execution Gas (3 txs):", totalEoaExecutionGas);
        console.log("EOA Total Gas (inc. base fee):", totalEoaTotalGas);
        console.log("SCW Batched Execution Gas (1 tx):", scwExecutionGas);
        console.log("SCW Total Gas (inc. base fee):", scwTotalGas);

        // Verify that the SCW gas saving is positive
        assertLt(scwTotalGas, totalEoaTotalGas, "Smart account batching should save gas overall");
        
        uint256 gasSaved = totalEoaTotalGas - scwTotalGas;
        console.log("Total Gas Saved via SCW Batching:", gasSaved);
    }
}
