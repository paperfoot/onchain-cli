use crate::errors::EvmError;
use crate::rpc::provider::{endpoint_label, validate_url};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::HashMap;
use std::time::Duration;

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ReadRequest {
    pub method: String,
    #[serde(default = "empty_params")]
    pub params: Value,
}
fn empty_params() -> Value {
    json!([])
}

impl ReadRequest {
    pub fn new(method: &str, params: Value) -> Self {
        Self {
            method: method.into(),
            params,
        }
    }
    pub fn validate(&self) -> Result<(), EvmError> {
        // Explicit allowlist: no wallet exports, signing, broadcasting or node administration.
        const METHODS: &[&str] = &[
            "getblockchaininfo",
            "getblockcount",
            "getbestblockhash",
            "getblockhash",
            "getblock",
            "getblockheader",
            "getrawtransaction",
            "decoderawtransaction",
            "getaddressbalance",
            "getaddressutxos",
            "getaddresstxids",
            "getaddressmempool",
            "getrawmempool",
            "getmempoolinfo",
            "gettxout",
            "getnetworkinfo",
            "getinfo",
            "getdifficulty",
            "getblocksubsidy",
            "getnetworksolps",
            "z_gettreestate",
            "z_getsubtreesbyindex",
            "getdeprecationinfo",
            "getstandardfee",
        ];
        if !METHODS.contains(&self.method.as_str()) {
            return Err(EvmError::validation(
                "Zcash batch accepts only documented read-only node methods",
            ));
        }
        if !self.params.is_array() {
            return Err(EvmError::validation(
                "Zcash RPC params must be a JSON array",
            ));
        }
        Ok(())
    }
}

pub struct Client {
    http: reqwest::Client,
    url: reqwest::Url,
    auth: Option<(String, String)>,
    chain: &'static str,
}

impl Client {
    pub fn new(
        url: &str,
        chain: &'static str,
        timeout_ms: u64,
        auth: Option<(String, String)>,
    ) -> Result<Self, EvmError> {
        let url = validate_url(url)?;
        let loopback = matches!(
            url.host_str(),
            Some("localhost" | "127.0.0.1" | "[::1]" | "::1")
        );
        if (auth.is_some() || !url.username().is_empty()) && url.scheme() != "https" && !loopback {
            return Err(EvmError::config(
                "Authenticated Zcash RPC requires HTTPS or a loopback URL",
            ));
        }
        let http = reqwest::Client::builder()
            .user_agent(concat!("onchain/", env!("CARGO_PKG_VERSION")))
            .connect_timeout(Duration::from_millis(timeout_ms.min(3000)))
            .timeout(Duration::from_millis(timeout_ms))
            .pool_idle_timeout(Duration::from_secs(60))
            .tcp_nodelay(true)
            .redirect(reqwest::redirect::Policy::none())
            .build()
            .map_err(|_| EvmError::config("Cannot build Zcash HTTP client"))?;
        Ok(Self {
            http,
            url,
            auth,
            chain,
        })
    }

    pub fn endpoint(&self) -> String {
        endpoint_label(self.url.as_str())
    }

    /// Include chain validation in the same HTTP round trip; never cache a balance or a tip.
    pub async fn read(&self, requests: &[ReadRequest]) -> Result<Vec<Value>, EvmError> {
        if requests.is_empty() || requests.len() > 100 {
            return Err(EvmError::validation("Zcash batches require 1-100 requests"));
        }
        for request in requests {
            request.validate()?;
        }
        let info_index = requests
            .iter()
            .position(|r| r.method == "getblockchaininfo" && r.params == json!([]));
        let mut calls = requests.to_vec();
        let info_index = info_index.unwrap_or_else(|| {
            calls.push(ReadRequest::new("getblockchaininfo", json!([])));
            calls.len() - 1
        });
        let body: Vec<Value> = calls.iter().enumerate().map(|(id, call)| {
            json!({"jsonrpc":"1.0", "id": id, "method": call.method, "params": call.params})
        }).collect();
        // Single reads use a normal request; multi-reads use native JSON-RPC batching.
        let body = if body.len() == 1 {
            body[0].clone()
        } else {
            json!(body)
        };
        let mut request = self.http.post(self.url.clone()).json(&body);
        if let Some((user, password)) = &self.auth {
            request = request.basic_auth(user, Some(password));
        }
        let response = request
            .send()
            .await
            .map_err(|e| EvmError::rpc(e.to_string()))?;
        let status = response.status();
        if status == reqwest::StatusCode::UNAUTHORIZED || status == reqwest::StatusCode::FORBIDDEN {
            return Err(EvmError::config(
                "Zcash RPC authentication failed; configure the cookie file or RPC user/password",
            ));
        }
        if !status.is_success() && status != reqwest::StatusCode::INTERNAL_SERVER_ERROR {
            return Err(EvmError::rpc(format!(
                "Zcash RPC HTTP {status}; check the provider's rate limits and batch support"
            )));
        }
        let value: Value = response.json().await.map_err(|_| {
            EvmError::rpc(format!("Zcash RPC returned invalid JSON (HTTP {status})"))
        })?;
        // zcashd can return a valid JSON-RPC error together with HTTP 500.
        let responses = match value {
            Value::Array(values) => values,
            Value::Object(_) => vec![value],
            _ => return Err(EvmError::rpc("Invalid Zcash RPC response envelope")),
        };
        let mut by_id = HashMap::new();
        for response in responses {
            if response.get("id").is_none_or(Value::is_null) {
                if let Some(error) = response.get("error").filter(|e| !e.is_null()) {
                    return Err(EvmError::rpc(format!("Zcash RPC rejected the request: {} (batch reads require JSON-RPC batch support)", error["message"].as_str().unwrap_or("invalid request"))));
                }
            }
            let id = response
                .get("id")
                .and_then(Value::as_u64)
                .filter(|id| *id < calls.len() as u64)
                .ok_or_else(|| {
                    EvmError::rpc("Zcash RPC returned a missing or unexpected response ID")
                })?;
            if by_id.insert(id as usize, response).is_some() {
                return Err(EvmError::rpc("Zcash RPC returned duplicate response IDs"));
            }
        }
        let mut results = Vec::with_capacity(calls.len());
        for (index, call) in calls.iter().enumerate() {
            let response = by_id
                .remove(&index)
                .ok_or_else(|| EvmError::rpc("Zcash RPC omitted a response"))?;
            if let Some(error) = response.get("error").filter(|v| !v.is_null()) {
                let code = error.get("code").and_then(Value::as_i64).unwrap_or(0);
                let message = error
                    .get("message")
                    .and_then(Value::as_str)
                    .unwrap_or("unspecified node error");
                return Err(EvmError::rpc(format!(
                    "{} ({code}): {message}",
                    call.method
                )));
            }
            let result = response
                .get("result")
                .ok_or_else(|| EvmError::rpc("Zcash RPC response is missing result"))?;
            results.push(result.clone());
        }
        if !status.is_success() {
            return Err(EvmError::rpc(format!("Zcash RPC HTTP {status}")));
        }
        let actual_chain = results[info_index].get("chain").and_then(Value::as_str);
        if actual_chain != Some(self.chain) {
            return Err(EvmError::rpc(format!(
                "Zcash network mismatch: expected {}, received {}",
                self.chain,
                actual_chain.unwrap_or("unknown")
            )));
        }
        results.truncate(requests.len());
        Ok(results)
    }
}
