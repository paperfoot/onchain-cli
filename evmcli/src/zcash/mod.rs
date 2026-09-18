pub mod client;

use crate::errors::EvmError;
use crate::output::table::Tableable;
use clap::{Args, Subcommand, ValueEnum};
use client::{Client, ReadRequest};
use comfy_table::Table;
use serde::Serialize;
use serde_json::{json, Value};
use std::{path::PathBuf, time::Instant};

#[derive(Clone, Copy, Debug, ValueEnum)]
pub enum Network {
    Mainnet,
    Testnet,
    Regtest,
}
impl Network {
    fn chain(self) -> &'static str {
        match self {
            Self::Mainnet => "main",
            Self::Testnet => "test",
            Self::Regtest => "regtest",
        }
    }
    fn default_url(self) -> &'static str {
        match self {
            Self::Mainnet => "http://127.0.0.1:8232",
            Self::Testnet => "http://127.0.0.1:18232",
            Self::Regtest => "http://127.0.0.1:18232",
        }
    }
}

#[derive(Args)]
pub struct ZcashArgs {
    #[command(subcommand)]
    pub command: Command,
    /// Zcash node URL (defaults to localhost; never an EVM RPC)
    #[arg(
        long,
        global = true,
        env = "ONCHAIN_ZCASH_RPC_URL",
        hide_env_values = true
    )]
    pub zcash_rpc_url: Option<String>,
    #[arg(
        long,
        global = true,
        value_enum,
        default_value = "mainnet",
        env = "ONCHAIN_ZCASH_NETWORK"
    )]
    pub zcash_network: Network,
    /// Node cookie file containing user:password; alternatively set ONCHAIN_ZCASH_RPC_USER/PASSWORD
    #[arg(long, global = true, env = "ONCHAIN_ZCASH_COOKIE_FILE")]
    pub cookie_file: Option<PathBuf>,
    /// Complete request deadline, including response body (milliseconds)
    #[arg(long, global = true, default_value_t = 10000, value_parser = clap::value_parser!(u64).range(100..=120000))]
    pub timeout_ms: u64,
}

#[derive(Subcommand)]
pub enum Command {
    /// Node chain, sync progress, consensus upgrades and shielded pool totals
    Info,
    /// Fail if the node is behind, unsynced, or its tip is stale
    Health {
        #[arg(long, default_value_t = 600, value_parser = clap::value_parser!(u64).range(1..))]
        max_tip_age_seconds: u64,
        #[arg(long, default_value_t = 2)]
        max_lag_blocks: u64,
    },
    /// Transparent address balance (shielded funds require a wallet integration)
    Balance { address: String },
    /// Transparent unspent outputs, including chain tip information
    Utxos { address: String },
    /// Native transaction details (an optional block hash avoids a txindex lookup)
    Tx {
        txid: String,
        #[arg(long)]
        block_hash: Option<String>,
    },
    /// Block by decimal height, hash, or latest
    Block {
        #[arg(default_value = "latest")]
        id: String,
        #[arg(long)]
        full: bool,
    },
    /// Mempool size and memory usage
    Mempool,
    /// Run 1-100 read-only requests from a JSON file in one HTTP round trip
    Batch { file: PathBuf },
    /// Convert decimal ZEC to exact integer zatoshis, offline
    Amount { zec: String },
    /// ZIP-317 conventional fee for a known logical action count, offline
    Fee {
        #[arg(long)]
        actions: u32,
    },
    /// Measure fresh node reads over a reused connection (no cached results)
    Bench {
        #[arg(long, default_value_t = 10, value_parser = clap::value_parser!(u32).range(1..=1000))]
        iterations: u32,
    },
}

#[derive(Debug, Serialize)]
pub struct ResultData {
    pub network: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rpc_endpoint: Option<String>,
    pub elapsed_ms: u128,
    pub result: Value,
}
impl Tableable for ResultData {
    fn to_table(&self) -> Table {
        let mut table = Table::new();
        table.set_header(["Field", "Value"]);
        table.add_row(["Network", self.network]);
        if let Some(endpoint) = &self.rpc_endpoint {
            table.add_row(["RPC", endpoint]);
        }
        if let Value::Object(map) = &self.result {
            for (key, value) in map {
                table.add_row([key.clone(), display(value)]);
            }
        } else {
            table.add_row(["Result".into(), display(&self.result)]);
        }
        table
    }
}
fn display(value: &Value) -> String {
    value
        .as_str()
        .map(str::to_owned)
        .unwrap_or_else(|| value.to_string())
}

