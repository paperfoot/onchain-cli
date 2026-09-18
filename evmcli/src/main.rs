use clap::Parser;
use onchain::cli::{Cli, Commands};
use onchain::context::AppContext;
use onchain::output::{self, OutputFormat};
use std::process;
use std::time::Instant;

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .with_target(false)
        .with_writer(std::io::stderr)
        .init();

    let cli = Cli::parse();
    let format = OutputFormat::detect(cli.json);

    let start = Instant::now();

    // Handle commands that don't need RPC context
    match &cli.command {
        Commands::AgentInfo { command } => {
            match onchain::discovery::manifest(command.as_deref()) {
                Ok(value) => output::json::render(&value),
                Err(error) => {
                    output::render_error(&error, format);
                    process::exit(error.exit_code());
                }
            }
            return;
        }
        Commands::Swap(args) => {
            match onchain::swap::run(args).await {
                Ok(r) => output::render(&r, format),
                Err(e) => {
                    output::render_error(&e, format);
                    process::exit(e.exit_code());
                }
            }
            return;
        }
        Commands::Zcash(args) => {
            match onchain::zcash::run(args, cli.rpc_url.as_deref()).await {
                Ok(r) => output::render(&r, format),
                Err(e) => {
                    output::render_error(&e, format);
                    process::exit(e.exit_code());
                }
            }
            return;
        }
        Commands::Examples => {
            if format == OutputFormat::Json {
                output::json::render(&serde_json::json!({"examples": onchain::cli::EXAMPLES}));
            } else {
                println!("{}", onchain::cli::EXAMPLES);
            }
            return;
        }
        Commands::Decode { data, sig } => {
            match onchain::commands::decode::run_with_signature(data, sig.as_deref()).await {
                Ok(r) => output::render(&r, format),
                Err(e) => {
                    output::render_error(&e, format);
                    process::exit(e.exit_code());
                }
            }
            return;
        }
        Commands::Update { check } => {
            match onchain::commands::update::run(*check).await {
                Ok(r) => output::render(&r, format),
                Err(e) => {
                    output::render_error(&e, format);
                    process::exit(e.exit_code());
                }
            }
            return;
        }
        _ => {}
    }

    if let Err(e) = cli.validate() {
        output::render_error(&e, format);
        process::exit(e.exit_code());
    }

    let ctx = match AppContext::new(&cli).await {
        Ok(ctx) => ctx,
        Err(e) => {
            output::render_error(&e, format);
            process::exit(e.exit_code());
        }
    };

    let result = match cli.command {
        Commands::Balance {
            ref address,
            ref token,
        } => onchain::commands::balance::run(&ctx, address, token.as_deref())
            .await
            .map(|r| output::render(&r, format)),
        Commands::Tx { ref hash } => onchain::commands::tx::run(&ctx, hash)
            .await
            .map(|r| output::render(&r, format)),
        Commands::Receipt { ref hash } => onchain::commands::receipt::run(&ctx, hash)
            .await
            .map(|r| output::render(&r, format)),
        Commands::Block { ref id } => onchain::commands::block::run(&ctx, id)
            .await
            .map(|r| output::render(&r, format)),
        Commands::Gas => onchain::commands::gas::run(&ctx)
            .await
            .map(|r| output::render(&r, format)),
        Commands::Call {
            ref address,
            ref sig,
            ref args,
        } => onchain::commands::call::run(&ctx, address, sig, args)
            .await
            .map(|r| output::render(&r, format)),
        Commands::Txs { ref address, pages } => {
            onchain::commands::explorer::run(&ctx, address, pages)
                .await
                .map(|r| output::render(&r, format))
        }
        Commands::Abi { ref address } => onchain::commands::abi::run(&ctx, address)
            .await
            .map(|r| output::render(&r, format)),
        Commands::Logs {
            ref address,
            ref topic0,
            ref participant,
            from_block,
            to_block,
            ref event,
        } => onchain::commands::logs::run(
            &ctx,
            address.as_deref(),
            topic0.as_deref(),
            participant.as_deref(),
            from_block,
            to_block,
            event.as_deref(),
        )
        .await
        .map(|r| output::render(&r, format)),
        Commands::Transfers {
            ref address,
            ref token_type,
            pages,
        } => onchain::commands::transfers::run(&ctx, address, token_type, pages)
            .await
            .map(|r| output::render(&r, format)),
        Commands::Storage {
            ref address,
            ref slot,
            block,
        } => onchain::commands::storage::run(&ctx, address, slot, block)
            .await
            .map(|r| output::render(&r, format)),
        Commands::Nonce { ref address } => onchain::commands::nonce::run(&ctx, address)
            .await
            .map(|r| output::render(&r, format)),
        Commands::Code { ref address } => onchain::commands::code::run(&ctx, address)
            .await
            .map(|r| output::render(&r, format)),
        Commands::Trace { ref hash } => onchain::commands::trace::run(&ctx, hash)
            .await
            .map(|r| output::render(&r, format)),
        Commands::Bench {
            iterations,
            warmup,
            ref address,
        } => onchain::commands::bench::run(&ctx, iterations, warmup, address)
            .await
            .map(|r| output::render(&r, format)),
        Commands::Examples
        | Commands::AgentInfo { .. }
        | Commands::Update { .. }
        | Commands::Decode { .. }
        | Commands::Zcash(_)
        | Commands::Swap(_) => unreachable!(), // handled above
    };

    let elapsed = start.elapsed();
    tracing::debug!("Completed in {:.3}s", elapsed.as_secs_f64());

    if let Err(e) = result {
        output::render_error(&e, format);
        process::exit(e.exit_code());
    }
}
