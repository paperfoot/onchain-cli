use alloy::primitives::B256;
use comfy_table::Table;
use serde::{Deserialize, Serialize};

use crate::context::AppContext;
use crate::errors::EvmError;
use crate::output::table::Tableable;

#[derive(Debug, Serialize)]
pub struct TraceResult {
    pub hash: String,
    pub call_count: usize,
    pub calls: Vec<TraceCall>,
    pub rpc_endpoint: String,
}

#[derive(Debug, Serialize, Clone)]
pub struct TraceCall {
    pub depth: usize,
    pub call_type: String,
    pub from: String,
    pub to: String,
    pub value: String,
    pub gas_used: String,
    pub input_size: usize,
    pub output_size: usize,
    pub error: Option<String>,
}

impl Tableable for TraceResult {
    fn to_table(&self) -> Table {
        let mut table = Table::new();
        table.set_header(vec!["Depth", "Type", "From", "To", "Value", "Gas", "Error"]);
        for call in &self.calls {
            let indent = "  ".repeat(call.depth);
            let from_short = if call.from.len() > 14 {
                format!("{}...{}", &call.from[..8], &call.from[call.from.len()-4..])
            } else { call.from.clone() };
            let to_short = if call.to.len() > 14 {
                format!("{}...{}", &call.to[..8], &call.to[call.to.len()-4..])
            } else { call.to.clone() };
            table.add_row(vec![
                &format!("{indent}{}", call.depth),
                &call.call_type,
                &from_short,
                &to_short,
                &call.value,
                &call.gas_used,
                call.error.as_deref().unwrap_or(""),
            ]);
        }
        table.add_row(vec![&format!("{} calls", self.call_count), "", "", "", "", "", ""]);
        table
    }
}

// The debug_traceTransaction response structure
#[derive(Debug, Deserialize)]
struct DebugTraceResponse {
    result: Option<TraceFrame>,
}

#[derive(Debug, Deserialize)]
struct TraceFrame {
    #[serde(rename = "type")]
    call_type: Option<String>,
    from: Option<String>,
    to: Option<String>,
    value: Option<String>,
    gas: Option<String>,
    #[serde(rename = "gasUsed")]
    gas_used: Option<String>,
    input: Option<String>,
    output: Option<String>,
    error: Option<String>,
    calls: Option<Vec<TraceFrame>>,
}

fn flatten_calls(frame: &TraceFrame, depth: usize, result: &mut Vec<TraceCall>) {
    result.push(TraceCall {
        depth,
        call_type: frame.call_type.clone().unwrap_or_else(|| "CALL".to_string()),
        from: frame.from.clone().unwrap_or_default(),
        to: frame.to.clone().unwrap_or_default(),
        value: frame.value.clone().unwrap_or_else(|| "0x0".to_string()),
        gas_used: frame.gas_used.clone().unwrap_or_else(|| "0".to_string()),
        input_size: frame.input.as_ref().map(|i| (i.len().saturating_sub(2)) / 2).unwrap_or(0),
        output_size: frame.output.as_ref().map(|o| (o.len().saturating_sub(2)) / 2).unwrap_or(0),
        error: frame.error.clone(),
    });

    if let Some(ref subcalls) = frame.calls {
        for subcall in subcalls {
            flatten_calls(subcall, depth + 1, result);
        }
    }
}

