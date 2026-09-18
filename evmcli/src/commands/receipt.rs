use alloy::network::ReceiptResponse;
use alloy::primitives::B256;
use alloy::providers::Provider;
use comfy_table::Table;
use serde::Serialize;

use crate::context::AppContext;
use crate::errors::EvmError;
use crate::output::table::Tableable;

#[derive(Debug, Serialize)]
pub struct ReceiptResult {
    pub hash: String,
    pub status: String,
    pub block_number: u64,
    pub gas_used: String,
    pub effective_gas_price: String,
    pub logs_count: usize,
    pub contract_address: Option<String>,
    pub rpc_endpoint: String,
}

impl Tableable for ReceiptResult {
    fn to_table(&self) -> Table {
        let mut table = Table::new();
        table.add_row(vec!["Hash", &self.hash]);
        table.add_row(vec!["Status", &self.status]);
        table.add_row(vec!["Block", &self.block_number.to_string()]);
        table.add_row(vec!["Gas Used", &self.gas_used]);
        table.add_row(vec!["Gas Price", &self.effective_gas_price]);
        table.add_row(vec!["Logs", &self.logs_count.to_string()]);
        if let Some(ref addr) = self.contract_address {
            table.add_row(vec!["Contract", addr]);
        }
        table.add_row(vec!["RPC", &self.rpc_endpoint]);
        table
    }
}

pub async fn run(ctx: &AppContext, hash: &str) -> Result<ReceiptResult, EvmError> {
    let tx_hash: B256 = hash
        .parse()
        .map_err(|_| EvmError::validation(format!("Invalid tx hash: {hash}")))?;

    let receipt = ctx
        .provider
        .get_transaction_receipt(tx_hash)
        .await
        .map_err(|e| EvmError::rpc(format!("get_receipt failed: {e}")))?
        .ok_or_else(|| EvmError::rpc(format!("Receipt not found for {hash}")))?;

    let status = if receipt.status() {
        "success"
    } else {
        "reverted"
    };

    Ok(ReceiptResult {
        hash: format!("{tx_hash}"),
        status: status.to_string(),
        block_number: receipt.block_number.unwrap_or(0),
        gas_used: receipt.gas_used.to_string(),
        effective_gas_price: receipt.effective_gas_price.to_string(),
        logs_count: receipt.inner.logs().len(),
        contract_address: receipt.contract_address.map(|a| format!("{a}")),
        rpc_endpoint: crate::rpc::provider::endpoint_label(&ctx.rpc_url),
    })
}
