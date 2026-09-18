use crate::{errors::EvmError, output::table::Tableable, rpc::provider::validate_url};
use alloy::primitives::U256;
use clap::{Args, Subcommand};
use comfy_table::Table;
use serde::Serialize;
use serde_json::{json, Value};
use std::time::{Duration, Instant};

#[derive(Args)]
pub struct SwapArgs {
    #[command(subcommand)]
    pub command: Command,
    /// 1Click API base URL; credentials come from ONCHAIN_SWAP_API_KEY or ONCHAIN_SWAP_JWT
    #[arg(
        long,
        global = true,
        default_value = "https://1click.chaindefuser.com",
        env = "ONCHAIN_SWAP_API_URL",
        hide_env_values = true
    )]
    pub api_url: String,
    #[arg(long, global = true, default_value_t = 15000, value_parser = clap::value_parser!(u64).range(100..=120000))]
    pub timeout_ms: u64,
}
#[derive(Subcommand)]
pub enum Command {
    /// List current supported assets and their exact IDs (native Zcash chain is zec)
    Tokens {
        #[arg(long)]
        chain: Option<String>,
        #[arg(long)]
        symbol: Option<String>,
    },
    /// Request an exact-input preview; dry=true, no deposit address or funds moved
    Quote(QuoteArgs),
    /// Inspect an existing swap by its deposit address
    Status {
        deposit_address: String,
        #[arg(long)]
        deposit_memo: Option<String>,
    },
}
#[derive(Args)]
pub struct QuoteArgs {
    /// Origin assetId from swap tokens (native ZEC: nep141:zec.omft.near)
    #[arg(long)]
    pub from: String,
    /// Destination assetId from swap tokens
    #[arg(long)]
    pub to: String,
    /// Exact input in smallest integer units (100000000 = 1 ZEC)
    #[arg(long)]
    pub amount: String,
    /// Address on the destination chain
    #[arg(long)]
    pub recipient: String,
    /// Refund address on the origin chain
    #[arg(long)]
    pub refund_to: String,
    /// Slippage tolerance in basis points (100 = 1%)
    #[arg(long, default_value_t = 100, value_parser = clap::value_parser!(u16).range(0..=5000))]
    pub slippage_bps: u16,
    /// Requested swap deadline relative to now
    #[arg(long, default_value_t = 30, value_parser = clap::value_parser!(u32).range(5..=1440))]
    pub deadline_minutes: u32,
}

impl QuoteArgs {
    pub fn request(&self) -> Result<Value, EvmError> {
        if self.from.trim().is_empty() || self.to.trim().is_empty() || self.from == self.to {
            return Err(EvmError::validation(
                "Swap requires two distinct asset IDs from 'onchain swap tokens'",
            ));
        }
        if self.amount.is_empty()
            || !self.amount.bytes().all(|b| b.is_ascii_digit())
            || self
                .amount
                .parse::<U256>()
                .ok()
                .filter(|v| !v.is_zero())
                .is_none()
        {
            return Err(EvmError::validation(
                "Swap amount must be a positive integer in the origin token's smallest units",
            ));
        }
        if self.recipient.trim().is_empty() || self.refund_to.trim().is_empty() {
            return Err(EvmError::validation(
                "Swap recipient and refund address are required",
            ));
        }
        if self.slippage_bps > 5000 || !(5..=1440).contains(&self.deadline_minutes) {
            return Err(EvmError::validation("Invalid swap slippage or deadline"));
        }
        // All requests are previews. Creating a funded swap is a separate wallet integration.
        let deadline =
            chrono::Utc::now() + chrono::Duration::minutes(i64::from(self.deadline_minutes));
        Ok(json!({
            "dry":true, "swapType":"EXACT_INPUT", "slippageTolerance":self.slippage_bps,
            "originAsset":self.from,"destinationAsset":self.to,"amount":self.amount,
            "depositType":"ORIGIN_CHAIN","refundTo":self.refund_to,"refundType":"ORIGIN_CHAIN",
            "recipient":self.recipient,"recipientType":"DESTINATION_CHAIN",
            "deadline":deadline.to_rfc3339_opts(chrono::SecondsFormat::Secs, true)
        }))
    }
}
#[derive(Debug, Serialize)]
pub struct SwapResult {
    pub provider: &'static str,
    pub operation: &'static str,
    pub elapsed_ms: u128,
    pub result: Value,
}
impl Tableable for SwapResult {
    fn to_table(&self) -> Table {
        let mut table = Table::new();
        if self.operation == "tokens" {
            table.set_header(["Chain", "Symbol", "Decimals", "Asset ID"]);
            if let Some(tokens) = self.result.as_array() {
                for token in tokens {
                    table.add_row([
                        token["blockchain"].as_str().unwrap_or("?").to_string(),
                        token["symbol"].as_str().unwrap_or("?").to_string(),
                        token["decimals"].to_string(),
                        token["assetId"].as_str().unwrap_or("?").to_string(),
                    ]);
                }
            }
        } else {
            table.set_header(["Field", "Value"]);
            table.add_row(["Operation", self.operation]);
            let fields = if self.operation == "quote_preview" {
                &self.result["quote"]
            } else {
                &self.result
            };
            if let Some(fields) = fields.as_object() {
                for (key, value) in fields {
                    table.add_row([
                        key.clone(),
                        value
                            .as_str()
                            .map(str::to_owned)
                            .unwrap_or_else(|| value.to_string()),
                    ]);
                }
            }
        }
        table
    }
}

