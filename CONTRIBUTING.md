# Contributing

The root is a Cargo workspace; `evmcli/` contains the `onchain` library and binary. Rust 1.94.1 or newer is required. Run all checks from the repository root:

```sh
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo test --workspace --locked
cargo build --release --locked
cargo audit
```

Code map:

- `evmcli/src/cli.rs`: EVM/global commands.
- `evmcli/src/commands/`: EVM query implementations and updater.
- `evmcli/src/rpc/`: endpoint discovery and pooled HTTP provider.
- `evmcli/src/explorer.rs`: bounded Blockscout pagination.
- `evmcli/src/zcash/`: native Zcash CLI, exact amounts, health checks, validated RPC batches.
- `evmcli/src/swap.rs`: 1Click asset discovery, dry quotes, and status.
- `evmcli/src/output/`: terminal tables and machine-readable JSON.
- `evmcli/tests/`: mock HTTP regression tests.

Keep amounts exact; use integer base units or decimal strings. Never guess token decimals, substitute zero on network errors, or cache balances and quotes. Keep credentials out of output. A public chain query cannot reveal a shielded wallet balance.

Tests must run without production credentials or real funds. Use Wiremock for response schemas, errors, and request verification. Live smoke tests are read-only and explicitly separate from deterministic CI.

To release, update `evmcli/Cargo.toml`, regenerate the workspace lockfile, document the changes, pass all checks, push main, and tag that tested commit `v<version>`. The release workflow checks the version, tests/builds on native macOS and Linux runners, publishes archives, and generates checksums. Verify the published archives and self-update behavior before declaring the release installed.

Open issues with the exact command (redact credentials), expected/observed behavior, `onchain --version`, and OS. Do not include wallet secrets or API tokens.
