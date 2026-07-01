// SPDX-License-Identifier: agpl-3.0
pragma solidity ^0.8.7;

import "./libs/SignatureValidatorV2.sol";

contract AmbireAccount {
	address private constant FALLBACK_HANDLER_SLOT = address(0x6969);

	// Variables
	mapping (address => bytes32) public privileges;
	uint public nonce;
	mapping (bytes32 => uint) public scheduledRecoveries;

	// Events
	event LogPrivilegeChanged(address indexed addr, bytes32 priv);
	event LogErr(address indexed to, uint value, bytes data, bytes returnData); // only used in tryCatch
	event LogScheduled(bytes32 indexed txnHash, bytes32 indexed recoveryHash, address indexed recoveryKey, uint nonce, uint time, Transaction[] txns);
	event LogCancelled(bytes32 indexed txnHash, bytes32 indexed recoveryHash, address indexed recoveryKey, uint time);
	event LogExecScheduled(bytes32 indexed txnHash, bytes32 indexed recoveryHash, uint time);

	// Transaction structure
	struct Transaction {
		address to;
		uint value;
		bytes data;
	}
	struct RecoveryInfo {
		address[] keys;
		uint timelock;
	}

	// Recovery mode constants
	uint8 private constant SIGMODE_RECOVER = 254;
	uint8 private constant SIGMODE_CANCEL = 255;

	constructor(address[] memory addrs) {
		uint len = addrs.length;
		for (uint i=0; i<len; i++) {
			privileges[addrs[i]] = bytes32(uint(1));
			emit LogPrivilegeChanged(addrs[i], bytes32(uint(1)));
		}
	}

	receive() external payable {}
	function onERC721Received(address, address, uint256, bytes memory) external pure returns (bytes4) { return this.onERC721Received.selector; }
	function onERC1155Received(address, address, uint256, uint256, bytes memory) external pure returns (bytes4) { return this.onERC1155Received.selector; }
	function onERC1155BatchReceived(address, address, uint256[] memory, uint256[] memory, bytes memory) external pure returns (bytes4) {  return this.onERC1155BatchReceived.selector;  }

	fallback() external payable {
		address fallbackHandler = address(uint160(uint(privileges[FALLBACK_HANDLER_SLOT])));
		if (fallbackHandler == address(0)) return;
		assembly {
			calldatacopy(0, 0, calldatasize())
			let result := delegatecall(gas(), fallbackHandler, 0, calldatasize(), 0, 0)
			let size := returndatasize()
			returndatacopy(0, 0, size)
			if eq(result, 0) { revert(0, size) }
			return(0, size)
		}
	}

	function setAddrPrivilege(address addr, bytes32 priv)
		external
	{
		require(msg.sender == address(this), 'ONLY_IDENTITY_CAN_CALL');
		if (uint(privileges[addr]) > 1) require(priv != bytes32(uint(1)), 'UNSETTING_SPECIAL_DATA');
		privileges[addr] = priv;
		emit LogPrivilegeChanged(addr, priv);
	}

	function tryCatch(address to, uint value, bytes calldata data)
		external
	{
		require(msg.sender == address(this), 'ONLY_IDENTITY_CAN_CALL');
		(bool success, bytes memory returnData) = to.call{value: value, gas: gasleft()}(data);
		if (!success) emit LogErr(to, value, data, returnData);
	}
	function tryCatchLimit(address to, uint value, bytes calldata data, uint gasLimit)
		external
	{
		require(msg.sender == address(this), 'ONLY_IDENTITY_CAN_CALL');
		(bool success, bytes memory returnData) = to.call{value: value, gas: gasLimit}(data);
		if (!success) emit LogErr(to, value, data, returnData);
	}

	function execute(Transaction[] calldata txns, bytes calldata signature)
		public
	{
		uint currentNonce = nonce;
		bytes32 hash = keccak256(abi.encode(address(this), block.chainid, currentNonce, txns));
		nonce = currentNonce + 1;

		address signerKey;
		uint8 sigMode = uint8(signature[signature.length - 1]);
		if (sigMode == SIGMODE_RECOVER || sigMode == SIGMODE_CANCEL) {
			(bytes memory sig,) = SignatureValidator.splitSignature(signature);
			(RecoveryInfo memory recoveryInfo, bytes memory recoverySignature, address recoverySigner, address postRecoverySigner) = abi.decode(sig, (RecoveryInfo, bytes, address, address));
			bool isCancellation = sigMode == SIGMODE_CANCEL;
			bytes32 recoveryInfoHash = keccak256(abi.encode(recoveryInfo));
			require(privileges[recoverySigner] == recoveryInfoHash, 'RECOVERY_NOT_AUTHORIZED');
			uint scheduled = scheduledRecoveries[hash];
			if (scheduled != 0 && !isCancellation) {
				signerKey = postRecoverySigner;
				require(block.timestamp > scheduled, 'RECOVERY_NOT_READY');
				delete scheduledRecoveries[hash];
				emit LogExecScheduled(hash, recoveryInfoHash, block.timestamp);
			} else {
				address recoveryKey = SignatureValidator.recoverAddr(hash, recoverySignature);
				bool isIn;
				for (uint i=0; i<recoveryInfo.keys.length; i++) {
					if (recoveryInfo.keys[i] == recoveryKey) { isIn = true; break; }
				}
				require(isIn, 'RECOVERY_NOT_AUTHORIZED');
				if (isCancellation) {
					delete scheduledRecoveries[hash];
					emit LogCancelled(hash, recoveryInfoHash, recoveryKey, block.timestamp);
				} else {
					scheduledRecoveries[hash] = block.timestamp + recoveryInfo.timelock;
					emit LogScheduled(hash, recoveryInfoHash, recoveryKey, currentNonce, block.timestamp, txns);
				}
				return;
			}
		} else {
			signerKey = SignatureValidator.recoverAddrImpl(hash, signature, true);
			require(privileges[signerKey] != bytes32(0), 'INSUFFICIENT_PRIVILEGE');
		}

		executeBatch(txns);
		require(privileges[signerKey] != bytes32(0), 'PRIVILEGE_NOT_DOWNGRADED');
	}

	struct ExecuteArgs { Transaction[] txns; bytes signature; }
	function executeMultiple(ExecuteArgs[] calldata toExec) external {
		for (uint i = 0; i != toExec.length; i++) execute(toExec[i].txns, toExec[i].signature);
	}

	function executeBySender(Transaction[] calldata txns) external {
		require(privileges[msg.sender] != bytes32(0), 'INSUFFICIENT_PRIVILEGE');
		executeBatch(txns);
		require(privileges[msg.sender] != bytes32(0), 'PRIVILEGE_NOT_DOWNGRADED');
	}

	function executeBySelf(Transaction[] calldata txns) external {
		require(msg.sender == address(this), 'ONLY_IDENTITY_CAN_CALL');
		executeBatch(txns);
	}

	function executeBatch(Transaction[] memory txns) internal {
		require(txns.length > 0, 'MUST_PASS_TX');
		uint len = txns.length;
		for (uint i=0; i<len; i++) {
			Transaction memory txn = txns[i];
			executeCall(txn.to, txn.value, txn.data);
		}
	}

	function executeCall(address to, uint256 value, bytes memory data)
		internal
	{
		assembly {
			let result := call(gas(), to, value, add(data, 0x20), mload(data), 0, 0)
			switch result case 0 {
				let size := returndatasize()
				let ptr := mload(0x40)
				returndatacopy(ptr, 0, size)
				revert(ptr, size)
			}
			default {}
		}
	}

	function isValidSignature(bytes32 hash, bytes calldata signature) external view returns (bytes4) {
		if (privileges[SignatureValidator.recoverAddr(hash, signature)] != bytes32(0)) {
			return 0x1626ba7e;
		} else {
			return 0xffffffff;
		}
	}

	function supportsInterface(bytes4 interfaceID) external pure returns (bool) {
		return
			interfaceID == 0x01ffc9a7 ||
			interfaceID == 0x4e2312e0;
	}
}
