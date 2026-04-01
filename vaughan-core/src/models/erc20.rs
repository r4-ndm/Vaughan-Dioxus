use alloy::sol;

sol! {
    #[sol(rpc)]
    contract IERC20 {
        function balanceOf(address account) external view returns (uint256);
        function symbol() external view returns (string memory);
        function name() external view returns (string memory);
        function decimals() external view returns (uint8);
        function transfer(address to, uint256 amount) external returns (bool);
    }
}
