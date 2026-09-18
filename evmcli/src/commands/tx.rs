use alloy::consensus::Transaction as TxTrait;
use alloy::primitives::B256;
use alloy::providers::Provider;
use comfy_table::Table;
use serde::Serialize;

use crate::context::AppContext;
use crate::errors::EvmError;
use crate::output::table::Tableable;

#[derive(Debug, Serialize)]
pub struct TxResult {
    pub hash: String,
    pub block_number: Option<u64>,
    pub from: String,
    pub to: Option<String>,
    pub value: String,
    pub gas: String,
    pub gas_price: Option<String>,
    pub input: String,
    pub nonce: u64,
    pub rpc_endpoint: String,
}

impl Tableable for TxResult {
    fn to_table(&self) -> Table {
        let mut table = Table::new();
        table.add_row(vec!["Hash", &self.hash]);
        table.add_row(vec![
            "Block",
            &self
                .block_number
                .map(|b| b.to_string())
                .unwrap_or("pending".into()),
        ]);
        table.add_row(vec!["From", &self.from]);
        table.add_row(vec![
            "To",
            self.to.as_deref().unwrap_or("(contract creation)"),
        ]);
        table.add_row(vec!["Value", &self.value]);
        table.add_row(vec!["Gas", &self.gas]);
        table.add_row(vec!["Nonce", &self.nonce.to_string()]);
        let input_display = if self.input.len() > 20 {
            format!(
                "{}... ({} bytes)",
                &self.input[..20],
                (self.input.len() - 2) / 2
            )
        } else {
            self.input.clone()
        };
        table.add_row(vec!["Input", &input_display]);
        table.add_row(vec!["RPC", &self.rpc_endpoint]);
        table
    }
}

pub async fn run(ctx: &AppContext, hash: &str) -> Result<TxResult, EvmError> {
    let tx_hash: B256 = hash
        .parse()
        .map_err(|_| EvmError::validation(format!("Invalid tx hash: {hash}")))?;

    let tx = ctx
        .provider
        .get_transaction_by_hash(tx_hash)
        .await
        .map_err(|e| EvmError::rpc(format!("get_transaction failed: {e}")))?
        .ok_or_else(|| EvmError::rpc(format!("Transaction not found: {hash}")))?;

    Ok(TxResult {
        hash: format!("{tx_hash}"),
        block_number: tx.block_number,
        from: format!("{}", tx.inner.inner.signer()),
        to: tx.inner.to().map(|a| format!("{a}")),
        value: tx.inner.value().to_string(),
        gas: tx.inner.gas_limit().to_string(),
        gas_price: tx.inner.gas_price().map(|p| format!("{p}")),
        input: format!("0x{}", hex::encode(tx.inner.input())),
        nonce: tx.inner.nonce(),
        rpc_endpoint: crate::rpc::provider::endpoint_label(&ctx.rpc_url),
    })
}
