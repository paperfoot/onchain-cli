use crate::errors::EvmError;

#[derive(Debug, Clone)]
pub struct ChainConfig {
    pub name: &'static str,
    pub chain_id: u64,
    pub public_rpc: &'static str,
    pub local_rpc: &'static str,
    pub explorer_url: &'static str,
    pub native_symbol: &'static str,
    pub native_decimals: u8,
}

impl ChainConfig {
    pub fn explorer_api_url(&self) -> String {
        format!("https://{}/api", self.explorer_url)
    }

    pub fn explorer_v2_url(&self) -> String {
        format!("https://{}/api/v2", self.explorer_url)
    }
}

pub const CHAINS: &[ChainConfig] = &[
    ChainConfig {
        name: "arbitrum",
        chain_id: 42161,
        public_rpc: "https://arb1.arbitrum.io/rpc",
        local_rpc: "http://127.0.0.1:18547",
        explorer_url: "arbitrum.blockscout.com",
        native_symbol: "ETH",
        native_decimals: 18,
    },
    ChainConfig {
        name: "ethereum",
        chain_id: 1,
        public_rpc: "https://ethereum-rpc.publicnode.com",
        local_rpc: "http://127.0.0.1:8545",
        explorer_url: "eth.blockscout.com",
        native_symbol: "ETH",
        native_decimals: 18,
    },
    ChainConfig {
        name: "base",
        chain_id: 8453,
        public_rpc: "https://mainnet.base.org",
        local_rpc: "http://127.0.0.1:8546",
        explorer_url: "base.blockscout.com",
        native_symbol: "ETH",
        native_decimals: 18,
    },
    ChainConfig {
        name: "optimism",
        chain_id: 10,
        public_rpc: "https://mainnet.optimism.io",
        local_rpc: "http://127.0.0.1:8548",
        explorer_url: "optimism.blockscout.com",
        native_symbol: "ETH",
        native_decimals: 18,
    },
    ChainConfig {
        name: "polygon",
        chain_id: 137,
        public_rpc: "https://polygon.drpc.org",
        local_rpc: "http://127.0.0.1:8549",
        explorer_url: "polygon.blockscout.com",
        native_symbol: "POL",
        native_decimals: 18,
    },
];

pub fn resolve_chain(network: &str) -> Result<&'static ChainConfig, EvmError> {
    // Try by name
    if let Some(chain) = CHAINS.iter().find(|c| c.name.eq_ignore_ascii_case(network)) {
        return Ok(chain);
    }
    // Try by chain ID
    if let Ok(id) = network.parse::<u64>() {
        if let Some(chain) = CHAINS.iter().find(|c| c.chain_id == id) {
            return Ok(chain);
        }
    }
    Err(EvmError::config(format!(
        "Unknown network: {network}. Supported: {}",
        CHAINS.iter().map(|c| c.name).collect::<Vec<_>>().join(", ")
    )))
}
