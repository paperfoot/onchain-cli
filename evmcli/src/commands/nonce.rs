use alloy::primitives::Address;
use alloy::providers::Provider;
use comfy_table::Table;
use serde::Serialize;

use crate::context::AppContext;
use crate::errors::EvmError;
use crate::output::table::Tableable;

#[derive(Debug, Serialize)]
pub struct NonceResult {
    pub address: String,
    pub nonce: u64,
    pub rpc_endpoint: String,
}

impl Tableable for NonceResult {
    fn to_table(&self) -> Table {
        let mut table = Table::new();
        table.add_row(vec!["Address", &self.address]);
        table.add_row(vec!["Nonce", &self.nonce.to_string()]);
        table.add_row(vec!["RPC", &self.rpc_endpoint]);
        table
    }
}

pub async fn run(ctx: &AppContext, address: &str) -> Result<NonceResult, EvmError> {
    let addr: Address = address
        .parse()
        .map_err(|_| EvmError::validation(format!("Invalid address: {address}")))?;

    let nonce = ctx
        .provider
        .get_transaction_count(addr)
        .await
        .map_err(|e| EvmError::rpc(format!("get_transaction_count failed: {e}")))?;

    Ok(NonceResult {
        address: format!("{addr}"),
        nonce,
        rpc_endpoint: crate::rpc::provider::endpoint_label(&ctx.rpc_url),
    })
}
