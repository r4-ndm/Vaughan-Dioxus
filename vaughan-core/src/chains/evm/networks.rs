//! EVM network configurations (Task 3.3).
//!
//! Canonical network list for EVM chains we support out of the box.

use serde::{Deserialize, Serialize};

/// EVM network configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EvmNetworkConfig {
    /// Stable identifier (e.g. "ethereum", "pulsechain")
    pub id: String,
    /// Human-readable name
    pub name: String,
    /// Chain ID
    pub chain_id: u64,
    /// Default RPC URL
    pub rpc_url: String,
    /// Explorer base URL (optional)
    pub explorer_url: Option<String>,
    /// Explorer API base URL (optional; Etherscan-compatible)
    pub explorer_api_url: Option<String>,
    /// Native token symbol (e.g. ETH, PLS)
    pub native_symbol: String,
    /// Native token name (e.g. Ethereum, PulseChain)
    pub native_name: String,
    /// Native token decimals (usually 18 for EVM)
    pub decimals: u8,
}

impl EvmNetworkConfig {
    pub fn new(
        id: impl Into<String>,
        name: impl Into<String>,
        chain_id: u64,
        rpc_url: impl Into<String>,
        native_symbol: impl Into<String>,
        native_name: impl Into<String>,
    ) -> Self {
        Self {
            id: id.into(),
            name: name.into(),
            chain_id,
            rpc_url: rpc_url.into(),
            explorer_url: None,
            explorer_api_url: None,
            native_symbol: native_symbol.into(),
            native_name: native_name.into(),
            decimals: 18,
        }
    }

    pub fn with_explorer(mut self, explorer_url: impl Into<String>) -> Self {
        self.explorer_url = Some(explorer_url.into());
        self
    }

    pub fn with_explorer_api(mut self, explorer_api_url: impl Into<String>) -> Self {
        self.explorer_api_url = Some(explorer_api_url.into());
        self
    }
}

// --- Required networks from tasks.md (3.3) ---

pub fn ethereum_mainnet() -> EvmNetworkConfig {
    EvmNetworkConfig::new(
        "ethereum",
        "Ethereum Mainnet",
        1,
        "https://eth.llamarpc.com",
        "ETH",
        "Ethereum",
    )
    .with_explorer("https://etherscan.io")
    .with_explorer_api("https://api.etherscan.io/api")
}

pub fn pulsechain_mainnet() -> EvmNetworkConfig {
    EvmNetworkConfig::new(
        "pulsechain",
        "PulseChain Mainnet",
        369,
        "https://rpc.pulsechain.com",
        "PLS",
        "PulseChain",
    )
    .with_explorer("https://scan.pulsechain.com")
    .with_explorer_api("https://api.scan.pulsechain.com/api")
}

pub fn polygon_mainnet() -> EvmNetworkConfig {
    EvmNetworkConfig::new(
        "polygon",
        "Polygon Mainnet",
        137,
        "https://polygon-bor-rpc.publicnode.com",
        "MATIC",
        "Polygon",
    )
    .with_explorer("https://polygonscan.com")
    .with_explorer_api("https://api.polygonscan.com/api")
}

pub fn arbitrum_one() -> EvmNetworkConfig {
    EvmNetworkConfig::new(
        "arbitrum",
        "Arbitrum One",
        42161,
        "https://arb1.arbitrum.io/rpc",
        "ETH",
        "Ethereum",
    )
    .with_explorer("https://arbiscan.io")
    .with_explorer_api("https://api.arbiscan.io/api")
}

pub fn optimism_mainnet() -> EvmNetworkConfig {
    EvmNetworkConfig::new(
        "optimism",
        "Optimism Mainnet",
        10,
        "https://mainnet.optimism.io",
        "ETH",
        "Ethereum",
    )
    .with_explorer("https://optimistic.etherscan.io")
    .with_explorer_api("https://api-optimistic.etherscan.io/api")
}

