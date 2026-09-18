use alloy::providers::Provider;
use comfy_table::Table;
use serde::Serialize;

use crate::context::AppContext;
use crate::errors::EvmError;
use crate::output::table::Tableable;

#[derive(Debug, Serialize)]
pub struct GasResult {
    pub gas_price_gwei: f64,
    pub gas_price_wei: String,
    pub base_fee_wei: Option<String>,
    pub priority_fee_wei: Option<String>,
    pub priority_fee_gwei: Option<f64>,
    pub base_fee_gwei: Option<f64>,
    pub rpc_endpoint: String,
}

impl Tableable for GasResult {
    fn to_table(&self) -> Table {
        let mut table = Table::new();
        table.add_row(vec![
            "Gas Price",
            &format!("{:.9} gwei", self.gas_price_gwei),
        ]);
        if let Some(base) = self.base_fee_gwei {
            table.add_row(vec!["Base Fee", &format!("{:.9} gwei", base)]);
        }
        if let Some(priority) = self.priority_fee_gwei {
            table.add_row(vec!["Priority Fee", &format!("{priority:.9} gwei")]);
        }
        table.add_row(vec!["RPC", &self.rpc_endpoint]);
        table
    }
}

pub async fn run(ctx: &AppContext) -> Result<GasResult, EvmError> {
    let (gas_price, latest_block, priority_fee) = tokio::join!(
        ctx.provider.get_gas_price(),
        ctx.provider
            .get_block_by_number(alloy::eips::BlockNumberOrTag::Latest),
        ctx.provider.get_max_priority_fee_per_gas(),
    );
    let gas_price = gas_price.map_err(|e| EvmError::rpc(format!("get_gas_price failed: {e}")))?;
    let latest_block = latest_block.map_err(|e| EvmError::rpc(format!("get_block failed: {e}")))?;
    let priority_fee = priority_fee.ok();
    let base_fee = latest_block.and_then(|b| b.header.base_fee_per_gas);

    Ok(GasResult {
        gas_price_gwei: gas_price as f64 / 1e9,
        gas_price_wei: gas_price.to_string(),
        base_fee_wei: base_fee.map(|f| f.to_string()),
        priority_fee_wei: priority_fee.map(|f| f.to_string()),
        priority_fee_gwei: priority_fee.map(|f| f as f64 / 1e9),
        base_fee_gwei: base_fee.map(|f| f as f64 / 1e9),
        rpc_endpoint: crate::rpc::provider::endpoint_label(&ctx.rpc_url),
    })
}
