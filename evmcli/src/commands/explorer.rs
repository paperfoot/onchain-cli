use comfy_table::Table;
use serde::{Deserialize, Serialize};

use crate::context::AppContext;
use crate::errors::EvmError;
use crate::output::table::Tableable;

#[derive(Debug, Deserialize)]
struct BlockscoutResponse {
    items: Vec<BlockscoutTx>,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
struct BlockscoutTx {
    hash: String,
    #[serde(default)]
    block: Option<u64>,
    timestamp: Option<String>,
    from: Option<BlockscoutAddr>,
    to: Option<BlockscoutAddr>,
    value: Option<String>,
    status: Option<String>,
    result: Option<String>,
    #[serde(default)]
    gas_used: Option<String>,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
struct BlockscoutAddr {
    hash: String,
}

#[derive(Debug, Serialize)]
pub struct ExplorerResult {
    pub address: String,
    pub tx_count: usize,
    pub transactions: Vec<TxSummary>,
    pub explorer_url: String,
}

#[derive(Debug, Serialize)]
pub struct TxSummary {
    pub hash: String,
    pub block: Option<u64>,
    pub timestamp: Option<String>,
    pub from: String,
    pub to: String,
    pub status: String,
}

impl Tableable for ExplorerResult {
    fn to_table(&self) -> Table {
        let mut table = Table::new();
        table.set_header(vec!["Hash", "Block", "From", "To", "Status"]);
        for tx in &self.transactions {
            let hash_short = if tx.hash.len() > 14 {
                format!("{}...{}", &tx.hash[..8], &tx.hash[tx.hash.len()-6..])
            } else {
                tx.hash.clone()
            };
            let from_short = if tx.from.len() > 14 {
                format!("{}...{}", &tx.from[..8], &tx.from[tx.from.len()-4..])
            } else {
                tx.from.clone()
            };
            let to_short = if tx.to.len() > 14 {
                format!("{}...{}", &tx.to[..8], &tx.to[tx.to.len()-4..])
            } else {
                tx.to.clone()
            };
            table.add_row(vec![
                &hash_short,
                &tx.block.map(|b| b.to_string()).unwrap_or("-".into()),
                &from_short,
                &to_short,
                &tx.status,
            ]);
        }
        table
    }
}

pub async fn run(ctx: &AppContext, address: &str) -> Result<ExplorerResult, EvmError> {
    crate::errors::validate_address(address)?;
    let url = format!("{}/addresses/{}/transactions",
        ctx.chain.explorer_v2_url(), address);

    let resp = ctx.http.get(&url).send().await
        .map_err(|e| EvmError::explorer(format!("Blockscout request failed: {e}")))?;

    if !resp.status().is_success() {
        return Err(EvmError::explorer(format!("Blockscout returned {}", resp.status())));
    }

    let data: BlockscoutResponse = resp.json().await
        .map_err(|e| EvmError::explorer(format!("Failed to parse Blockscout response: {e}")))?;

    let transactions: Vec<TxSummary> = data.items.iter().map(|tx| {
        TxSummary {
            hash: tx.hash.clone(),
            block: tx.block,
            timestamp: tx.timestamp.clone(),
            from: tx.from.as_ref().map(|a| a.hash.clone()).unwrap_or_default(),
            to: tx.to.as_ref().map(|a| a.hash.clone()).unwrap_or("(create)".into()),
            status: tx.status.clone().or(tx.result.clone()).unwrap_or("unknown".into()),
        }
    }).collect();

    Ok(ExplorerResult {
        address: address.to_string(),
        tx_count: transactions.len(),
        transactions,
        explorer_url: url,
    })
}
