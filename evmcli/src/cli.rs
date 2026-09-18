use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(name = "onchain", version, about = "Fast EVM and Zcash CLI toolkit", after_help = EXAMPLES)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Commands,

    /// Output as JSON (auto-detected when piped)
    #[arg(long, global = true)]
    pub json: bool,

    /// Network name or chain ID (default: arbitrum)
    #[arg(
        long,
        global = true,
        default_value = "arbitrum",
        env = "ONCHAIN_NETWORK"
    )]
    pub network: String,

    /// Custom RPC URL (overrides auto-detect)
    #[arg(long, global = true, env = "ONCHAIN_RPC_URL", hide_env_values = true)]
    pub rpc_url: Option<String>,

    /// Blockscout API base URL, including /api (overrides the network explorer)
    #[arg(
        long,
        global = true,
        env = "ONCHAIN_EXPLORER_URL",
        hide_env_values = true
    )]
    pub explorer_url: Option<String>,
}

#[derive(Subcommand)]
pub enum Commands {
    /// Native Zcash node queries, exact amounts, and batched reads
    Zcash(crate::zcash::ZcashArgs),
    /// Discover assets, preview cross-chain swaps, and check status via NEAR Intents 1Click
    Swap(crate::swap::SwapArgs),
    /// Get native token or ERC20 balance
    Balance {
        /// Address to check
        address: String,
        /// ERC20 token contract address (omit for native balance)
        #[arg(long)]
        token: Option<String>,
    },

    /// Get transaction details by hash
    Tx {
        /// Transaction hash
        hash: String,
    },

    /// Get transaction receipt
    Receipt {
        /// Transaction hash
        hash: String,
    },

    /// Get block details
    Block {
        /// Block number, hash, or "latest"
        #[arg(default_value = "latest")]
        id: String,
    },

    /// Get current gas prices
    Gas,

    /// Read a smart contract (eth_call)
    Call {
        /// Contract address
        address: String,
        /// Function signature, e.g. "owner()(address)"
        sig: String,
        /// Function arguments
        args: Vec<String>,
    },

    /// List transactions from Blockscout
    Txs {
        /// Address to list transactions for
        address: String,
        /// Maximum explorer pages to fetch (1-100)
        #[arg(long, default_value_t = 1, value_parser = clap::value_parser!(u32).range(1..=100))]
        pages: u32,
    },

    /// Decode calldata
    Decode {
        /// Calldata hex string (0x-prefixed)
        data: String,
        /// Explicit function signature for offline decoding and selector verification
        #[arg(long)]
        sig: Option<String>,
    },

    /// Fetch and cache contract ABI
    Abi {
        /// Contract address
        address: String,
    },

    /// Get event logs (Transfer, Swap, etc.)
    Logs {
        /// Contract address to filter logs from
        #[arg(long)]
        address: Option<String>,
        /// Event topic0 hash (e.g. Transfer topic)
        #[arg(long, conflicts_with = "event")]
        topic0: Option<String>,
        /// Filter by a specific address in topic1 or topic2
        #[arg(long)]
        participant: Option<String>,
        /// Start block (default: latest - 1000)
        #[arg(long)]
        from_block: Option<u64>,
        /// End block (default: latest)
        #[arg(long)]
        to_block: Option<u64>,
        /// Shorthand: --event transfer|approval|swap
        #[arg(long)]
        event: Option<String>,
    },

    /// Get token transfer history from Blockscout
    Transfers {
        /// Address to get transfers for
        address: String,
        /// Filter by token type: erc20, erc721, erc1155
        #[arg(long, default_value = "erc20", value_parser = ["erc20", "erc721", "erc1155"], ignore_case = true)]
        token_type: String,
        /// Maximum explorer pages to fetch (1-100)
        #[arg(long, default_value_t = 1, value_parser = clap::value_parser!(u32).range(1..=100))]
        pages: u32,
    },

    /// Read raw storage slot
    Storage {
        /// Contract address
        address: String,
        /// Storage slot (hex, e.g. 0x0)
        slot: String,
        /// Block number (default: latest)
        #[arg(long)]
        block: Option<u64>,
    },

    /// Get transaction count (nonce) for an address
    Nonce {
        /// Address
        address: String,
    },

    /// Inspect bytecode and EIP-7702 delegation
    Code {
        /// Address to check
        address: String,
    },

    /// Trace internal calls of a transaction (requires node tracing support)
    Trace {
        /// Transaction hash
        hash: String,
    },

