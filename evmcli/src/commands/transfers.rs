use comfy_table::Table;
use serde::{Deserialize, Serialize};

use crate::context::AppContext;
use crate::errors::EvmError;
use crate::output::table::Tableable;

#[derive(Debug, Deserialize)]
struct BlockscoutTransfersResponse {
    items: Vec<BlockscoutTransfer>,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
struct BlockscoutTransfer {
    #[serde(default)]
    transaction_hash: Option<String>,
    #[serde(default)]
    block_number: Option<u64>,
    #[serde(default)]
    timestamp: Option<String>,
    from: Option<TransferAddr>,
    to: Option<TransferAddr>,
    total: Option<TransferTotal>,
    token: Option<TransferToken>,
    #[serde(rename = "type")]
    transfer_type: Option<String>,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
struct TransferAddr {
    hash: String,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
struct TransferTotal {
    value: Option<String>,
    decimals: Option<String>,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
struct TransferToken {
    name: Option<String>,
    symbol: Option<String>,
    address_hash: Option<String>,
    #[serde(rename = "type")]
    token_type: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct TransfersResult {
    pub address: String,
    pub transfer_count: usize,
    pub transfers: Vec<TransferSummary>,
    pub explorer_url: String,
}

#[derive(Debug, Serialize)]
pub struct TransferSummary {
    pub tx_hash: String,
    pub block: Option<u64>,
    pub timestamp: Option<String>,
    pub from: String,
    pub to: String,
    pub value: String,
    pub token_symbol: String,
    pub token_address: String,
    pub direction: String,
}

impl Tableable for TransfersResult {
    fn to_table(&self) -> Table {
        let mut table = Table::new();
        table.set_header(vec!["Dir", "Token", "Value", "From", "To", "TX"]);
        for t in &self.transfers {
            let from_short = if t.from.len() > 14 {
                format!("{}...{}", &t.from[..8], &t.from[t.from.len()-4..])
            } else { t.from.clone() };
            let to_short = if t.to.len() > 14 {
                format!("{}...{}", &t.to[..8], &t.to[t.to.len()-4..])
            } else { t.to.clone() };
            let tx_short = if t.tx_hash.len() > 14 {
                format!("{}...{}", &t.tx_hash[..8], &t.tx_hash[t.tx_hash.len()-4..])
            } else { t.tx_hash.clone() };
            table.add_row(vec![
                &t.direction,
                &t.token_symbol,
                &t.value,
                &from_short,
                &to_short,
                &tx_short,
            ]);
        }
        table.add_row(vec![&format!("{} transfers", self.transfer_count), "", "", "", "", ""]);
        table
    }
}

fn format_token_value(raw: &str, decimals_str: &str) -> String {
    let decimals: u32 = decimals_str.parse().unwrap_or(18);
    if decimals == 0 || raw.is_empty() { return raw.to_string(); }

    // Simple decimal formatting
    let raw_len = raw.len();
    if raw_len <= decimals as usize {
        let padded = format!("{:0>width$}", raw, width = decimals as usize + 1);
        let (whole, frac) = padded.split_at(padded.len() - decimals as usize);
        let trimmed = frac.trim_end_matches('0');
        if trimmed.is_empty() {
            whole.to_string()
        } else {
            format!("{whole}.{trimmed}")
        }
    } else {
        let (whole, frac) = raw.split_at(raw_len - decimals as usize);
        let trimmed = frac.trim_end_matches('0');
        if trimmed.is_empty() {
            whole.to_string()
        } else {
            format!("{whole}.{trimmed}")
        }
    }
}

pub async fn run(ctx: &AppContext, address: &str, _token_type: &str) -> Result<TransfersResult, EvmError> {
    crate::errors::validate_address(address)?;
    let url = format!("{}/addresses/{}/token-transfers",
        ctx.chain.explorer_v2_url(), address);

    let resp = ctx.http.get(&url).send().await
        .map_err(|e| EvmError::explorer(format!("Blockscout request failed: {e}")))?;

    if !resp.status().is_success() {
        return Err(EvmError::explorer(format!("Blockscout returned {}", resp.status())));
    }

    let data: BlockscoutTransfersResponse = resp.json().await
        .map_err(|e| EvmError::explorer(format!("Failed to parse transfers response: {e}")))?;

    let addr_lower = address.to_lowercase();

    let transfers: Vec<TransferSummary> = data.items.iter().map(|t| {
        let from = t.from.as_ref().map(|a| a.hash.clone()).unwrap_or_default();
        let to = t.to.as_ref().map(|a| a.hash.clone()).unwrap_or_default();
        let direction = if from.to_lowercase() == addr_lower { "OUT" }
            else if to.to_lowercase() == addr_lower { "IN" }
            else { "???" };

        let value = t.total.as_ref()
            .and_then(|total| {
                let raw = total.value.as_deref().unwrap_or("0");
                let dec = total.decimals.as_deref().unwrap_or("18");
                Some(format_token_value(raw, dec))
            })
            .unwrap_or_else(|| "?".to_string());

        let token_symbol = t.token.as_ref()
            .and_then(|tk| tk.symbol.clone())
            .unwrap_or_else(|| "???".to_string());

        let token_address = t.token.as_ref()
            .and_then(|tk| tk.address_hash.clone())
            .unwrap_or_default();

        TransferSummary {
            tx_hash: t.transaction_hash.clone().unwrap_or_default(),
            block: t.block_number,
            timestamp: t.timestamp.clone(),
            from,
            to,
            value,
            token_symbol,
            token_address,
            direction: direction.to_string(),
        }
    }).collect();

    Ok(TransfersResult {
        address: address.to_string(),
        transfer_count: transfers.len(),
        transfers,
        explorer_url: url,
    })
}
