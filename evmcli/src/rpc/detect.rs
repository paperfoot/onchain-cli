use crate::config::ChainConfig;
use crate::errors::EvmError;
use directories::ProjectDirs;
use std::path::PathBuf;
use std::time::{Duration, SystemTime};
use tokio::time::timeout;

const CACHE_TTL_SECS: u64 = 30;
const LOCAL_PROBE_TIMEOUT_MS: u64 = 200;
const PUBLIC_PROBE_TIMEOUT_MS: u64 = 2000;

/// Select the best RPC endpoint using happy-eyeballs probing with disk cache.
pub async fn select_endpoint(
    rpc_override: Option<&str>,
    chain: &ChainConfig,
    http: &reqwest::Client,
) -> Result<String, EvmError> {
    // 1. Explicit override
    if let Some(url) = rpc_override {
        super::provider::validate_url(url)?;
        probe_rpc(http, url, chain.chain_id, PUBLIC_PROBE_TIMEOUT_MS).await?;
        return Ok(url.to_string());
    }

    // 2. Check disk cache
    if let Some(cached) = read_cache(chain.chain_id) {
        if [chain.local_rpc, chain.public_rpc].contains(&cached.as_str())
            && probe_rpc(
                http,
                &cached,
                chain.chain_id,
                if cached == chain.local_rpc {
                    LOCAL_PROBE_TIMEOUT_MS
                } else {
                    PUBLIC_PROBE_TIMEOUT_MS
                },
            )
            .await
            .is_ok()
        {
            return Ok(cached);
        }
    }

    // 3. Happy-eyeballs probe: race local vs public
    let winner = probe_endpoints(chain, http).await?;
    write_cache(chain.chain_id, &winner);
    Ok(winner)
}

async fn probe_endpoints(chain: &ChainConfig, http: &reqwest::Client) -> Result<String, EvmError> {
    let local = probe_rpc(
        http,
        chain.local_rpc,
        chain.chain_id,
        LOCAL_PROBE_TIMEOUT_MS,
    );
    let public = async {
        tokio::time::sleep(Duration::from_millis(40)).await;
        probe_rpc(
            http,
            chain.public_rpc,
            chain.chain_id,
            PUBLIC_PROBE_TIMEOUT_MS,
        )
        .await
    };
    tokio::pin!(local, public);
    tokio::select! {
        result = &mut local => {
            if result.is_ok() { return Ok(chain.local_rpc.to_string()); }
            public.await.map(|_| chain.public_rpc.to_string())
        },
        result = &mut public => {
            if result.is_ok() { return Ok(chain.public_rpc.to_string()); }
            local.await.map(|_| chain.local_rpc.to_string())
        },
    }.map_err(|_| EvmError::rpc("All RPC endpoints failed for the selected network; use --rpc-url to select another node"))
}

pub async fn probe_rpc(
    http: &reqwest::Client,
    url: &str,
    expected_chain_id: u64,
    timeout_ms: u64,
) -> Result<(), EvmError> {
    let request = async {
        let response = http
            .post(url)
            .json(&serde_json::json!({
                "jsonrpc": "2.0", "method": "eth_chainId", "params": [], "id": 1
            }))
            .send()
            .await
            .map_err(|e| EvmError::rpc(e.without_url().to_string()))?
            .error_for_status()
            .map_err(|e| EvmError::rpc(e.without_url().to_string()))?;
        let json: serde_json::Value = response
            .json()
            .await
            .map_err(|e| EvmError::rpc(e.without_url().to_string()))?;
        let chain_id = json["result"]
            .as_str()
            .and_then(|s| s.strip_prefix("0x"))
            .and_then(|s| u64::from_str_radix(s, 16).ok())
            .ok_or_else(|| EvmError::rpc("Invalid eth_chainId response"))?;
        if chain_id != expected_chain_id {
            return Err(EvmError::config(format!("RPC chain ID {chain_id} does not match selected network {expected_chain_id}; set --network correctly")));
        }
        Ok(())
    };
    timeout(Duration::from_millis(timeout_ms), request)
        .await
        .map_err(|_| EvmError::rpc_timeout("RPC chain check timed out"))?
}

fn cache_path(chain_id: u64) -> Option<PathBuf> {
    ProjectDirs::from("", "", "onchain")
        .map(|dirs| dirs.cache_dir().join(format!("rpc_winner_{chain_id}")))
}

fn read_cache(chain_id: u64) -> Option<String> {
    let path = cache_path(chain_id)?;
    let metadata = std::fs::metadata(&path).ok()?;
    let modified = metadata.modified().ok()?;
    let age = SystemTime::now().duration_since(modified).ok()?;
    if age > Duration::from_secs(CACHE_TTL_SECS) {
        return None;
    }
    std::fs::read_to_string(&path).ok()
}

fn write_cache(chain_id: u64, url: &str) {
    if let Some(path) = cache_path(chain_id) {
        if let Some(parent) = path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        let _ = std::fs::write(&path, url);
    }
}
