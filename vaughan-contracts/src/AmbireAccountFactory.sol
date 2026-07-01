// SPDX-License-Identifier: agpl-3.0
pragma solidity ^0.8.7;

import "./AmbireAccount.sol";
import "./libs/IERC20.sol";

contract AmbireAccountFactory {
	event LogDeployed(address addr, uint256 salt);

	address public immutable allowedToDrain;
	constructor(address allowed) {
		allowedToDrain = allowed;
	}

	function deploy(bytes calldata code, uint256 salt) external {
		deploySafe(code, salt);
	}

	function deployAndExecute(
		bytes calldata code, uint256 salt,
		AmbireAccount.Transaction[] calldata txns, bytes calldata signature
	) external {
		address payable addr = payable(deploySafe(code, salt));
		AmbireAccount(addr).execute(txns, signature);
	}

	function deployAndCall(bytes calldata code, uint256 salt, address callee, bytes calldata data) external {
		deploySafe(code, salt);
		require(data.length > 4, 'DATA_LEN');
		bytes4 method;
		assembly {
			method := and(calldataload(data.offset), 0xffffffff00000000000000000000000000000000000000000000000000000000)
		}
		require(
			method == 0x6171d1c9
			|| method == 0x534255ff
			|| method == 0x4b776c6d
			|| method == 0x63486689
		, 'INVALID_METHOD');

		assembly {
			let dataPtr := mload(0x40)
			calldatacopy(dataPtr, data.offset, data.length)
			let result := call(gas(), callee, 0, dataPtr, data.length, 0, 0)

			switch result case 0 {
				let size := returndatasize()
				let ptr := mload(0x40)
				returndatacopy(ptr, 0, size)
				revert(ptr, size)
			}
			default {}
		}
	}

	function withdraw(IERC20 token, address to, uint256 tokenAmount) external {
		require(msg.sender == allowedToDrain, 'ONLY_AUTHORIZED');
		token.transfer(to, tokenAmount);
	}

	function deploySafe(bytes memory code, uint256 salt) internal returns (address) {
		address expectedAddr = address(uint160(uint256(
			keccak256(abi.encodePacked(bytes1(0xff), address(this), salt, keccak256(code)))
		)));
		uint size;
		assembly { size := extcodesize(expectedAddr) }
		if (size == 0) {
			address addr;
			assembly { addr := create2(0, add(code, 0x20), mload(code), salt) }
			require(addr != address(0), 'FAILED_DEPLOYING');
			require(addr == expectedAddr, 'FAILED_MATCH');
			emit LogDeployed(addr, salt);
		}
		return expectedAddr;
	}
}
