# Contributing to onchain

Thanks for your interest in contributing.

## Getting Started

```bash
git clone https://github.com/paperfoot/onchain-cli.git
cd onchain-cli/evmcli
cargo build
cargo test
```

## Development

The project uses a Cargo workspace. The main binary lives in `evmcli/`.

- `evmcli/src/cli.rs` -- Command definitions (clap derive)
- `evmcli/src/commands/` -- One file per command
- `evmcli/src/rpc/` -- RPC endpoint detection and provider setup
- `evmcli/src/output/` -- Table and JSON rendering
- `evmcli/src/config.rs` -- Chain configurations

### Adding a New Command

1. Add the variant to `Commands` in `cli.rs`
2. Create `commands/your_command.rs`
3. Register the module in `commands/mod.rs`
4. Wire it up in `main.rs`

### Adding a New Network

Add a `ChainConfig` entry to the `CHAINS` array in `config.rs`.

## Pull Requests

- Keep PRs focused on a single change
- Run `cargo clippy` and `cargo test` before submitting
- Use conventional commit messages (`feat:`, `fix:`, `chore:`)

## Reporting Issues

Open an issue on GitHub with:
- What you ran
- What you expected
- What happened instead
- Your OS and Rust version (`rustc --version`)

## License

By contributing, you agree that your contributions will be licensed under the MIT License.