pub fn zec_to_zatoshis(value: &str) -> Result<u64, EvmError> {
    let (whole, fraction) = value.split_once('.').unwrap_or((value, ""));
    if whole.is_empty()
        || !whole.bytes().all(|b| b.is_ascii_digit())
        || fraction.len() > 8
        || !fraction.bytes().all(|b| b.is_ascii_digit())
    {
        return Err(EvmError::validation(
            "ZEC amount must be a nonnegative decimal with at most 8 fractional digits",
        ));
    }
    let whole = whole
        .parse::<u64>()
        .map_err(|_| EvmError::validation("ZEC amount exceeds the monetary range"))?;
    let fraction = format!("{fraction:0<8}")
        .parse::<u64>()
        .map_err(|_| EvmError::validation("Invalid ZEC fraction"))?;
    whole
        .checked_mul(100_000_000)
        .and_then(|w| w.checked_add(fraction))
        .filter(|amount| *amount <= 21_000_000 * 100_000_000)
        .ok_or_else(|| EvmError::validation("ZEC amount exceeds 21 million ZEC"))
}
pub fn format_zatoshis(value: u64) -> String {
    format!("{}.{:08}", value / 100_000_000, value % 100_000_000)
}

fn validate_hash(value: &str) -> Result<(), EvmError> {
    if value.len() == 64 && value.bytes().all(|b| b.is_ascii_hexdigit()) {
        Ok(())
    } else {
        Err(EvmError::validation(
            "Zcash hash must contain exactly 64 hexadecimal characters, without 0x",
        ))
    }
}
pub fn validate_transparent(address: &str, network: Network) -> Result<(), EvmError> {
    let decoded = bs58::decode(address).with_check(None).into_vec()
        .map_err(|_| EvmError::validation("Expected a checksum-valid transparent Zcash address; shielded balances require a wallet"))?;
    let prefixes: &[[u8; 2]] = match network {
        Network::Mainnet => &[[0x1c, 0xb8], [0x1c, 0xbd]],
        _ => &[[0x1d, 0x25], [0x1c, 0xba]],
    };
    if decoded.len() != 22 || !prefixes.iter().any(|p| decoded.starts_with(p)) {
        return Err(EvmError::validation(
            "Transparent address does not match the selected Zcash network",
        ));
    }
    Ok(())
}
fn integer(value: &Value) -> Result<u64, EvmError> {
    value
        .as_u64()
        .or_else(|| value.as_str().and_then(|s| s.parse().ok()))
        .ok_or_else(|| EvmError::rpc("Node returned a non-integer or negative zatoshi balance"))
}

fn health(
    info: &Value,
    block: &Value,
    max_tip_age_seconds: u64,
    max_lag_blocks: u64,
) -> Result<Value, EvmError> {
    let height = info["blocks"]
        .as_u64()
        .ok_or_else(|| EvmError::rpc("Node health: missing block height"))?;
    let target = info["headers"]
        .as_u64()
        .unwrap_or(height)
        .max(info["estimatedheight"].as_u64().unwrap_or(height));
    let block_height = block["height"]
        .as_u64()
        .ok_or_else(|| EvmError::rpc("Node health: missing tip block height"))?;
    if block_height.abs_diff(height) > 1 {
        return Err(EvmError::rpc(
            "Node health: inconsistent chain tip across batch responses",
        ));
    }
    let tip_time = block["time"]
        .as_u64()
        .ok_or_else(|| EvmError::rpc("Node health: missing tip timestamp"))?;
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_err(|_| EvmError::config("System clock precedes Unix epoch"))?
        .as_secs();
    let age = now.saturating_sub(tip_time);
    let lag = target.saturating_sub(height);
    if info["initialblockdownload"].as_bool() == Some(true)
        || info["verificationprogress"]
            .as_f64()
            .is_some_and(|p| p < 0.999)
        || lag > max_lag_blocks
    {
        return Err(EvmError::rpc(format!(
            "Node health: not synced (height {height}, estimated target {target}, lag {lag})"
        )));
    }
    if age > max_tip_age_seconds || tip_time > now.saturating_add(120) {
        return Err(EvmError::rpc(format!(
            "Node health: stale or future tip timestamp {tip_time} (current time {now})"
        )));
    }
    Ok(
        json!({"healthy":true,"height":height,"estimated_target":target,"lag_blocks":lag,"tip_age_seconds":age,"tip_hash":block["hash"],"consensus":info["consensus"]}),
    )
}