/// Ethereum Sepolia (matches Vaughan-Tauri `ethereum_sepolia`).
pub fn ethereum_sepolia() -> EvmNetworkConfig {
    EvmNetworkConfig::new(
        "sepolia",
        "Ethereum Sepolia",
        11_155_111,
        "https://ethereum-sepolia-rpc.publicnode.com",
        "ETH",
        "Sepolia Ether",
    )
    .with_explorer("https://sepolia.etherscan.io")
    .with_explorer_api("https://api-sepolia.etherscan.io/api")
}

/// PulseChain Testnet V4 (matches Vaughan-Tauri `pulsechain_testnet_v4`).
pub fn pulsechain_testnet_v4() -> EvmNetworkConfig {
    EvmNetworkConfig::new(
        "pulsechain-testnet-v4",
        "PulseChain Testnet V4",
        943,
        "https://rpc.v4.testnet.pulsechain.com",
        "tPLS",
        "Test PulseChain",
    )
    .with_explorer("https://scan.v4.testnet.pulsechain.com")
    .with_explorer_api("https://api.scan.v4.testnet.pulsechain.com/api")
}

/// Polygon Amoy (matches Vaughan-Tauri `polygon_amoy`).
pub fn polygon_amoy() -> EvmNetworkConfig {
    EvmNetworkConfig::new(
        "polygon-amoy",
        "Polygon Amoy",
        80_002,
        "https://polygon-amoy-bor-rpc.publicnode.com",
        "MATIC",
        "Amoy Matic",
    )
    .with_explorer("https://amoy.polygonscan.com")
    .with_explorer_api("https://api-amoy.polygonscan.com/api")
}

/// BSC Mainnet (matches Vaughan-Tauri `bsc_mainnet`).
pub fn bsc_mainnet() -> EvmNetworkConfig {
    EvmNetworkConfig::new(
        "bsc",
        "BSC Mainnet",
        56,
        "https://bsc-dataseed.binance.org",
        "BNB",
        "Binance Coin",
    )
    .with_explorer("https://bscscan.com")
}

/// Avalanche C-Chain (matches Vaughan-Tauri `avalanche_cchain`).
pub fn avalanche_cchain() -> EvmNetworkConfig {
    EvmNetworkConfig::new(
        "avalanche",
        "Avalanche C-Chain",
        43_114,
        "https://api.avax.network/ext/bc/C/rpc",
        "AVAX",
        "Avalanche",
    )
    .with_explorer("https://snowtrace.io")
}

/// Base Mainnet (matches Vaughan-Tauri `base_mainnet`).
pub fn base_mainnet() -> EvmNetworkConfig {
    EvmNetworkConfig::new(
        "base",
        "Base Mainnet",
        8453,
        "https://mainnet.base.org",
        "ETH",
        "Ethereum",
    )
    .with_explorer("https://basescan.org")
}

/// Default built-in EVM networks (same set and order as Vaughan-Tauri `all_networks()`).
pub fn builtin_networks() -> Vec<EvmNetworkConfig> {
    vec![
        ethereum_mainnet(),
        ethereum_sepolia(),
        pulsechain_mainnet(),
        pulsechain_testnet_v4(),
        polygon_mainnet(),
        polygon_amoy(),
        bsc_mainnet(),
        arbitrum_one(),
        optimism_mainnet(),
        avalanche_cchain(),
        base_mainnet(),
    ]
}

/// Find a built-in network by chain id.
pub fn get_network_by_chain_id(chain_id: u64) -> Option<EvmNetworkConfig> {
    builtin_networks()
        .into_iter()
        .find(|n| n.chain_id == chain_id)
}

/// Find a built-in network by id (case-insensitive).
pub fn get_network_by_id(id: &str) -> Option<EvmNetworkConfig> {
    let needle = id.trim().to_ascii_lowercase();
    builtin_networks()
        .into_iter()
        .find(|n| n.id.to_ascii_lowercase() == needle)
}
