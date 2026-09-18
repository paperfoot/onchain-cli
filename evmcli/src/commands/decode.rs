use comfy_table::Table;
use serde::Serialize;
use std::collections::HashMap;

use crate::errors::EvmError;
use crate::output::table::Tableable;
use alloy::dyn_abi::JsonAbiExt;

#[derive(Debug, Serialize)]
pub struct DecodeResult {
    pub selector: String,
    pub function_name: Option<String>,
    pub arguments: Option<Vec<String>>,
    pub candidates: Vec<String>,
    pub decoding_error: Option<String>,
    pub raw_data: String,
    pub data_length: usize,
}

impl Tableable for DecodeResult {
    fn to_table(&self) -> Table {
        let mut table = Table::new();
        table.add_row(vec!["Selector", &self.selector]);
        table.add_row(vec![
            "Function",
            self.function_name.as_deref().unwrap_or("unknown"),
        ]);
        if let Some(ref args) = self.arguments {
            for (i, arg) in args.iter().enumerate() {
                table.add_row(vec![format!("Argument {i}"), arg.clone()]);
            }
        }
        if let Some(error) = &self.decoding_error {
            table.add_row(vec!["Decode error", error]);
        }
        if self.candidates.len() > 1 {
            table.add_row(vec!["Candidates", &self.candidates.join(", ")]);
        }
        table.add_row(vec!["Data Length", &format!("{} bytes", self.data_length)]);
        table
    }
}

fn known_selectors() -> HashMap<[u8; 4], &'static str> {
    let mut m = HashMap::new();
    // ERC20
    m.insert(hex_selector("a9059cbb"), "transfer(address,uint256)");
    m.insert(hex_selector("095ea7b3"), "approve(address,uint256)");
    m.insert(
        hex_selector("23b872dd"),
        "transferFrom(address,address,uint256)",
    );
    m.insert(hex_selector("70a08231"), "balanceOf(address)");
    m.insert(hex_selector("dd62ed3e"), "allowance(address,address)");
    m.insert(hex_selector("313ce567"), "decimals()");
    m.insert(hex_selector("95d89b41"), "symbol()");
    m.insert(hex_selector("06fdde03"), "name()");
    m.insert(hex_selector("18160ddd"), "totalSupply()");
    // Uniswap V3
    m.insert(
        hex_selector("414bf389"),
        "exactInputSingle((address,address,uint24,address,uint256,uint256,uint256,uint160))",
    );
    m.insert(
        hex_selector("db3e2198"),
        "exactOutputSingle((address,address,uint24,address,uint256,uint256,uint256,uint160))",
    );
    m.insert(hex_selector("ac9650d8"), "multicall(bytes[])");
    m.insert(
        hex_selector("128acb08"),
        "swap(address,bool,int256,uint160,bytes)",
    );
    // Aave V3
    m.insert(
        hex_selector("00a718a9"),
        "liquidationCall(address,address,address,uint256,bool)",
    );
    m.insert(
        hex_selector("617ba037"),
        "supply(address,uint256,address,uint16)",
    );
    m.insert(
        hex_selector("a415bcad"),
        "borrow(address,uint256,uint256,uint16,address)",
    );
    m.insert(
        hex_selector("573ade81"),
        "repay(address,uint256,uint256,address)",
    );
    m.insert(
        hex_selector("ab9c4b5d"),
        "flashLoan(address,address[],uint256[],uint256[],address,bytes,uint16)",
    );
    // Balancer V2
    m.insert(
        hex_selector("5c38449e"),
        "flashLoan(address,address[],uint256[],bytes)",
    );
    m.insert(hex_selector("52bbbe29"), "swap((bytes32,uint8,address,address,uint256,bytes),(address,bool,address,bool),uint256,uint256)");
    m.insert(hex_selector("945bcec9"), "batchSwap(uint8,(bytes32,uint256,uint256,uint256,bytes)[],address[],(address,bool,address,bool),int256[],uint256)");
    // Multicall3
    m.insert(
        hex_selector("82ad56cb"),
        "aggregate3((address,bool,bytes)[])",
    );
    m.insert(
        hex_selector("399542e9"),
        "tryBlockAndAggregate(bool,(address,bytes)[])",
    );
    // Common
    m.insert(hex_selector("8da5cb5b"), "owner()");
    m.insert(hex_selector("f2fde38b"), "transferOwnership(address)");
    m.insert(hex_selector("715018a6"), "renounceOwnership()");
    m
}

