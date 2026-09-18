use alloy::dyn_abi::{FunctionExt, JsonAbiExt, Specifier};
use alloy::json_abi::Function;
use alloy::primitives::Address;
use alloy::providers::Provider;
use comfy_table::Table;
use serde::Serialize;

use crate::context::AppContext;
use crate::errors::EvmError;
use crate::output::table::Tableable;

#[derive(Debug, Serialize)]
pub struct CallResult {
    pub contract: String,
    pub function: String,
    pub result_hex: String,
    pub result_decoded: Option<String>,
    pub rpc_endpoint: String,
}

impl Tableable for CallResult {
    fn to_table(&self) -> Table {
        let mut table = Table::new();
        table.add_row(vec!["Contract", &self.contract]);
        table.add_row(vec!["Function", &self.function]);
        if let Some(ref decoded) = self.result_decoded {
            table.add_row(vec!["Result", decoded]);
        }
        table.add_row(vec!["Raw", &self.result_hex]);
        table.add_row(vec!["RPC", &self.rpc_endpoint]);
        table
    }
}

pub fn parse_function(sig: &str) -> Result<Function, EvmError> {
    let function = Function::parse(sig)
        .map_err(|e| EvmError::validation(format!("Invalid function signature: {e}")))?;
    // Resolving validates Solidity types as well as the human-readable grammar.
    for param in function.inputs.iter().chain(&function.outputs) {
        param
            .resolve()
            .map_err(|e| EvmError::validation(format!("Invalid ABI type: {e}")))?;
    }
    Ok(function)
}

pub fn encode_call(sig: &str, args: &[String]) -> Result<(Function, Vec<u8>), EvmError> {
    let function = parse_function(sig)?;
    if function.inputs.len() != args.len() {
        return Err(EvmError::validation(format!(
            "Expected {} arguments, got {}",
            function.inputs.len(),
            args.len()
        )));
    }
    let values = function
        .inputs
        .iter()
        .zip(args)
        .map(|(param, value)| {
            param
                .resolve()
                .map_err(|e| EvmError::validation(e.to_string()))?
                .coerce_str(value)
                .map_err(|e| EvmError::validation(format!("Invalid argument: {e}")))
        })
        .collect::<Result<Vec<_>, _>>()?;
    let calldata = function
        .abi_encode_input(&values)
        .map_err(|e| EvmError::validation(e.to_string()))?;
    Ok((function, calldata))
}

pub async fn run(
    ctx: &AppContext,
    address: &str,
    sig: &str,
    args: &[String],
) -> Result<CallResult, EvmError> {
    let addr: Address = address
        .parse()
        .map_err(|_| EvmError::validation("Invalid contract address"))?;
    let (function, calldata) = encode_call(sig, args)?;
    let tx = alloy::rpc::types::TransactionRequest::default()
        .to(addr)
        .input(alloy::primitives::Bytes::from(calldata).into());
    let result = ctx
        .provider
        .call(tx.into())
        .await
        .map_err(|e| EvmError::rpc(format!("eth_call failed: {e}")))?;
    let decoded = if function.outputs.is_empty() {
        None
    } else {
        let values = function.abi_decode_output(&result).map_err(|e| {
            EvmError::decode(format!(
                "Contract returned data incompatible with the output ABI: {e}"
            ))
        })?;
        Some(format!("{values:?}"))
    };
    Ok(CallResult {
        contract: addr.to_string(),
        function: function.signature_with_outputs(),
        result_hex: format!("0x{}", hex::encode(result)),
        result_decoded: decoded,
        rpc_endpoint: crate::rpc::provider::endpoint_label(&ctx.rpc_url),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn malformed_signatures_are_errors_not_panics() {
        for sig in ["foo(", "foo)(", "", "owner()(bogus)"] {
            assert!(parse_function(sig).is_err(), "{sig}");
        }
    }
    #[test]
    fn canonical_selector_and_nested_tuples() {
        let (_, data) = encode_call(
            "balanceOf( address )(uint256)",
            &["0x0000000000000000000000000000000000000001".into()],
        )
        .unwrap();
        assert_eq!(hex::encode(&data[..4]), "70a08231");
        let (function, _) = encode_call(
            "foo((uint256,address))((bool,address))",
            &["(42,0x0000000000000000000000000000000000000001)".into()],
        )
        .unwrap();
        assert_eq!(function.inputs.len(), 1);
        assert_eq!(function.outputs.len(), 1);
    }
    #[test]
    fn dynamic_return_values_decode_as_output_parameters() {
        let function = parse_function("name()(string)").unwrap();
        let data = function
            .abi_encode_output(&[alloy::dyn_abi::DynSolValue::String("Token".into())])
            .unwrap();
        assert_eq!(
            function.abi_decode_output(&data).unwrap(),
            vec![alloy::dyn_abi::DynSolValue::String("Token".into())]
        );
    }
}
