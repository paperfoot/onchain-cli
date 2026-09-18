# onchain

A Rust CLI for EVM and native Zcash queries, transaction investigation, and cross-chain swap previews. JSON when piped, tables in a terminal. No background updater or wallet connection during ordinary queries.

## Install

Download a macOS or Linux binary from [Releases](https://github.com/paperfoot/onchain-cli/releases), verify its archive against `SHA256SUMS`, and put `onchain` on your PATH. Releases include Apple Silicon, Intel Mac, Linux x86-64, and Linux ARM64 builds. Linux binaries target Ubuntu 24.04 or a compatible glibc runtime; build from source on older distributions.

From source, with Rust 1.94.1 or newer:

```sh
git clone https://github.com/paperfoot/onchain-cli.git
cd onchain-cli
cargo install --locked --path evmcli
onchain --version
```

```sh
onchain update --check
onchain update
```

Self-update uses this repository's releases, verifies the downloaded archive checksum, and never downgrades to an older version.

Agent usage guidance is maintained in [skills/onchain/SKILL.md](skills/onchain/SKILL.md). To install it for Codex, copy that file to `~/.codex/skills/onchain/SKILL.md`.

## Zcash

Zcash has its own native RPC commands. `--network` selects EVM networks; `--zcash-network` selects `mainnet`, `testnet`, or `regtest`.

```sh
export ONCHAIN_ZCASH_RPC_URL=http://127.0.0.1:8232
# For a Zebra node with cookie authentication:
export ONCHAIN_ZCASH_COOKIE_FILE=/path/to/zebra/.cookie

onchain zcash info
onchain zcash health
onchain zcash block latest
onchain zcash balance t3dvVE3SQEi7kqNzwrfNePxZ1d4hUyztBA1
onchain zcash utxos t3dvVE3SQEi7kqNzwrfNePxZ1d4hUyztBA1
onchain zcash tx TRANSACTION_ID --block-hash BLOCK_HASH
onchain zcash mempool
onchain zcash bench --iterations 10
```

The example address above is the Zcash Foundation funding-stream address from the [Zebra documentation](https://zebra.zfnd.org/user/mining.html). Use your own address for your balances.

The default is a local node, port 8232 on mainnet or 18232 on testnet/regtest. Set `--zcash-rpc-url` or `ONCHAIN_ZCASH_RPC_URL` for another node; the global `--rpc-url`/`ONCHAIN_RPC_URL` is a fallback override. For authenticated nodes, use a cookie file or both `ONCHAIN_ZCASH_RPC_USER` and `ONCHAIN_ZCASH_RPC_PASSWORD`. Credentials require HTTPS unless the node is on loopback. Endpoint metadata and transport errors omit URL credentials, paths, and queries.

Every online Zcash command checks the returned chain. Multiple reads and the network check share a single HTTP batch and a reused connection. Results are never cached. `--timeout-ms` bounds each complete HTTP request, including its body; the default is 10 seconds. `health` exits unsuccessfully if the node is unsynced, more than two blocks behind its reported target, or its tip is over ten minutes old. These thresholds are configurable.

`balance` and `utxos` cover transparent addresses. Shielded balances require access to a wallet's notes or viewing keys; a public address alone is insufficient. The CLI currently provides node reads and swap previews. Wallet signing, shielded sends, and swap funding are not implemented. No private keys are accepted or exported. This release is a foundation for wallet/venue integration, not an automated trading bot.

### Exact amounts and fees

```sh
onchain zcash amount 1.23456789
# result.zatoshis = "123456789"
onchain zcash fee --actions 2
# result.fee_zatoshis = "10000"
```

ZEC conversions use integer arithmetic, reject sub-zatoshi precision, and enforce the monetary range. `fee` computes the [ZIP-317 conventional fee](https://zips.z.cash/zip-0317) for a supplied logical-action count; the wallet must determine that count and construct the transaction. It is not a live inclusion estimate.

### Batched reads

```json
[
  {"method":"getblockcount","params":[]},
  {"method":"getmempoolinfo","params":[]}
]
```

Save as `reads.json`, then run `onchain zcash batch reads.json`. Batches accept 1–100 allowlisted node reads, preserve request order even if responses arrive out of order, and fail on any RPC error, missing result, duplicate response ID, or network mismatch. Signing, broadcast, wallet exports, and administrative methods are rejected before connecting.

## Swap previews

The [NEAR Intents 1Click API](https://docs.near-intents.org/integration/distribution-channels/1click-api/about-1click-api) supplies supported assets, exact-input quote previews, and status for existing swaps.

```sh
onchain swap tokens --chain zec
onchain swap tokens --symbol USDC
# Take the exact destination assetId from the returned token list.
onchain swap quote \
  --from nep141:zec.omft.near \
  --to DESTINATION_ASSET_ID \
  --amount 100000000 \
  --recipient YOUR_DESTINATION_ADDRESS \
  --refund-to YOUR_ZCASH_ADDRESS \
  --slippage-bps 100
onchain swap status DEPOSIT_ADDRESS
```

`--amount` is always an integer in the origin asset's smallest units: `100000000` is 1 ZEC. Native ZEC is distinguished by `blockchain: "zec"`; symbols alone can also match bridged representations. All quotes set `dry: true`: they do not create deposit addresses or transfer funds. Responses retain the provider's complete quote, minimum output, fees, timestamp, and request echo. The CLI verifies the echoed assets, addresses, amount, deadline, and slippage before displaying a quote. A quote preview is not a reserved executable price.

Set `ONCHAIN_SWAP_API_KEY` (X-API-Key) or `ONCHAIN_SWAP_JWT` (Bearer) for authenticated API access. The service may allow anonymous requests subject to its current policy. `--timeout-ms` defaults to 15 seconds. HTTP authentication/rate-limit errors are explicit; quotes are never silently retried or served from a cache. Use `--deposit-memo` when checking a memo-based deposit.

## EVM

```sh
onchain --network ethereum balance 0xd8dA6BF26964aF9D7eEd9e03E53415D37aA96045
onchain --network ethereum call 0xC02aaA39b223FE8D0A0e5C4F27eAD9083C756Cc2 'name()(string)'
onchain --network base gas
onchain --network arbitrum block latest
onchain --network polygon gas --json
```

| Command | Purpose |
|---|---|
| `balance ADDRESS [--token CONTRACT]` | Native or ERC-20 balance; exact raw units and formatted amount |
| `tx HASH`, `receipt HASH` | Transaction details and receipt |
| `block [ID]` | Latest, block number/hash, or supported block tag |
| `gas` | Gas price, base fee, optional priority fee; exact wei plus gwei display |
| `call ADDRESS SIGNATURE [ARGS...]` | Read-only contract call with validated ABI; tuples and dynamic return values |
| `decode CALLDATA [--sig SIGNATURE]` | Built-in selectors decoded offline; explicit signature validation; remote candidates for unknown selectors |
| `abi ADDRESS` | Verified Blockscout ABI, cached for 24 hours |
| `txs ADDRESS [--pages N]` | Recent transaction pages, with continuation cursor |
| `transfers ADDRESS [--token-type erc20\|erc721\|erc1155] [--pages N]` | Typed transfer history, exact quantities and NFT IDs |
| `logs` | Topic/event/address/block filters; participant matches topic1 OR topic2 |
| `storage ADDRESS SLOT [--block NUMBER]` | Historical or latest 32-byte storage word, hex and decimal |
| `nonce ADDRESS`, `code ADDRESS` | Nonce, bytecode, and EIP-7702 delegation detection |
| `trace HASH` | Call-tracer output from a node with `debug_traceTransaction` support |
| `bench [--iterations N]` | Measured RPC/explorer latency, success and failure counts |
| `examples` | Investigation command examples |

| Network | Chain ID |
|---|---|
| Arbitrum (default) | 42161 |
| Ethereum | 1 |
| Base | 8453 |
| Optimism | 10 |
| Polygon | 137 |

Use `--network` or `ONCHAIN_NETWORK`, and `--rpc-url` or `ONCHAIN_RPC_URL` for a custom node. Explicit RPCs are checked against the selected chain. Otherwise local and public endpoints race with bounded probes; a cached endpoint is revalidated before use. The probe and query reuse their HTTP connection. Explorer-only commands skip RPC discovery entirely. `--explorer-url`/`ONCHAIN_EXPLORER_URL` overrides the Blockscout base URL, including `/api`.

`txs` and `transfers` default to one explorer page. `--pages` accepts 1–100; `next_page_params` indicates whether more data exists. These commands do not imply complete account history. Explorer failures are errors, not empty histories. Tracing depends on the node's tracing methods and retained history; a custom `--rpc-url` is honored exclusively. `ONCHAIN_TRACE_RPC_URL` supplies an optional fallback when no explicit RPC was given.

## Output and performance

Successful commands emit one JSON document when piped or when `--json` is set. Runtime errors emit `error` and `message` in JSON; diagnostics go to stderr. Exit codes: 0 success, 1 validation/explorer/decode failure, 2 configuration or CLI syntax error, 3 RPC failure. Help and CLI parser diagnostics use Clap's normal text output.

Benchmark your actual endpoint with `onchain bench` or `onchain zcash bench`. Benchmarks report measured successes/failures; an operation with no successful samples fails instead of reporting zero-millisecond latency. Latency depends on node location, method, load, and connection setup. No generic sub-200ms guarantee is made.

## Development

```sh
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo test --workspace --locked
cargo build --release --locked
cargo audit
```

Tests use local mock servers, including malformed/error replies, timeout boundaries, exact amounts, chain mismatches, historical reads, pagination, and quote integrity. CI runs these checks. Version tags publish four platform archives and `SHA256SUMS` after tests pass.

See [CONTRIBUTING.md](CONTRIBUTING.md) and [CHANGELOG.md](CHANGELOG.md). Licensed under [MIT](LICENSE).