pub async fn run(ctx: &AppContext, hash: &str) -> Result<TraceResult, EvmError> {
    let _tx_hash: B256 = hash.parse()
        .map_err(|_| EvmError::validation(format!("Invalid tx hash: {hash}")))?;

    // Trace fallback chain:
    // 1. Local node (fastest, ~128 blocks of state via SSH tunnel)
    // 2. Alchemy archive (full history, any TX ever)
    // Skip public RPCs entirely — they never have debug API.
    let local_rpc = ctx.chain.local_rpc.to_string();
    let alchemy_rpc = std::env::var("ALCHEMY_ARB_RPC").ok();

    // If user passed --rpc-url explicitly, try that first (they know what they're doing)
    if ctx.rpc_url != ctx.chain.public_rpc {
        if let Ok((trace_resp, used_rpc)) = try_trace(&ctx.http, &ctx.rpc_url, hash).await {
            return parse_trace_response(trace_resp, hash, &used_rpc);
        }
    }

    // Try local node first (fast, but limited to recent TXs)
    eprintln!("Trying local node at {}...", local_rpc);
    match try_trace(&ctx.http, &local_rpc, hash).await {
        Ok((trace_resp, used_rpc)) => {
            return parse_trace_response(trace_resp, hash, &used_rpc);
        }
        Err(e) => {
            let msg = e.to_string();
            if msg.contains("too old") || msg.contains("reexec") {
                eprintln!("  TX too old for local node (~128 blocks). Falling back to Alchemy archive...");
            } else {
                eprintln!("  Local node not reachable. Trying Alchemy archive...");
            }
        }
    }

    // Fall back to Alchemy (full archive — handles any TX)
    if let Some(ref alch_url) = alchemy_rpc {
        match try_trace(&ctx.http, alch_url, hash).await {
            Ok((trace_resp, used_rpc)) => {
                eprintln!("  Traced via Alchemy archive");
                return parse_trace_response(trace_resp, hash, &used_rpc);
            }
            Err(e) => {
                eprintln!("  Alchemy failed: {e}");
            }
        }
    }

    // All failed
    let mut hints = vec![];
    if alchemy_rpc.is_none() {
        hints.push("Set ALCHEMY_ARB_RPC env var (full archive trace for any TX)");
    }
    hints.push("Run 'ssh -fN mev' to start SSH tunnel (fast trace for recent TXs)");

    Err(EvmError::Rpc {
        code: "rpc.trace_failed",
        message: format!(
            "Could not trace TX. Tried local node + Alchemy.\n\nFix:\n{}",
            hints.iter().map(|h| format!("  - {h}")).collect::<Vec<_>>().join("\n")
        ),
    })
}

async fn try_trace(http: &reqwest::Client, rpc_url: &str, hash: &str) -> Result<(serde_json::Value, String), EvmError> {
    let body = serde_json::json!({
        "jsonrpc": "2.0",
        "method": "debug_traceTransaction",
        "params": [hash, {"tracer": "callTracer", "tracerConfig": {"onlyTopCall": false}}],
        "id": 1
    });

    let resp = http.post(rpc_url)
        .json(&body)
        .send()
        .await
        .map_err(|e| EvmError::rpc(format!("trace request to {} failed: {e}", rpc_url)))?;

    if !resp.status().is_success() {
        return Err(EvmError::rpc(format!("trace returned HTTP {}", resp.status())));
    }

    let trace_resp: serde_json::Value = resp.json().await
        .map_err(|e| EvmError::rpc(format!("Failed to parse trace response: {e}")))?;

    if trace_resp.get("error").is_some() {
        let msg = trace_resp["error"]["message"].as_str().unwrap_or("unsupported");
        if msg.contains("historical state unavailable") || msg.contains("reexec") {
            return Err(EvmError::Rpc {
                code: "rpc.state_too_old",
                message: format!(
                    "TX is too old to trace — node only keeps ~128 blocks of state. \
                     Trace works for recent TXs (last ~2 min on Arbitrum). \
                     For older TXs, the node needs --gcmode=archive or higher --init.reexec value."
                ),
            });
        }
        return Err(EvmError::rpc(msg.to_string()));
    }

    Ok((trace_resp, rpc_url.to_string()))
}

fn parse_trace_response(trace_resp: serde_json::Value, hash: &str, rpc_url: &str) -> Result<TraceResult, EvmError> {

    let frame: TraceFrame = serde_json::from_value(
        trace_resp.get("result").cloned().unwrap_or(serde_json::Value::Null)
    ).map_err(|e| EvmError::rpc(format!("Failed to parse trace frame: {e}")))?;

    let mut calls = Vec::new();
    flatten_calls(&frame, 0, &mut calls);

    Ok(TraceResult {
        hash: hash.to_string(),
        call_count: calls.len(),
        calls,
        rpc_endpoint: rpc_url.to_string(),
    })
}