fn validate_quote(response: &Value, request: &Value) -> Result<(), EvmError> {
    for key in [
        "dry",
        "swapType",
        "slippageTolerance",
        "originAsset",
        "destinationAsset",
        "amount",
        "depositType",
        "refundTo",
        "refundType",
        "recipient",
        "recipientType",
    ] {
        if response["quoteRequest"].get(key) != request.get(key) {
            return Err(EvmError::explorer(format!(
                "Swap quote response does not match requested {key}"
            )));
        }
    }
    let parse_deadline = |value: &Value| {
        value
            .as_str()
            .and_then(|s| chrono::DateTime::parse_from_rfc3339(s).ok())
    };
    let echoed = parse_deadline(&response["quoteRequest"]["deadline"]);
    if echoed.is_none() || echoed != parse_deadline(&request["deadline"]) {
        return Err(EvmError::explorer(
            "Swap quote response does not match requested deadline",
        ));
    }
    let quote = &response["quote"];
    for field in ["amountIn", "minAmountIn", "amountOut", "minAmountOut"] {
        let value = quote[field]
            .as_str()
            .ok_or_else(|| EvmError::explorer("Swap quote is missing integer amounts"))?;
        if value.is_empty()
            || !value.bytes().all(|b| b.is_ascii_digit())
            || value.parse::<U256>().is_err()
        {
            return Err(EvmError::explorer(
                "Swap quote contains invalid integer amounts",
            ));
        }
    }
    if quote["amountIn"]
        .as_str()
        .and_then(|s| s.parse::<U256>().ok())
        != request["amount"]
            .as_str()
            .and_then(|s| s.parse::<U256>().ok())
    {
        return Err(EvmError::explorer(
            "Swap quote input amount differs from the requested exact input",
        ));
    }
    let amount_out = quote["amountOut"]
        .as_str()
        .unwrap()
        .parse::<U256>()
        .unwrap();
    let min_out = quote["minAmountOut"]
        .as_str()
        .unwrap()
        .parse::<U256>()
        .unwrap();
    if amount_out.is_zero() || min_out.is_zero() || min_out > amount_out {
        return Err(EvmError::explorer(
            "Swap quote output or minimum output is invalid",
        ));
    }
    Ok(())
}

