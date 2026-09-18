use alloy::primitives::{Address, U256};
use alloy::providers::Provider;
use comfy_table::Table;
use serde::Serialize;

use crate::context::AppContext;
use crate::errors::EvmError;
use crate::output::table::Tableable;

#[derive(Debug, Serialize)]
pub struct StorageResult {
    pub address: String,
    pub slot: String,
    pub value: String,
    pub value_decimal: String,
    pub block: Option<u64>,
    pub rpc_endpoint: String,
}

impl Tableable for StorageResult {
    fn to_table(&self) -> Table {
        let mut table = Table::new();
        table.add_row(vec!["Address", &self.address]);
        table.add_row(vec!["Slot", &self.slot]);
        table.add_row(vec!["Value (hex)", &self.value]);
        table.add_row(vec!["Value (dec)", &self.value_decimal]);
        if let Some(b) = self.block {
            table.add_row(vec!["Block", &b.to_string()]);
        }
        table.add_row(vec!["RPC", &self.rpc_endpoint]);
        table
    }
}

pub async fn run(
    ctx: &AppContext,
    address: &str,
    slot: &str,
    block: Option<u64>,
) -> Result<StorageResult, EvmError> {
    let addr: Address = address
        .parse()
        .map_err(|_| EvmError::validation(format!("Invalid address: {address}")))?;

    let slot_u256: U256 = if let Some(hex) = slot.strip_prefix("0x") {
        U256::from_str_radix(hex, 16)
            .map_err(|_| EvmError::validation(format!("Invalid slot: {slot}")))?
    } else {
        slot.parse::<U256>()
            .map_err(|_| EvmError::validation(format!("Invalid slot: {slot}")))?
    };

    let value = ctx
        .provider
        .get_storage_at(addr, slot_u256)
        .block_id(block.map(alloy::eips::BlockId::number).unwrap_or_default())
        .await
        .map_err(|e| EvmError::rpc(format!("get_storage_at failed: {e}")))?;

    Ok(StorageResult {
        address: format!("{addr}"),
        slot: format!("{slot_u256:#x}"),
        value: format!("{value:#066x}"),
        value_decimal: value.to_string(),
        block,
        rpc_endpoint: crate::rpc::provider::endpoint_label(&ctx.rpc_url),
    })
}
