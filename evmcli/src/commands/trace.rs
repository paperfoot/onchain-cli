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
                format!(
                    "{}...{}",
                    &call.from[..8],
                    &call.from[call.from.len() - 4..]
                )
            } else {
                call.from.clone()
            };
            let to_short = if call.to.len() > 14 {
                format!("{}...{}", &call.to[..8], &call.to[call.to.len() - 4..])
            } else {
                call.to.clone()
            };
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
        table.add_row(vec![
            &format!("{} calls", self.call_count),
            "",
            "",
            "",
            "",
            "",
            "",
        ]);
        table
    }
}

#[derive(Debug, Deserialize)]
struct TraceFrame {
    #[serde(rename = "type")]
    call_type: Option<String>,
    from: Option<String>,
    to: Option<String>,
    value: Option<String>,
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
        call_type: frame
            .call_type
            .clone()
            .unwrap_or_else(|| "CALL".to_string()),
        from: frame.from.clone().unwrap_or_default(),
        to: frame.to.clone().unwrap_or_default(),
        value: frame.value.clone().unwrap_or_else(|| "0x0".to_string()),
        gas_used: frame.gas_used.clone().unwrap_or_else(|| "0".to_string()),
        input_size: frame
            .input
            .as_ref()
            .map(|i| (i.len().saturating_sub(2)) / 2)
            .unwrap_or(0),
        output_size: frame
            .output
            .as_ref()
            .map(|o| (o.len().saturating_sub(2)) / 2)
            .unwrap_or(0),
        error: frame.error.clone(),
    });

    if let Some(ref subcalls) = frame.calls {
        for subcall in subcalls {
            flatten_calls(subcall, depth + 1, result);
        }
    }
}

pub async fn run(ctx: &AppContext, hash: &str) -> Result<TraceResult, EvmError> {
    let _tx_hash: B256 = hash
        .parse()
        .map_err(|_| EvmError::validation(format!("Invalid tx hash: {hash}")))?;

    let mut endpoints = vec![ctx.rpc_url.clone()];
    if !ctx.rpc_explicit {
        let legacy_var = match ctx.chain.chain_id {
            42161 => "ALCHEMY_ARB_RPC",
            1 => "ALCHEMY_ETH_RPC",
            8453 => "ALCHEMY_BASE_RPC",
            10 => "ALCHEMY_OP_RPC",
            137 => "ALCHEMY_POLYGON_RPC",
            _ => "",
        };
        let archive = std::env::var("ONCHAIN_TRACE_RPC_URL")
            .ok()
            .or_else(|| std::env::var(legacy_var).ok());
        if let Some(url) = archive {
            endpoints.insert(0, url);
        }
        endpoints.push(ctx.chain.local_rpc.to_string());
    }
    let mut seen = std::collections::HashSet::new();
    let mut errors = Vec::new();
    for endpoint in endpoints {
        if !seen.insert(endpoint.clone()) {
            continue;
        }
        let label = crate::rpc::provider::endpoint_label(&endpoint);
        let attempt = async {
            crate::rpc::provider::validate_url(&endpoint)?;
            crate::rpc::detect::probe_rpc(&ctx.http, &endpoint, ctx.chain.chain_id, 2000).await?;
            let (response, _) = try_trace(&ctx.http, &endpoint, hash).await?;
            parse_trace_response(response, hash, &endpoint)
        }
        .await;
        match attempt {
            Ok(result) => return Ok(result),
            Err(error) => errors.push(format!("{label}: {error}")),
        }
    }
    Err(EvmError::Rpc { code: "rpc.trace_failed", message: format!(
        "Trace failed. Use --rpc-url or ONCHAIN_TRACE_RPC_URL with debug_traceTransaction support for this network. {}", errors.join("; ")) })
}

async fn try_trace(
    http: &reqwest::Client,
    rpc_url: &str,
    hash: &str,
) -> Result<(serde_json::Value, String), EvmError> {
    let body = serde_json::json!({
        "jsonrpc": "2.0",
        "method": "debug_traceTransaction",
        "params": [hash, {"tracer": "callTracer", "tracerConfig": {"onlyTopCall": false}}],
        "id": 1
    });

    let resp = http
        .post(rpc_url)
        .json(&body)
        .send()
        .await
        .map_err(|e| EvmError::rpc(format!("trace request failed: {}", e.without_url())))?;

    if !resp.status().is_success() {
        return Err(EvmError::rpc(format!(
            "trace returned HTTP {}",
            resp.status()
        )));
    }

    let trace_resp: serde_json::Value = resp
        .json()
        .await
        .map_err(|e| EvmError::rpc(format!("Failed to parse trace response: {e}")))?;

    if trace_resp.get("error").is_some() {
        let msg = trace_resp["error"]["message"]
            .as_str()
            .unwrap_or("unsupported");
        return Err(EvmError::rpc(msg.to_string()));
    }

    Ok((trace_resp, rpc_url.to_string()))
}

fn parse_trace_response(
    trace_resp: serde_json::Value,
    hash: &str,
    rpc_url: &str,
) -> Result<TraceResult, EvmError> {
    if trace_resp["result"]["type"].as_str().is_none() {
        return Err(EvmError::rpc(
            "Missing callTracer result; the node may not support this tracer",
        ));
    }
    let frame: TraceFrame = serde_json::from_value(
        trace_resp
            .get("result")
            .cloned()
            .unwrap_or(serde_json::Value::Null),
    )
    .map_err(|e| EvmError::rpc(format!("Failed to parse trace frame: {e}")))?;

    let mut calls = Vec::new();
    flatten_calls(&frame, 0, &mut calls);

    Ok(TraceResult {
        hash: hash.to_string(),
        call_count: calls.len(),
        calls,
        rpc_endpoint: crate::rpc::provider::endpoint_label(rpc_url),
    })
}
