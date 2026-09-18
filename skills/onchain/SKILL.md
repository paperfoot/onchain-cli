---
name: onchain
description: Query EVM and native Zcash chains, investigate transactions, decode calldata, and obtain dry cross-chain swap quotes with the onchain CLI. Use for onchain CLI work, ZEC node queries, balances, transfers, gas, and swap previews.
---

# onchain

Installed executable: `~/.local/bin/onchain`. Check `onchain --version` and command `--help` for the installed interface. Version 0.2 adds native Zcash and NEAR Intents 1Click previews. Piped output or `--json` returns one JSON document; a nonzero exit means failure, including provider errors. CLI parsing/help uses text.

## Native Zcash

Use `onchain zcash`, not EVM `--network`, for native ZEC. The default RPC is a local mainnet node at `http://127.0.0.1:8232`. Configure `ONCHAIN_ZCASH_RPC_URL` for a different node and `--zcash-network mainnet|testnet|regtest` for its chain. Authentication uses `ONCHAIN_ZCASH_COOKIE_FILE` or both `ONCHAIN_ZCASH_RPC_USER` and `ONCHAIN_ZCASH_RPC_PASSWORD`; keep secrets out of command arguments and logs. Authenticated remote nodes require HTTPS.

```sh
onchain zcash health
onchain zcash info
onchain zcash balance TRANSPARENT_ADDRESS
onchain zcash utxos TRANSPARENT_ADDRESS
onchain zcash tx TRANSACTION_ID --block-hash BLOCK_HASH
onchain zcash block latest
onchain zcash mempool
onchain zcash amount 1.23456789
onchain zcash fee --actions 2
onchain zcash bench --iterations 10
```

Amounts are exact: 1 ZEC = 100,000,000 zatoshis. `fee` computes the ZIP-317 conventional fee from a supplied logical-action count; it does not construct a transaction or estimate live inclusion. `balance` and `utxos` cover transparent addresses, not shielded notes.

For several reads, use `onchain zcash batch reads.json` with an array of 1–100 `{ "method": "getblockcount", "params": [] }` objects. Each online request checks the chain in the same HTTP batch; results are not cached. The allowlist excludes signing, broadcasting, exports, and node administration. `--timeout-ms` bounds the entire request. `health` checks sync progress and tip freshness, not wallet or exchange readiness.

For sustained work, use a synced local node or a dedicated provider. Public demonstration endpoints can rate-limit bursts. Benchmark the selected endpoint; there is no fixed latency guarantee. The repository's verification record does not imply a node or wallet is installed on the current machine.

## Swap previews

```sh
onchain swap tokens --chain zec
onchain swap tokens --symbol USDC
onchain swap quote --from nep141:zec.omft.near --to DESTINATION_ASSET_ID \
  --amount 100000000 --recipient DESTINATION_ADDRESS --refund-to ZCASH_ADDRESS \
  --slippage-bps 100
onchain swap status DEPOSIT_ADDRESS
```

Discover exact asset IDs from `tokens`; native ZEC has `blockchain: "zec"`, not merely symbol ZEC. Quote amounts are integer base units of the origin asset. Every quote is `dry: true`, creates no deposit address, and moves no funds. The CLI validates the request echo and preserves the complete provider response. Preview prices are not reserved executable prices. Status accepts `--deposit-memo` for memo-based deposits.

Use `ONCHAIN_SWAP_API_KEY` or `ONCHAIN_SWAP_JWT` if provider authentication is needed. `--api-url` selects a compatible 1Click API; `--timeout-ms` controls its deadline. Authentication and rate-limit failures are explicit; quotes are not automatically retried or cached.

Version 0.2 does not implement wallet signing, shielded sends, swap funding, or trade execution. For execution requests, establish the user's wallet and venue and use their supported integration; do not treat a preview as a completed swap.

## EVM

The default network is Arbitrum. Supported names are `arbitrum`, `ethereum`, `base`, `optimism`, and `polygon`. Use `--network`/`ONCHAIN_NETWORK` and optionally `--rpc-url`/`ONCHAIN_RPC_URL`. A custom endpoint must match the selected chain ID; it does not add arbitrary chains.

```sh
onchain --network ethereum balance ADDRESS --token TOKEN_CONTRACT
onchain --network ethereum call CONTRACT 'balanceOf(address)(uint256)' ADDRESS
onchain --network base gas
onchain tx TRANSACTION_HASH
onchain receipt TRANSACTION_HASH
onchain txs ADDRESS --pages 2
onchain transfers ADDRESS --token-type erc721 --pages 2
onchain logs --event transfer --participant ADDRESS --from-block 19000000
onchain storage CONTRACT 0x0 --block 20000000
onchain abi CONTRACT
onchain decode CALLDATA --sig 'transfer(address,uint256)'
onchain trace TRANSACTION_HASH
```

Contract signatures include input and output types, such as `name()(string)`. `txs`/`transfers` default to one explorer page; inspect `next_page_params` before describing history as complete. Tracing needs `debug_traceTransaction`; historical reads need retained state. A custom RPC is honored exclusively for tracing. Use `ONCHAIN_TRACE_RPC_URL` for a fallback only when no explicit RPC was supplied.

Empty bytecode or a low nonce alone does not establish ownership, intent, or account type; delegated EVM accounts can contain code. Decoded selectors may have collisions. Keep investigation conclusions tied to observed transactions.

`onchain update --check` checks this repository's release; `onchain update` installs a newer checksummed binary. Documentation and releases: https://github.com/paperfoot/onchain-cli.
