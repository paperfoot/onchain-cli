# v0.2.0 verification

Checked on 18 September 2026 on macOS ARM64 with Rust 1.98.1.

## Deterministic checks

- 36 passing tests: 17 unit, 6 EVM HTTP regressions, 7 Zcash RPC regressions, 6 swap HTTP regressions.
- Workspace formatting, strict all-target Clippy, optimized release build, workflow actionlint, and Git whitespace checks pass.
- Cargo audit: zero known vulnerabilities. Two unmaintained transitive packages remain in the resolved dependency graph (`paste`, `derivative`); the advisory check does not classify these as vulnerabilities.
- JSON output and closed-pipe behavior checked from a subprocess.

## Live reads

- Gas and current blocks succeed on Arbitrum, Ethereum, Base, Optimism, and Polygon.
- Ethereum native/token balance, dynamic contract return, bytecode, nonce, ABI, two transaction-history pages, NFT transfers, transaction, and receipt succeed.
- Historical WETH storage at block 20,000,000 succeeds through `eth.drpc.org`. The default public Ethereum endpoint rejects archive requests without a provider token; that restriction is surfaced as an error.
- Native Zcash info, chain health, latest block, transaction within that block, transparent balance, UTXOs, and mempool succeed through QuickNode's documented demonstration endpoint. Addresses used are public examples, not user wallet data.
- 1Click asset discovery returns native ZEC (`nep141:zec.omft.near`, chain `zec`, 8 decimals). A live ZEC-to-Ethereum-USDC dry quote succeeds, echoes the request, and contains no deposit address. Provider timestamps normalize to millisecond precision; validation compares timestamp values.
- Free Zcash gateways impose rate limits. Concurrent/burst tests deliberately exposed HTTP 429 and JSON-RPC rate-limit errors; these are reported, not converted to empty or zero-valued results. The default remains a local node, not a shared public gateway.

## Observed performance

These are small samples from the test machine, not service guarantees.

| Operation | Observed sample |
|---|---|
| Ethereum balance, existing connection | mean 40.6 ms, 3/3 successful |
| Ethereum block number, existing connection | mean 39.9 ms, 3/3 successful |
| Ethereum gas price, existing connection | mean 43.5 ms, 3/3 successful |
| Blockscout transaction page | mean 117.7 ms, 3/3 successful |
| Zcash node reads, successful samples | warm median about 89 ms; first successful request 406 ms |
| Zcash 10-request burst against demo gateway | 6 successful, 4 rate-limited |
| ZEC-to-USDC dry quote, complete command request | 518 ms |

For sustained Zcash work, configure a synced local node or a dedicated authenticated provider and rerun `zcash health` and `zcash bench`. No Zcash node, wallet, or paid provider has been provisioned by this update. Wallet signing, shielded note access, swap funding, and live trading remain to be integrated with the selected wallet/venue. No funds were moved in verification.
