use crate::errors::EvmError;
use alloy::network::AnyNetwork;
use alloy::providers::ProviderBuilder;
use std::time::Duration;

pub type ReadProvider = alloy::providers::RootProvider<AnyNetwork>;

pub fn build_http_client() -> reqwest::Client {
    reqwest::Client::builder()
        .user_agent(concat!("onchain/", env!("CARGO_PKG_VERSION")))
        .pool_idle_timeout(Duration::from_secs(60))
        .tcp_nodelay(true)
        .connect_timeout(Duration::from_secs(3))
        .timeout(Duration::from_secs(15))
        .build()
        .expect("Failed to build HTTP client")
}

pub async fn build_read_provider(rpc_url: &str) -> Result<ReadProvider, EvmError> {
    build_read_provider_with_client(rpc_url, build_http_client())
}

pub fn build_read_provider_with_client(
    rpc_url: &str,
    http: reqwest::Client,
) -> Result<ReadProvider, EvmError> {
    let url = validate_url(rpc_url)?;
    Ok(ProviderBuilder::<_, _, AnyNetwork>::default().connect_reqwest(http, url))
}

pub fn validate_url(value: &str) -> Result<reqwest::Url, EvmError> {
    let url = reqwest::Url::parse(value).map_err(|_| EvmError::config("Invalid RPC URL"))?;
    if !matches!(url.scheme(), "http" | "https") || url.host_str().is_none() {
        return Err(EvmError::config("RPC URL must use HTTP or HTTPS"));
    }
    Ok(url)
}

/// Endpoints may carry API keys in their path, query or user information.
pub fn endpoint_label(value: &str) -> String {
    validate_url(value)
        .map(|url| url.origin().ascii_serialization())
        .unwrap_or_else(|_| "custom RPC".into())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn endpoint_labels_strip_credentials() {
        assert_eq!(
            endpoint_label("https://user:password@rpc.example/v2/secret?key=secret"),
            "https://rpc.example"
        );
        assert_eq!(
            endpoint_label("http://localhost:8545/secret"),
            "http://localhost:8545"
        );
        assert!(validate_url("file:///tmp/socket").is_err());
    }
}