pub async fn run(args: &SwapArgs) -> Result<SwapResult, EvmError> {
    let start = Instant::now();
    let quote_request = match &args.command {
        Command::Quote(quote) => Some(quote.request()?),
        _ => None,
    };
    let url = validate_url(&args.api_url)?;
    let api_key = std::env::var("ONCHAIN_SWAP_API_KEY").ok();
    let jwt = std::env::var("ONCHAIN_SWAP_JWT").ok();
    if (api_key.is_some() || jwt.is_some())
        && url.scheme() != "https"
        && !matches!(
            url.host_str(),
            Some("localhost" | "127.0.0.1" | "[::1]" | "::1")
        )
    {
        return Err(EvmError::config(
            "Authenticated swap APIs require HTTPS or loopback",
        ));
    }
    let http = reqwest::Client::builder()
        .user_agent(concat!("onchain/", env!("CARGO_PKG_VERSION")))
        .tcp_nodelay(true)
        .connect_timeout(Duration::from_millis(args.timeout_ms.min(3000)))
        .timeout(Duration::from_millis(args.timeout_ms))
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .map_err(|_| EvmError::config("Cannot build swap HTTP client"))?;
    let base = args.api_url.trim_end_matches('/');
    let (operation, mut request) = match &args.command {
        Command::Tokens { .. } => ("tokens", http.get(format!("{base}/v0/tokens"))),
        Command::Quote(_) => (
            "quote_preview",
            http.post(format!("{base}/v0/quote"))
                .json(quote_request.as_ref().unwrap()),
        ),
        Command::Status {
            deposit_address,
            deposit_memo,
        } => {
            if deposit_address.trim().is_empty() {
                return Err(EvmError::validation("Swap deposit address is required"));
            }
            let mut request = http
                .get(format!("{base}/v0/status"))
                .query(&[("depositAddress", deposit_address)]);
            if let Some(memo) = deposit_memo {
                request = request.query(&[("depositMemo", memo)]);
            }
            ("status", request)
        }
    };
    if let Some(key) = api_key {
        request = request.header("X-API-Key", key);
    } else if let Some(jwt) = jwt {
        request = request.bearer_auth(jwt);
    }
    let response = request
        .send()
        .await
        .map_err(|e| EvmError::explorer(e.to_string()))?;
    let status = response.status();
    if status == reqwest::StatusCode::UNAUTHORIZED || status == reqwest::StatusCode::FORBIDDEN {
        return Err(EvmError::config(
            "1Click authentication failed; set ONCHAIN_SWAP_API_KEY or ONCHAIN_SWAP_JWT",
        ));
    }
    if status == reqwest::StatusCode::TOO_MANY_REQUESTS {
        return Err(EvmError::explorer(
            "1Click rate limit reached (HTTP 429); retry later or use your API key",
        ));
    }
    let mut result: Value = response
        .json()
        .await
        .map_err(|_| EvmError::explorer(format!("1Click returned invalid JSON (HTTP {status})")))?;
    if !status.is_success() {
        return Err(EvmError::explorer(format!(
            "1Click HTTP {status}: {}",
            result["message"].as_str().unwrap_or("request failed")
        )));
    }
    match &args.command {
        Command::Tokens { chain, symbol } => {
            let tokens = result
                .as_array_mut()
                .ok_or_else(|| EvmError::explorer("1Click tokens response is not an array"))?;
            for token in tokens.iter() {
                if token["assetId"].as_str().is_none()
                    || token["blockchain"].as_str().is_none()
                    || token["symbol"].as_str().is_none()
                    || token["decimals"].as_u64().filter(|d| *d <= 255).is_none()
                {
                    return Err(EvmError::explorer(
                        "1Click token response is missing asset metadata",
                    ));
                }
            }
            tokens.retain(|token| {
                chain.as_ref().is_none_or(|c| {
                    token["blockchain"]
                        .as_str()
                        .is_some_and(|v| v.eq_ignore_ascii_case(c))
                }) && symbol.as_ref().is_none_or(|s| {
                    token["symbol"]
                        .as_str()
                        .is_some_and(|v| v.eq_ignore_ascii_case(s))
                })
            });
        }
        Command::Quote(_) => validate_quote(&result, quote_request.as_ref().unwrap())?,
        Command::Status { .. } => {
            if !matches!(
                result["status"].as_str(),
                Some(
                    "KNOWN_DEPOSIT_TX"
                        | "PENDING_DEPOSIT"
                        | "INCOMPLETE_DEPOSIT"
                        | "PROCESSING"
                        | "SUCCESS"
                        | "REFUNDED"
                        | "FAILED"
                )
            ) {
                return Err(EvmError::explorer("1Click returned an unknown swap status"));
            }
        }
    }
    Ok(SwapResult {
        provider: "near_intents_1click",
        operation,
        elapsed_ms: start.elapsed().as_millis(),
        result,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn quote_is_always_dry_and_amount_is_exact() {
        let mut quote = QuoteArgs {
            from: "nep141:zec.omft.near".into(),
            to: "nep141:eth.omft.near".into(),
            amount: "100000001".into(),
            recipient: "recipient".into(),
            refund_to: "refund".into(),
            slippage_bps: 100,
            deadline_minutes: 30,
        };
        let request = quote.request().unwrap();
        assert_eq!(request["dry"], true);
        assert_eq!(request["amount"], "100000001");
        for amount in ["0", "-1", "1.5", "1e8", "", "0x10"] {
            quote.amount = amount.into();
            assert!(quote.request().is_err());
        }
    }
    #[test]
    fn quote_must_match_addresses_amounts_and_slippage() {
        let request = json!({"dry":true,"recipient":"alice","amount":"100","deadline":"2026-09-18T18:00:00Z"});
        let valid = json!({"quoteRequest":request,"quote":{"amountIn":"100","minAmountIn":"100","amountOut":"90","minAmountOut":"89"}});
        assert!(validate_quote(&valid, &request).is_ok());
        let mut normalized = valid.clone();
        normalized["quoteRequest"]["deadline"] = json!("2026-09-18T18:00:00.000Z");
        assert!(validate_quote(&normalized, &request).is_ok());
        normalized["quoteRequest"]["deadline"] = json!("2026-09-18T18:00:01Z");
        assert!(validate_quote(&normalized, &request).is_err());
        let mut modified = valid.clone();
        modified["quoteRequest"]["recipient"] = json!("mallory");
        assert!(validate_quote(&modified, &request).is_err());
        let mut modified = valid.clone();
        modified["quote"]["amountIn"] = json!("101");
        assert!(validate_quote(&modified, &request).is_err());
        let mut modified = valid;
        modified["quote"]["minAmountOut"] = json!("91");
        assert!(validate_quote(&modified, &request).is_err());
    }
}