    /// Run performance benchmark
    Bench {
        /// Number of iterations
        #[arg(long, default_value = "20", value_parser = clap::value_parser!(u32).range(1..))]
        iterations: u32,
        /// Warmup iterations
        #[arg(long, default_value = "3")]
        warmup: u32,
        /// Address to benchmark with
        #[arg(long, default_value = "0x4a0aCaC60321d89E8d4d01fA09318849Cb6a586A")]
        address: String,
    },

    /// Check for updates and self-update
    Update {
        /// Only check, don't install
        #[arg(long)]
        check: bool,
    },

    /// Show investigation examples and forensic workflow
    Examples,
}

impl Cli {
    pub fn validate(&self) -> Result<(), crate::errors::EvmError> {
        use crate::errors::{validate_address, EvmError};
        use alloy::primitives::{B256, U256};
        match &self.command {
            Commands::Balance { address, token } => {
                validate_address(address)?;
                if let Some(token) = token {
                    validate_address(token)?;
                }
            }
            Commands::Call { address, sig, args } => {
                validate_address(address)?;
                crate::commands::call::encode_call(sig, args)?;
            }
            Commands::Tx { hash } | Commands::Receipt { hash } | Commands::Trace { hash } => {
                hash.parse::<B256>()
                    .map_err(|_| EvmError::validation("Invalid transaction hash"))?;
            }
            Commands::Abi { address }
            | Commands::Txs { address, .. }
            | Commands::Transfers { address, .. }
            | Commands::Nonce { address }
            | Commands::Code { address }
            | Commands::Bench { address, .. } => validate_address(address)?,
            Commands::Storage { address, slot, .. } => {
                validate_address(address)?;
                slot.parse::<U256>()
                    .map_err(|_| EvmError::validation("Invalid storage slot"))?;
            }
            Commands::Block { id } => {
                crate::commands::block::parse_block_id(id)?;
            }
            Commands::Logs {
                address,
                participant,
                topic0,
                from_block,
                to_block,
                event,
            } => {
                if let Some(addr) = address {
                    validate_address(addr)?;
                }
                if let Some(addr) = participant {
                    validate_address(addr)?;
                }
                if let Some(topic) = topic0 {
                    topic
                        .parse::<B256>()
                        .map_err(|_| EvmError::validation("Invalid topic0"))?;
                }
                if let Some(event) = event {
                    crate::commands::logs::resolve_event_topic(event)?;
                }
                if let (Some(start), Some(end)) = (from_block, to_block) {
                    if start > end {
                        return Err(EvmError::validation(
                            "--from-block must not exceed --to-block",
                        ));
                    }
                }
            }
            _ => {}
        }
        Ok(())
    }
}

pub const EXAMPLES: &str = r#"
EXAMPLES:
  # Basic queries
  onchain balance 0xADDR                          # ETH balance
  onchain balance 0xADDR --token 0xUSDC           # ERC20 balance
  onchain tx 0xHASH                               # Transaction details
  onchain receipt 0xHASH                           # Receipt + logs count
  onchain gas                                      # Current gas prices
  onchain call 0xCONTRACT "owner()(address)"      # Read contract

  # Forensic investigation
  onchain code 0xADDR                              # Bytecode or delegation
  onchain nonce 0xADDR                             # How many TXs sent?
  onchain transfers 0xADDR                         # Token transfer history (funding trail)
  onchain txs 0xADDR                               # Transaction list from explorer
  onchain logs --event transfer --participant 0xADDR --from-block 19000000
  onchain trace 0xHASH                             # Internal calls (needs tracing support)

  # Decode + ABI
  onchain decode 0xCALLDATA                        # Decode function call
  onchain abi 0xCONTRACT                           # Fetch + cache contract ABI

  # Multi-chain
  onchain --network ethereum balance 0xADDR        # Query Ethereum
  onchain --network base balance 0xADDR            # Query Base
  onchain --rpc-url http://localhost:8547 balance 0xADDR  # Custom RPC

FORENSIC WORKFLOW (investigate a suspicious wallet):
  1. onchain code 0xSUSPECT              # Bytecode or delegation
  2. onchain nonce 0xSUSPECT             # Low nonce indicates few outgoing transactions
  3. onchain transfers 0xSUSPECT         # Where did funds come from? (Binance? Tornado?)
  4. onchain txs 0xSUSPECT              # Recent transaction page
  5. onchain tx 0xSUSPICIOUS_TX          # Details of the key transaction
  6. onchain receipt 0xSUSPICIOUS_TX     # Status + gas + logs count
  7. onchain trace 0xSUSPICIOUS_TX       # Internal calls (what contracts were hit?)
  8. For each funder address from step 3, repeat steps 1-4 (multi-hop tracing)
"#;
