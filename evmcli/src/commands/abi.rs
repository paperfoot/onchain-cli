use comfy_table::Table;
use directories::ProjectDirs;
use serde::Serialize;
use std::path::PathBuf;

use crate::context::AppContext;
use crate::errors::EvmError;
use crate::output::table::Tableable;

#[derive(Debug, Serialize)]
pub struct AbiResult {
    pub address: String,
    pub source: String,
    pub function_count: usize,
    pub event_count: usize,
    pub cache_path: Option<String>,
    pub abi_json: serde_json::Value,
}

impl Tableable for AbiResult {
    fn to_table(&self) -> Table {
        let mut table = Table::new();
        table.add_row(vec!["Address", &self.address]);
        table.add_row(vec!["Source", &self.source]);
        table.add_row(vec!["Functions", &self.function_count.to_string()]);
        table.add_row(vec!["Events", &self.event_count.to_string()]);
        if let Some(ref path) = self.cache_path {
            table.add_row(vec!["Cached", path]);
        }
        table
    }
}

fn cache_dir(chain_id: u64) -> Option<PathBuf> {
    ProjectDirs::from("", "", "onchain").map(|dirs| {
        dirs.cache_dir().join("abis").join(chain_id.to_string())
    })
}

fn sanitize_address(address: &str) -> String {
    // Only allow hex chars and 0x prefix — prevent path traversal
    address.chars().filter(|c| c.is_ascii_hexdigit() || *c == 'x' || *c == 'X').collect::<String>().to_lowercase()
}

fn read_cached_abi(chain_id: u64, address: &str) -> Option<(serde_json::Value, PathBuf)> {
    let dir = cache_dir(chain_id)?;
    let path = dir.join(format!("{}.json", sanitize_address(address)));
    let content = std::fs::read_to_string(&path).ok()?;
    let abi: serde_json::Value = serde_json::from_str(&content).ok()?;
    Some((abi, path))
}

fn write_cached_abi(chain_id: u64, address: &str, abi: &serde_json::Value) -> Option<PathBuf> {
    let dir = cache_dir(chain_id)?;
    std::fs::create_dir_all(&dir).ok()?;
    let path = dir.join(format!("{}.json", sanitize_address(address)));
    std::fs::write(&path, serde_json::to_string_pretty(abi).ok()?).ok()?;
    Some(path)
}

pub async fn run(ctx: &AppContext, address: &str) -> Result<AbiResult, EvmError> {
    crate::errors::validate_address(address)?;
    // Check cache first
    if let Some((abi, path)) = read_cached_abi(ctx.chain.chain_id, address) {
        let (funcs, events) = count_abi_entries(&abi);
        return Ok(AbiResult {
            address: address.to_string(),
            source: "cache".to_string(),
            function_count: funcs,
            event_count: events,
            cache_path: Some(path.display().to_string()),
            abi_json: abi,
        });
    }

    // Fetch from Blockscout (explorer_api_url already ends with /api)
    let url = format!("{}?module=contract&action=getabi&address={}",
        ctx.chain.explorer_api_url(), address);

    let resp = ctx.http.get(&url).send().await
        .map_err(|e| EvmError::explorer(format!("ABI fetch failed: {e}")))?;

    let json: serde_json::Value = resp.json().await
        .map_err(|e| EvmError::explorer(format!("ABI parse failed: {e}")))?;

    let abi_str = json["result"].as_str()
        .ok_or_else(|| EvmError::Abi {
            code: "abi.not_found",
            message: format!("No ABI found for {address}. Contract may not be verified."),
        })?;

    let abi: serde_json::Value = serde_json::from_str(abi_str)
        .map_err(|e| EvmError::Abi {
            code: "abi.parse_error",
            message: format!("Failed to parse ABI: {e}"),
        })?;

    let cache_path = write_cached_abi(ctx.chain.chain_id, address, &abi);
    let (funcs, events) = count_abi_entries(&abi);

    Ok(AbiResult {
        address: address.to_string(),
        source: "blockscout".to_string(),
        function_count: funcs,
        event_count: events,
        cache_path: cache_path.map(|p| p.display().to_string()),
        abi_json: abi,
    })
}

fn count_abi_entries(abi: &serde_json::Value) -> (usize, usize) {
    let arr = match abi.as_array() {
        Some(a) => a,
        None => return (0, 0),
    };
    let funcs = arr.iter().filter(|e| e["type"].as_str() == Some("function")).count();
    let events = arr.iter().filter(|e| e["type"].as_str() == Some("event")).count();
    (funcs, events)
}