pub async fn run(args: &ZcashArgs, global_rpc: Option<&str>) -> Result<ResultData, EvmError> {
    let start = Instant::now();
    let offline = match &args.command {
        Command::Amount { zec } => Some(
            json!({"zec":format_zatoshis(zec_to_zatoshis(zec)?), "zatoshis":zec_to_zatoshis(zec)?.to_string()}),
        ),
        Command::Fee { actions } => {
            let fee = u64::from((*actions).max(2)) * 5000;
            Some(
                json!({"logical_actions":actions,"fee_zatoshis":fee.to_string(),"fee_zec":format_zatoshis(fee),"policy":"ZIP-317 conventional fee; not a mempool inclusion guarantee"}),
            )
        }
        _ => None,
    };
    if let Some(result) = offline {
        return Ok(ResultData {
            network: args.zcash_network.chain(),
            rpc_endpoint: None,
            elapsed_ms: start.elapsed().as_millis(),
            result,
        });
    }
    // Validate inputs before opening credential files or contacting a node.
    let requests = match &args.command {
        Command::Info | Command::Bench { .. } => {
            vec![ReadRequest::new("getblockchaininfo", json!([]))]
        }
        Command::Health { .. } => vec![
            ReadRequest::new("getblockchaininfo", json!([])),
            ReadRequest::new("getblock", json!(["-1", 1])),
        ],
        Command::Balance { address } | Command::Utxos { address } => {
            validate_transparent(address, args.zcash_network)?;
            let utxos = matches!(args.command, Command::Utxos { .. });
            vec![ReadRequest::new(
                if utxos {
                    "getaddressutxos"
                } else {
                    "getaddressbalance"
                },
                if utxos {
                    json!([{"addresses":[address], "chainInfo":true}])
                } else {
                    json!([{"addresses":[address]}])
                },
            )]
        }
        Command::Tx { txid, block_hash } => {
            validate_hash(txid)?;
            let mut params = vec![json!(txid), json!(1)];
            if let Some(hash) = block_hash {
                validate_hash(hash)?;
                params.push(json!(hash));
            }
            vec![ReadRequest::new("getrawtransaction", json!(params))]
        }
        Command::Block { id, full } => {
            if id != "latest" && id.parse::<u32>().is_err() {
                validate_hash(id)?;
            }
            // Negative heights are documented by zcashd and Zebra; -1 denotes the current tip.
            vec![ReadRequest::new(
                "getblock",
                json!([
                    if id == "latest" { "-1" } else { id },
                    if *full { 2 } else { 1 }
                ]),
            )]
        }
        Command::Mempool => vec![ReadRequest::new("getmempoolinfo", json!([]))],
        Command::Batch { file } => {
            let bytes = tokio::fs::read(file)
                .await
                .map_err(|_| EvmError::validation("Cannot read Zcash batch file"))?;
            if bytes.len() > 1_048_576 {
                return Err(EvmError::validation("Zcash batch file exceeds 1 MiB"));
            }
            serde_json::from_slice::<Vec<ReadRequest>>(&bytes).map_err(|_| {
                EvmError::validation(
                    "Batch file must contain an array of {method, params} requests",
                )
            })?
        }
        Command::Amount { .. } | Command::Fee { .. } => unreachable!(),
    };
    if requests.is_empty() || requests.len() > 100 {
        return Err(EvmError::validation("Zcash batches require 1-100 requests"));
    }
    for request in &requests {
        request.validate()?;
    }
    let auth = if let Some(file) = &args.cookie_file {
        let cookie = tokio::fs::read_to_string(file)
            .await
            .map_err(|_| EvmError::config("Cannot read Zcash RPC cookie file"))?;
        let (user, password) = cookie
            .trim()
            .split_once(':')
            .ok_or_else(|| EvmError::config("Zcash cookie must contain user:password"))?;
        Some((user.into(), password.into()))
    } else {
        match (
            std::env::var("ONCHAIN_ZCASH_RPC_USER").ok(),
            std::env::var("ONCHAIN_ZCASH_RPC_PASSWORD").ok(),
        ) {
            (Some(user), Some(password)) => Some((user, password)),
            (None, None) => None,
            _ => {
                return Err(EvmError::config(
                    "Set both ONCHAIN_ZCASH_RPC_USER and ONCHAIN_ZCASH_RPC_PASSWORD",
                ))
            }
        }
    };
    let url = args
        .zcash_rpc_url
        .as_deref()
        .or(global_rpc)
        .unwrap_or(args.zcash_network.default_url());
    let client = Client::new(url, args.zcash_network.chain(), args.timeout_ms, auth)?;
    let result = if let Command::Bench { iterations } = args.command {
        let mut samples = Vec::new();
        let mut errors = Vec::new();
        for _ in 0..iterations {
            let began = Instant::now();
            match client.read(&requests).await {
                Ok(_) => samples.push(began.elapsed().as_secs_f64() * 1000.0),
                Err(e) => errors.push(e.to_string()),
            }
        }
        if samples.is_empty() {
            return Err(EvmError::rpc(format!(
                "All {iterations} Zcash benchmark requests failed: {}",
                errors[0]
            )));
        }
        let cold_ms = samples[0];
        samples.sort_by(f64::total_cmp);
        json!({"attempted":iterations,"successful":samples.len(),"failed":errors.len(),"first_success_ms":cold_ms,"min_ms":samples[0],"p50_ms":samples[(samples.len()-1)/2],"p95_ms":samples[(samples.len()*95).div_ceil(100)-1],"mean_ms":samples.iter().sum::<f64>()/samples.len() as f64,"errors":errors})
    } else {
        let results = client.read(&requests).await?;
        match &args.command {
            Command::Health {
                max_tip_age_seconds,
                max_lag_blocks,
            } => health(
                &results[0],
                &results[1],
                *max_tip_age_seconds,
                *max_lag_blocks,
            )?,
            Command::Balance { address } => {
                let raw = &results[0];
                let balance = integer(&raw["balance"])?;
                let received = integer(&raw["received"])?;
                json!({"address":address,"pool":"transparent","balance_zatoshis":balance.to_string(),"balance_zec":format_zatoshis(balance),"received_zatoshis":received.to_string(),"received_zec":format_zatoshis(received)})
            }
            Command::Batch { .. } => json!(requests
                .iter()
                .zip(results)
                .map(|(r, result)| json!({"method":r.method,"result":result}))
                .collect::<Vec<_>>()),
            _ => results.into_iter().next().unwrap(),
        }
    };
    Ok(ResultData {
        network: args.zcash_network.chain(),
        rpc_endpoint: Some(client.endpoint()),
        elapsed_ms: start.elapsed().as_millis(),
        result,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn health_rejects_stale_unsynced_and_inconsistent_nodes() {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs();
        let info = json!({"blocks":100,"headers":100,"verificationprogress":1});
        let block = json!({"height":100,"time":now-10,"hash":"tip"});
        assert!(health(&info, &block, 600, 2).is_ok());
        assert!(health(&info, &json!({"height":100,"time":now-700}), 600, 2).is_err());
        assert!(health(&json!({"blocks":100,"headers":105}), &block, 600, 2).is_err());
        assert!(health(
            &json!({"blocks":100,"headers":100,"initialblockdownload":true}),
            &block,
            600,
            2
        )
        .is_err());
        assert!(health(&info, &json!({"height":90,"time":now}), 600, 2).is_err());
    }
    #[test]
    fn exact_amounts_never_round() {
        assert_eq!(zec_to_zatoshis("0.00000001").unwrap(), 1);
        assert_eq!(zec_to_zatoshis("21000000").unwrap(), 2_100_000_000_000_000);
        assert_eq!(
            format_zatoshis(zec_to_zatoshis("1.23456789").unwrap()),
            "1.23456789"
        );
        for bad in [
            "-1",
            "+1",
            "1e-8",
            "NaN",
            "0.000000001",
            "21000000.00000001",
            "18446744073709551616",
            "1.2.3",
            "",
        ] {
            assert!(zec_to_zatoshis(bad).is_err(), "{bad}");
        }
    }
    #[test]
    fn transparent_address_checksum_and_network_are_checked() {
        let mut payload = vec![0x1c, 0xb8];
        payload.extend([1; 20]);
        let address = bs58::encode(payload).with_check().into_string();
        assert!(validate_transparent(&address, Network::Mainnet).is_ok());
        assert!(validate_transparent(&address, Network::Testnet).is_err());
        assert!(validate_transparent("u1shielded", Network::Mainnet).is_err());
        assert!(validate_transparent(&(address + "x"), Network::Mainnet).is_err());
    }
    #[test]
    fn batches_cannot_mutate_or_export_secrets() {
        for method in [
            "sendrawtransaction",
            "z_sendmany",
            "stop",
            "dumpprivkey",
            "z_exportwallet",
        ] {
            assert!(ReadRequest::new(method, json!([])).validate().is_err());
        }
    }
}
