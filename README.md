<div align="center">

# onchain

**Query any EVM blockchain from your terminal. No browser, no wallet connect, no waiting.**

[![Star this repo](https://img.shields.io/github/stars/paperfoot/onchain-cli?style=for-the-badge&logo=github&label=%E2%AD%90%20Star%20this%20repo&color=yellow)](https://github.com/paperfoot/onchain-cli/stargazers)
[![Follow @longevityboris](https://img.shields.io/badge/Follow_%40longevityboris-000000?style=for-the-badge&logo=x&logoColor=white)](https://x.com/longevityboris)

[![Rust](https://img.shields.io/badge/Rust-000000?style=for-the-badge&logo=rust&logoColor=white)](https://www.rust-lang.org/)
[![License: MIT](https://img.shields.io/badge/License-MIT-blue?style=for-the-badge)](LICENSE)
[![EVM Compatible](https://img.shields.io/badge/EVM-Compatible-3C3C3D?style=for-the-badge&logo=ethereum&logoColor=white)](https://ethereum.org/)

---

A single binary that talks to EVM chains over RPC and Blockscout. Check balances, decode calldata, trace internal calls, pull transfer histories, read storage slots. Pipe everything to `jq`. Works on Arbitrum, Ethereum, Base, Optimism, and Polygon out of the box.

[Install](#install) | [Quick Start](#quick-start) | [Commands](#commands) | [Networks](#supported-networks) | [Contributing](CONTRIBUTING.md)

</div>

---

## Why This Exists

MCP servers for blockchain queries are slow. They spin up a runtime, negotiate a protocol handshake, and then make the same RPC call you could have made directly. For on-chain forensics and investigations where you need answers fast, that overhead adds up.

`onchain` skips all of it. It is a compiled Rust binary that goes straight to the RPC endpoint. It uses happy-eyeballs probing to pick the fastest node (local or public) and caches the winner. Typical queries return in under 200ms.

It was built for a specific workflow: investigating suspicious wallets, tracing fund flows, and decoding what smart contracts actually did. The forensic commands (`code`, `nonce`, `transfers`, `trace`) exist because those are the first things you check when you see a wallet doing something weird.

## Install

### From source (recommended)

```bash
git clone https://github.com/paperfoot/onchain-cli.git
cd onchain-cli/evmcli
cargo install --path .
```

### Self-update

Once installed, the binary can update itself:

```bash
onchain update
```

## Quick Start

```bash
# Check a wallet balance on Arbitrum (default network)
onchain balance 0xd8dA6BF26964aF9D7eEd9e03E53415D37aA96045

# Check an ERC20 token balance
onchain balance 0xd8dA6BF26964aF9D7eEd9e03E53415D37aA96045 --token 0xaf88d065e77c8cC2239327C5EDb3A432268e5831

# Get transaction details
onchain tx 0xYOUR_TX_HASH

# Check gas prices
onchain gas

# Read a contract's owner
onchain call 0xCONTRACT "owner()(address)"

# Query Ethereum instead of Arbitrum
onchain --network ethereum balance 0xd8dA6BF26964aF9D7eEd9e03E53415D37aA96045

# Output as JSON and pipe to jq
onchain balance 0xADDR --json | jq '.balance'
```

## How It Works

```
onchain balance 0xADDR
        |
        v
  +-----------+     +------------------+     +----------+
  | CLI Parse | --> | RPC Auto-detect  | --> | Execute  |
  | (clap)    |     | happy-eyeballs   |     | (alloy)  |
  +-----------+     | local vs public  |     +----------+
                    | 30s disk cache   |          |
                    +------------------+          v
                                            +----------+
                                            | Render   |
                                            | table or |
                                            | JSON     |
                                            +----------+
```

1. **Parse** -- Clap handles argument parsing with strong types.
2. **Detect** -- Happy-eyeballs probing races your local node (200ms timeout) against the public RPC (40ms delayed start). Winner gets cached to disk for 30 seconds.
3. **Execute** -- Alloy makes the RPC call. Blockscout API handles explorer queries (transfers, transaction lists, ABI lookups).
4. **Render** -- Output goes to a formatted table for humans, or JSON when piped or `--json` is passed.

## Commands

| Command | What it does |
|---------|-------------|
| `balance <addr>` | Native token or ERC20 balance (use `--token`) |
| `tx <hash>` | Transaction details (from, to, value, gas, input) |
| `receipt <hash>` | Transaction receipt with status, gas used, logs count |
| `block <id>` | Block details by number, hash, or `latest` |
| `gas` | Current gas prices (base fee, priority fee) |
| `call <addr> <sig>` | Read-only smart contract call (`eth_call`) |
| `decode <calldata>` | Decode calldata using cached or fetched ABI |
| `abi <addr>` | Fetch and cache a contract's ABI from Blockscout |
| `logs` | Event logs with filters (`--event transfer`, `--participant`, block range) |
| `transfers <addr>` | Token transfer history from Blockscout (ERC20/721/1155) |
| `txs <addr>` | Transaction list from Blockscout explorer |
| `storage <addr> <slot>` | Read raw storage slot value |
| `nonce <addr>` | Transaction count (nonce) for an address |
| `code <addr>` | Check if address is EOA or contract |
| `trace <hash>` | Trace internal calls (auto-fallback to archive node) |
| `bench` | Run RPC performance benchmark |
| `update` | Self-update to latest release |
| `examples` | Show investigation examples and forensic workflow |

All commands accept `--network`, `--rpc-url`, and `--json` flags.

## Forensic Workflow

Investigating a suspicious wallet follows a consistent pattern:

```bash
# 1. Is it an EOA or a contract?
onchain code 0xSUSPECT

# 2. Fresh wallet? Low nonce = likely created for this purpose
onchain nonce 0xSUSPECT

# 3. Where did the funds come from?
onchain transfers 0xSUSPECT

# 4. Full transaction history
onchain txs 0xSUSPECT

# 5. Details of the key transaction
onchain tx 0xSUSPICIOUS_TX_HASH

# 6. What happened internally?
onchain trace 0xSUSPICIOUS_TX_HASH

# 7. Repeat for each funding source (multi-hop tracing)
```

## Supported Networks

| Network | Chain ID | Default |
|---------|----------|---------|
| Arbitrum | 42161 | Yes |
| Ethereum | 1 | |
| Base | 8453 | |
| Optimism | 10 | |
| Polygon | 137 | |

Switch networks with `--network`:

```bash
onchain --network ethereum gas
onchain --network base balance 0xADDR
onchain --network 137 tx 0xHASH        # Chain ID also works
```

Use a custom RPC with `--rpc-url` or set the `ONCHAIN_RPC_URL` environment variable.

## Contributing

See [CONTRIBUTING.md](CONTRIBUTING.md) for guidelines.

## License

[MIT](LICENSE)

---

<div align="center">

Built by [Boris Djordjevic](https://github.com/longevityboris) at [199 Biotechnologies](https://github.com/199-biotechnologies) | [Paperfoot AI](https://paperfoot.ai)

[![Star this repo](https://img.shields.io/github/stars/paperfoot/onchain-cli?style=for-the-badge&logo=github&label=%E2%AD%90%20Star%20this%20repo&color=yellow)](https://github.com/paperfoot/onchain-cli/stargazers)
[![Follow @longevityboris](https://img.shields.io/badge/Follow_%40longevityboris-000000?style=for-the-badge&logo=x&logoColor=white)](https://x.com/longevityboris)

</div>
