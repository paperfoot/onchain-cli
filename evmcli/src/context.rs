use crate::cli::{Cli, Commands};
use crate::config::{self, ChainConfig};
use crate::errors::EvmError;
use crate::output::OutputFormat;
use crate::rpc::{detect, provider};

pub struct AppContext {
    pub provider: provider::ReadProvider,
    pub http: reqwest::Client,
    pub chain: &'static ChainConfig,
    pub format: OutputFormat,
    pub rpc_url: String,
    pub rpc_explicit: bool,
    pub explorer_override: Option<String>,
}

impl AppContext {
    pub fn explorer_api_url(&self) -> String {
        self.explorer_override
            .as_deref()
            .map(|s| s.trim_end_matches('/').to_string())
            .unwrap_or_else(|| self.chain.explorer_api_url())
    }

    pub fn explorer_v2_url(&self) -> String {
        format!("{}/v2", self.explorer_api_url())
    }

    pub async fn new(cli: &Cli) -> Result<Self, EvmError> {
        let chain = config::resolve_chain(&cli.network)?;
        let http = provider::build_http_client();
        if let Some(url) = &cli.explorer_url {
            provider::validate_url(url)?;
        }

        // Explorer-only commands and trace fallbacks must work without an ordinary RPC.
        let rpc_url = match cli.command {
            Commands::Txs { .. }
            | Commands::Transfers { .. }
            | Commands::Abi { .. }
            | Commands::Trace { .. } => cli
                .rpc_url
                .clone()
                .unwrap_or_else(|| chain.public_rpc.to_string()),
            _ => detect::select_endpoint(cli.rpc_url.as_deref(), chain, &http).await?,
        };

        // Reuse the probe connection for the actual query (including its TLS session).
        let alloy_provider = provider::build_read_provider_with_client(&rpc_url, http.clone())?;
        let format = OutputFormat::detect(cli.json);

        Ok(Self {
            provider: alloy_provider,
            http,
            chain,
            format,
            rpc_url,
            rpc_explicit: cli.rpc_url.is_some(),
            explorer_override: cli.explorer_url.clone(),
        })
    }
}