fn hex_selector(hex: &str) -> [u8; 4] {
    let bytes = hex::decode(hex).unwrap();
    [bytes[0], bytes[1], bytes[2], bytes[3]]
}

pub async fn run(data: &str) -> Result<DecodeResult, EvmError> {
    run_with_signature(data, None).await
}

pub async fn run_with_signature(data: &str, sig: Option<&str>) -> Result<DecodeResult, EvmError> {
    let data_clean = data.strip_prefix("0x").unwrap_or(data);
    let bytes = hex::decode(data_clean).map_err(|_| EvmError::decode("Invalid hex data"))?;

    if bytes.len() < 4 {
        return Err(EvmError::decode(
            "Calldata must be at least 4 bytes (function selector)",
        ));
    }

    let selector: [u8; 4] = [bytes[0], bytes[1], bytes[2], bytes[3]];
    let selector_hex = format!("0x{}", hex::encode(selector));

    // Check baked-in selectors first
    let known = known_selectors();
    let candidates = if let Some(sig) = sig {
        let function = crate::commands::call::parse_function(sig)?;
        if function.selector().as_slice() != selector {
            return Err(EvmError::decode(
                "Signature does not match the calldata selector",
            ));
        }
        vec![function.signature()]
    } else if let Some(name) = known.get(&selector) {
        vec![name.to_string()]
    } else {
        lookup_4byte(&selector_hex).await.unwrap_or_default()
    };
    let function_name = (candidates.len() == 1).then(|| candidates[0].clone());
    let mut arguments = None;
    let mut decoding_error = None;
    if let Some(ref name) = function_name {
        let decoded = crate::commands::call::parse_function(name).and_then(|function| {
            function
                .abi_decode_input(&bytes[4..])
                .map_err(|e| EvmError::decode(e.to_string()))
        });
        match decoded {
            Ok(values) => {
                arguments = Some(values.iter().map(|value| format!("{value:?}")).collect())
            }
            Err(error) if sig.is_some() => return Err(error),
            Err(error) => decoding_error = Some(error.to_string()),
        }
    }

    Ok(DecodeResult {
        selector: selector_hex,
        candidates,
        decoding_error,
        function_name,
        arguments,
        raw_data: format!("0x{}", data_clean),
        data_length: bytes.len(),
    })
}

async fn lookup_4byte(selector: &str) -> Option<Vec<String>> {
    let url =
        format!("https://api.4byte.sourcify.dev/signature-database/v1/lookup?function={selector}");
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(3))
        .build()
        .ok()?;
    let resp = client
        .get(&url)
        .send()
        .await
        .ok()?
        .error_for_status()
        .ok()?;
    let json: serde_json::Value = resp.json().await.ok()?;
    let results = json.get("result")?.get("function")?.get(selector)?;
    Some(
        results
            .as_array()?
            .iter()
            .filter_map(|entry| entry.get("name")?.as_str().map(str::to_owned))
            .collect(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn builtin_selectors_match_signatures() {
        for (selector, signature) in known_selectors() {
            let function = crate::commands::call::parse_function(signature).unwrap();
            assert_eq!(function.selector().as_slice(), selector, "{signature}");
        }
    }
    #[tokio::test]
    async fn decodes_transfer_arguments_without_rpc() {
        let (_, bytes) = crate::commands::call::encode_call(
            "transfer(address,uint256)",
            &[
                "0x0000000000000000000000000000000000000001".into(),
                "42".into(),
            ],
        )
        .unwrap();
        let result = run(&hex::encode(bytes)).await.unwrap();
        assert_eq!(result.selector, "0xa9059cbb");
        assert_eq!(result.arguments.unwrap().len(), 2);
    }
}
