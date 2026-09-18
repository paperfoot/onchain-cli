# Changelog

## 0.2.0 — 2026-09-18

- Add native Zcash info, health, block, transaction, transparent balance/UTXO, mempool, batched reads, exact ZEC conversion, ZIP-317 fee calculation, and connection benchmarks.
- Add NEAR Intents 1Click asset discovery, dry exact-input quotes, and existing-swap status. Validate quote request echoes and exact amounts. Wallet signing, shielded sends, and swap funding remain outside this release.
- Replace obsolete root prototype with one Cargo workspace and lockfile; upgrade Alloy, reqwest, directories, and self-update dependencies.
- Fix endpoint racing and cached endpoint checks, validate explicit network overrides, reuse HTTP connections, and replace the retired Polygon RPC.
- Honor historical storage blocks and return fixed-width hex plus decimal storage values.
- Fix ABI signature parsing, tuple encoding, dynamic output decoding, and selector validation; decode known calldata offline.
- Match log participants in either indexed position, sort and deduplicate results, and avoid unnecessary latest-block calls.
- Use Blockscout v2 APIs with bounded cursor pagination, accurate block numbers, NFT token types/IDs/quantities, and validated ABI caches.
- Preserve exact balances and gas amounts; detect EIP-7702 delegated accounts; support EVM network-specific transaction envelopes.
- Keep trace requests on the selected chain and hide endpoint credentials in results and errors.
- Expose benchmark failures, enforce complete-request timeouts, and fix self-update repository, async compatibility, semantic version ordering, and checksum verification.
- Add deterministic regression tests and CI/release workflows for macOS/Linux on ARM64 and x86-64.

Compatibility: the obsolete root `evmtool` binary is removed. Use `onchain`. Storage `value` is now fixed-width hexadecimal, with a separate `value_decimal`; explorer output adds continuation metadata. EVM command names and default network remain unchanged.
