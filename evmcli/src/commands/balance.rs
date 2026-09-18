use alloy::primitives::{Address, U256};
use alloy::providers::Provider;
use alloy::sol;
use comfy_table::Table;
use serde::Serialize;

use crate::context::AppContext;
use crate::errors::EvmError;
use crate::output::table::Tableable;

// ERC20 balanceOf + decimals + symbol
sol! {
    #[sol(rpc)]
    interface IERC20 {
        function balanceOf(address account) external view returns (uint256);
        function decimals() external view returns (uint8);
        function symbol() external view returns (string);
    }
}

#[derive(Debug, Serialize)]
pub struct BalanceResult {
    pub address: String,
    pub balance_wei: String,
    pub balance_formatted: String,
    pub symbol: String,
    pub decimals: u8,
    pub token_contract: Option<String>,
    pub rpc_endpoint: String,
}

impl Tableable for BalanceResult {
    fn to_table(&self) -> Table {
        let mut table = Table::new();
        table.add_row(vec!["Address", &self.address]);
        table.add_row(vec!["Balance", &self.balance_formatted]);
        table.add_row(vec!["Symbol", &self.symbol]);
        table.add_row(vec!["Wei", &self.balance_wei]);
        if let Some(ref token) = self.token_contract {
            table.add_row(vec!["Token", token]);
        }
        table.add_row(vec!["RPC", &self.rpc_endpoint]);
        table
    }
}

fn format_units(wei: U256, decimals: u8) -> String {
    let digits = wei.to_string();
    if decimals == 0 {
        return digits;
    }
    let padded = format!("{:0>width$}", digits, width = decimals as usize + 1);
    let (whole, fraction) = padded.split_at(padded.len() - decimals as usize);
    let fraction = fraction.trim_end_matches('0');
    if fraction.is_empty() {
        whole.to_string()
    } else {
        format!("{whole}.{fraction}")
    }
}

pub async fn run(
    ctx: &AppContext,
    address: &str,
    token: Option<&str>,
) -> Result<BalanceResult, EvmError> {
    let addr: Address = address
        .parse()
        .map_err(|_| EvmError::validation(format!("Invalid address: {address}")))?;

    match token {
        None => {
            // Native ETH balance
            let balance = ctx
                .provider
                .get_balance(addr)
                .await
                .map_err(|e| EvmError::rpc(format!("get_balance failed: {e}")))?;

            Ok(BalanceResult {
                address: format!("{addr}"),
                balance_wei: balance.to_string(),
                balance_formatted: format_units(balance, ctx.chain.native_decimals),
                symbol: ctx.chain.native_symbol.to_string(),
                decimals: ctx.chain.native_decimals,
                token_contract: None,
                rpc_endpoint: crate::rpc::provider::endpoint_label(&ctx.rpc_url),
            })
        }
        Some(token_addr) => {
            let token_address: Address = token_addr.parse().map_err(|_| {
                EvmError::validation(format!("Invalid token address: {token_addr}"))
            })?;

            let contract = IERC20::new(token_address, &ctx.provider);

            // Bind the calls first, then await concurrently
            let balance_call = contract.balanceOf(addr);
            let decimals_call = contract.decimals();
            let symbol_call = contract.symbol();

            let (balance_res, decimals_res, symbol_res) = tokio::join!(
                balance_call.call(),
                decimals_call.call(),
                symbol_call.call(),
            );

            let balance =
                balance_res.map_err(|e| EvmError::rpc(format!("balanceOf failed: {e}")))?;
            // Missing symbol can be displayed; guessing decimals would misstate the amount.
            let decimals = decimals_res.map_err(|_| {
                EvmError::rpc("Token decimals() failed; cannot safely format the balance")
            })?;
            let symbol = symbol_res.unwrap_or_else(|_| "???".to_string());

            Ok(BalanceResult {
                address: format!("{addr}"),
                balance_wei: balance.to_string(),
                balance_formatted: format_units(balance, decimals),
                symbol,
                decimals,
                token_contract: Some(format!("{token_address}")),
                rpc_endpoint: crate::rpc::provider::endpoint_label(&ctx.rpc_url),
            })
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn token_units_do_not_overflow() {
        assert_eq!(format_units(U256::from(123400), 4), "12.34");
        assert_eq!(format_units(U256::ZERO, 255), "0");
        let formatted = format_units(U256::MAX, 255);
        assert!(formatted.starts_with("0."));
        assert_eq!(formatted.len(), 257);
    }
}
