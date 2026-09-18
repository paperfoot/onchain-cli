use comfy_table::Table;
use serde::{Deserialize, Serialize};

use crate::context::AppContext;
use crate::errors::EvmError;
use crate::output::table::Tableable;

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
    total: Option<serde_json::Value>,
    token: Option<TransferToken>,
    #[serde(rename = "type")]
    transfer_type: Option<String>,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
struct TransferAddr {
    hash: String,
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
    pub pages_fetched: u32,
    pub next_page_params: Option<serde_json::Value>,
}

#[derive(Debug, Serialize)]
pub struct TransferSummary {
    pub tx_hash: String,
    pub block: Option<u64>,
    pub timestamp: Option<String>,
    pub from: String,
    pub to: String,
    pub value: String,
    pub raw_value: Option<String>,
    pub decimals: Option<u8>,
    pub token_id: Option<String>,
    pub token_type: Option<String>,
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
                format!("{}...{}", &t.from[..8], &t.from[t.from.len() - 4..])
            } else {
                t.from.clone()
            };
            let to_short = if t.to.len() > 14 {
                format!("{}...{}", &t.to[..8], &t.to[t.to.len() - 4..])
            } else {
                t.to.clone()
            };
            let tx_short = if t.tx_hash.len() > 14 {
                format!(
                    "{}...{}",
                    &t.tx_hash[..8],
                    &t.tx_hash[t.tx_hash.len() - 4..]
                )
            } else {
                t.tx_hash.clone()
            };
            table.add_row(vec![
                &t.direction,
                &t.token_symbol,
                &t.value,
                &from_short,
                &to_short,
                &tx_short,
            ]);
        }
        table.add_row(vec![
            &format!("{} transfers", self.transfer_count),
            "",
            "",
            "",
            "",
            "",
        ]);
        table
    }
}

fn format_token_value(raw: &str, decimals: u8) -> String {
    if decimals == 0 || !raw.bytes().all(|b| b.is_ascii_digit()) {
        return raw.to_string();
    }
    let padded = format!("{:0>width$}", raw, width = decimals as usize + 1);
    let (whole, fraction) = padded.split_at(padded.len() - decimals as usize);
    let fraction = fraction.trim_end_matches('0');
    if fraction.is_empty() {
        whole.into()
    } else {
        format!("{whole}.{fraction}")
    }
}

pub async fn run(
    ctx: &AppContext,
    address: &str,
    token_type: &str,
    max_pages: u32,
) -> Result<TransfersResult, EvmError> {
    crate::errors::validate_address(address)?;
    let url = format!(
        "{}/addresses/{}/token-transfers",
        ctx.explorer_v2_url(),
        address
    );

    let kind = match token_type.to_ascii_lowercase().as_str() {
        "erc20" => "ERC-20",
        "erc721" => "ERC-721",
        "erc1155" => "ERC-1155",
        _ => {
            return Err(EvmError::validation(
                "Token type must be erc20, erc721 or erc1155",
            ))
        }
    };
    let (page, pages_fetched) =
        crate::explorer::pages(&ctx.http, &url, &[("type", kind)], max_pages).await?;
    let items: Vec<BlockscoutTransfer> = page
        .items
        .into_iter()
        .map(serde_json::from_value)
        .collect::<Result<_, _>>()
        .map_err(|e| EvmError::explorer(format!("Invalid token transfer: {e}")))?;
    let addr_lower = address.to_lowercase();

    let transfers: Vec<TransferSummary> = items
        .iter()
        .flat_map(|t| {
            let totals = match &t.total {
                Some(serde_json::Value::Array(totals)) => totals.clone(),
                Some(total) => vec![total.clone()],
                None => vec![serde_json::Value::Null],
            };
            totals
                .into_iter()
                .map(|total| {
                    let from = t.from.as_ref().map(|a| a.hash.clone()).unwrap_or_default();
                    let to = t.to.as_ref().map(|a| a.hash.clone()).unwrap_or_default();
                    let direction = if from.to_lowercase() == addr_lower {
                        "OUT"
                    } else if to.to_lowercase() == addr_lower {
                        "IN"
                    } else {
                        "???"
                    };

                    let token_type = t.token.as_ref().and_then(|tk| tk.token_type.clone());
                    let raw_value = total["value"]
                        .as_str()
                        .map(str::to_owned)
                        .or_else(|| (token_type.as_deref() == Some("ERC-721")).then(|| "1".into()));
                    let decimals = total["decimals"]
                        .as_str()
                        .and_then(|d| d.parse::<u8>().ok());
                    let value = raw_value
                        .as_deref()
                        .map(|raw| format_token_value(raw, decimals.unwrap_or(0)))
                        .unwrap_or_else(|| "?".into());
                    let token_id = total["token_id"].as_str().map(str::to_owned);

                    let token_symbol = t
                        .token
                        .as_ref()
                        .and_then(|tk| tk.symbol.clone())
                        .unwrap_or_else(|| "???".to_string());

                    let token_address = t
                        .token
                        .as_ref()
                        .and_then(|tk| tk.address_hash.clone())
                        .unwrap_or_default();

                    TransferSummary {
                        tx_hash: t.transaction_hash.clone().unwrap_or_default(),
                        block: t.block_number,
                        timestamp: t.timestamp.clone(),
                        from,
                        to,
                        value,
                        raw_value,
                        decimals,
                        token_id,
                        token_type,
                        token_symbol,
                        token_address,
                        direction: direction.to_string(),
                    }
                })
                .collect::<Vec<_>>()
        })
        .collect();

    Ok(TransfersResult {
        address: address.to_string(),
        transfer_count: transfers.len(),
        transfers,
        explorer_url: crate::rpc::provider::endpoint_label(&url),
        pages_fetched,
        next_page_params: page.next_page_params,
    })
}
