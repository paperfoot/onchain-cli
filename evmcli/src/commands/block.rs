use alloy::eips::BlockNumberOrTag;
use alloy::providers::Provider;
use comfy_table::Table;
use serde::Serialize;

use crate::context::AppContext;
use crate::errors::EvmError;
use crate::output::table::Tableable;

#[derive(Debug, Serialize)]
pub struct BlockResult {
    pub number: u64,
    pub hash: String,
    pub timestamp: u64,
    pub gas_used: String,
    pub gas_limit: String,
    pub base_fee: Option<String>,
    pub tx_count: usize,
    pub rpc_endpoint: String,
}

impl Tableable for BlockResult {
    fn to_table(&self) -> Table {
        let mut table = Table::new();
        table.add_row(vec!["Number", &self.number.to_string()]);
        table.add_row(vec!["Hash", &self.hash]);
        table.add_row(vec!["Timestamp", &self.timestamp.to_string()]);
        table.add_row(vec!["Gas Used", &self.gas_used]);
        table.add_row(vec!["Gas Limit", &self.gas_limit]);
        if let Some(ref fee) = self.base_fee {
            table.add_row(vec!["Base Fee", fee]);
        }
        table.add_row(vec!["Transactions", &self.tx_count.to_string()]);
        table.add_row(vec!["RPC", &self.rpc_endpoint]);
        table
    }
}

pub async fn run(ctx: &AppContext, id: &str) -> Result<BlockResult, EvmError> {
    let id_parsed = parse_block_id(id)?;
    let block = ctx.provider.get_block(id_parsed).await;

    let block = block
        .map_err(|e| EvmError::rpc(format!("get_block failed: {e}")))?
        .ok_or_else(|| EvmError::rpc(format!("Block not found: {id}")))?;

    let header = &block.header;

    Ok(BlockResult {
        number: header.number,
        hash: format!("{}", header.hash),
        timestamp: header.timestamp,
        gas_used: header.gas_used.to_string(),
        gas_limit: header.gas_limit.to_string(),
        base_fee: header.base_fee_per_gas.map(|f| format!("{f}")),
        tx_count: block.transactions.len(),
        rpc_endpoint: crate::rpc::provider::endpoint_label(&ctx.rpc_url),
    })
}

pub fn parse_block_id(id: &str) -> Result<alloy::eips::BlockId, EvmError> {
    if id.starts_with("0x") && id.len() == 66 {
        return id
            .parse::<alloy::primitives::B256>()
            .map(Into::into)
            .map_err(|_| EvmError::validation("Invalid block hash"));
    }
    if let Ok(number) = id.parse::<u64>() {
        return Ok(alloy::eips::BlockId::number(number));
    }
    id.parse::<BlockNumberOrTag>().map(Into::into)
        .map_err(|_| EvmError::validation("Invalid block: use a decimal/hex number, hash, latest, earliest, pending, safe or finalized"))
}
