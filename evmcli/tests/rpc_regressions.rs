use onchain::commands::{abi, explorer, logs, storage, transfers};
use onchain::context::AppContext;
use onchain::output::OutputFormat;
use onchain::rpc::{detect, provider};
use serde_json::{json, Value};
use wiremock::matchers::{method, path, query_param};
use wiremock::{Mock, MockServer, Request, ResponseTemplate};

const SUBJECT: &str = "0x1111111111111111111111111111111111111111";
const OTHER: &str = "0x2222222222222222222222222222222222222222";
const TOKEN: &str = "0x3333333333333333333333333333333333333333";
const TRANSFER_TOPIC: &str = "0xddf252ad1be2c89b69c2b068fc378daa952ba7f163c4a11628f55a4df523b3ef";

async fn context(server: &MockServer, network: &str) -> AppContext {
    let rpc_url = server.uri();
    AppContext {
        provider: provider::build_read_provider(&rpc_url).await.unwrap(),
        http: provider::build_http_client(),
        chain: onchain::config::resolve_chain(network).unwrap(),
        format: OutputFormat::Json,
        rpc_url: rpc_url.clone(),
        rpc_explicit: true,
        explorer_override: Some(format!("{rpc_url}/api")),
    }
}

fn rpc_response(request: &Request, result: Value) -> ResponseTemplate {
    let request_json: Value = request.body_json().unwrap();
    ResponseTemplate::new(200).set_body_json(json!({
        "jsonrpc": "2.0",
        "id": request_json["id"].clone(),
        "result": result,
    }))
}

fn rpc_matches(request: &Request, method_name: &str, predicate: impl Fn(&Value) -> bool) -> bool {
    let Ok(body) = request.body_json::<Value>() else {
        return false;
    };
    body["method"] == method_name && predicate(&body["params"])
}

fn topic_address(address: &str) -> String {
    format!("0x{:0>64}", address.trim_start_matches("0x"))
}

fn hash(byte: u8) -> String {
    format!("0x{}", format!("{byte:02x}").repeat(32))
}

fn log_fixture(
    block: u64,
    transaction_index: u64,
    log_index: u64,
    tx_hash: String,
    from: &str,
    to: &str,
) -> Value {
    json!({
        "address": TOKEN,
        "topics": [TRANSFER_TOPIC, topic_address(from), topic_address(to)],
        "data": "0x000000000000000000000000000000000000000000000000000000000000002a",
        "blockNumber": format!("0x{block:x}"),
        "transactionHash": tx_hash,
        "transactionIndex": format!("0x{transaction_index:x}"),
        "blockHash": hash(block as u8),
        "logIndex": format!("0x{log_index:x}"),
        "removed": false,
    })
}

#[tokio::test]
async fn storage_uses_requested_block_and_preserves_full_word() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(|request: &Request| {
            rpc_matches(request, "eth_getStorageAt", |params| {
                params.as_array().is_some_and(|params| {
                    params.len() == 3
                        && params.last() == Some(&json!("0x7b"))
                        && params[0] == SUBJECT
                })
            })
        })
        .respond_with(|request: &Request| {
            rpc_response(
                request,
                json!("0x000000000000000000000000000000000000000000000000000000000000002a"),
            )
        })
        .expect(1)
        .mount(&server)
        .await;

    let result = storage::run(
        &context(&server, "ethereum").await,
        SUBJECT,
        "0x0",
        Some(123),
    )
    .await
    .unwrap();

    assert_eq!(
        result.value,
        "0x000000000000000000000000000000000000000000000000000000000000002a"
    );
    assert_eq!(result.value_decimal, "42");
    assert_eq!(result.block, Some(123));
}

#[tokio::test]
async fn participant_logs_query_both_topic_positions_sort_and_deduplicate() {
    let server = MockServer::start().await;
    let participant_topic = topic_address(SUBJECT);
    let outbound = log_fixture(102, 0, 2, hash(12), SUBJECT, OTHER);
    let inbound = log_fixture(100, 1, 0, hash(10), OTHER, SUBJECT);
    let self_transfer = log_fixture(101, 0, 1, hash(11), SUBJECT, SUBJECT);

    let outgoing_result = json!([outbound, self_transfer.clone()]);
    Mock::given(method("POST"))
        .and(move |request: &Request| {
            rpc_matches(request, "eth_getLogs", |params| {
                params[0]["topics"][1] == participant_topic
                    && params[0]["topics"]
                        .as_array()
                        .is_some_and(|topics| topics.len() == 2)
                    && params[0]["fromBlock"] == "0x64"
                    && params[0]["toBlock"] == "0x66"
            })
        })
        .respond_with(move |request: &Request| rpc_response(request, outgoing_result.clone()))
        .expect(1)
        .mount(&server)
        .await;

    let participant_topic = topic_address(SUBJECT);
    let incoming_result = json!([inbound, self_transfer]);
    Mock::given(method("POST"))
        .and(move |request: &Request| {
            rpc_matches(request, "eth_getLogs", |params| {
                params[0]["topics"][2] == participant_topic
                    && params[0]["topics"]
                        .as_array()
                        .is_some_and(|topics| topics.len() == 3)
                    && params[0]["fromBlock"] == "0x64"
                    && params[0]["toBlock"] == "0x66"
            })
        })
        .respond_with(move |request: &Request| rpc_response(request, incoming_result.clone()))
        .expect(1)
        .mount(&server)
        .await;

    let result = logs::run(
        &context(&server, "ethereum").await,
        Some(TOKEN),
        None,
        Some(SUBJECT),
        Some(100),
        Some(102),
        Some("transfer"),
    )
    .await
    .unwrap();

    assert_eq!(result.log_count, 3);
    assert_eq!(
        result
            .logs
            .iter()
            .map(|log| log.block_number)
            .collect::<Vec<_>>(),
        vec![100, 101, 102]
    );
    assert_eq!(result.logs[0].tx_hash, hash(10), "incoming log was omitted");
}

#[tokio::test]
async fn explorer_follows_two_pages_and_accepts_block_number_alias() {
    let server = MockServer::start().await;
    let endpoint = format!("/api/v2/addresses/{SUBJECT}/transactions");
    Mock::given(method("GET"))
        .and(path(endpoint.clone()))
        .and(|request: &Request| request.url.query().is_none())
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "items": [{
                "hash": hash(1),
                "block_number": 100,
                "from": {"hash": SUBJECT},
                "to": {"hash": OTHER},
                "status": "ok"
            }],
            "next_page_params": {"block_number": "99", "index": "1"}
        })))
        .expect(1)
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path(endpoint))
        .and(query_param("block_number", "99"))
        .and(query_param("index", "1"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "items": [{
                "hash": hash(2),
                "block_number": 99,
                "from": {"hash": OTHER},
                "to": {"hash": SUBJECT},
                "status": "ok"
            }],
            "next_page_params": null
        })))
        .expect(1)
        .mount(&server)
        .await;

    let result = explorer::run(&context(&server, "ethereum").await, SUBJECT, 2)
        .await
        .unwrap();

    assert_eq!(result.tx_count, 2);
    assert_eq!(result.pages_fetched, 2);
    assert!(result.next_page_params.is_none());
    assert_eq!(result.transactions[0].block, Some(100));
    assert_eq!(result.transactions[1].block, Some(99));
}

#[tokio::test]
async fn erc721_transfer_uses_blockscout_type_and_retains_token_id() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path(format!("/api/v2/addresses/{SUBJECT}/token-transfers")))
        .and(query_param("type", "ERC-721"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "items": [{
                "transaction_hash": hash(3),
                "block_number": 123,
                "from": {"hash": OTHER},
                "to": {"hash": SUBJECT},
                "total": {"token_id": "4242"},
                "token": {
                    "name": "Test NFT",
                    "symbol": "TNFT",
                    "address_hash": TOKEN,
                    "type": "ERC-721"
                },
                "type": "token_transfer"
            }],
            "next_page_params": null
        })))
        .expect(1)
        .mount(&server)
        .await;

    let result = transfers::run(&context(&server, "ethereum").await, SUBJECT, "erc721", 1)
        .await
        .unwrap();

    assert_eq!(result.transfer_count, 1);
    assert_eq!(result.transfers[0].token_id.as_deref(), Some("4242"));
    assert_eq!(result.transfers[0].value, "1");
    assert_eq!(result.transfers[0].raw_value.as_deref(), Some("1"));
}

#[tokio::test]
async fn invalid_explorer_abi_is_an_error_without_an_rpc_probe() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path(format!("/api/v2/smart-contracts/{SUBJECT}")))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "abi": [{"type": "function", "name": "broken", "inputs": "not-an-array"}]
        })))
        .expect(1)
        .mount(&server)
        .await;

    let result = abi::run(&context(&server, "ethereum").await, SUBJECT).await;

    assert!(result.is_err());
    let requests = server.received_requests().await.unwrap();
    assert_eq!(requests.len(), 1);
    assert_eq!(requests[0].method.as_str(), "GET");
}

#[tokio::test]
async fn explicit_rpc_rejects_a_chain_id_mismatch() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(|request: &Request| {
            rpc_matches(request, "eth_chainId", |params| {
                params.as_array().is_some_and(Vec::is_empty)
            })
        })
        .respond_with(|request: &Request| rpc_response(request, json!("0x1")))
        .expect(1)
        .mount(&server)
        .await;

    let http = provider::build_http_client();
    let chain = onchain::config::resolve_chain("arbitrum").unwrap();
    let result = detect::select_endpoint(Some(&server.uri()), chain, &http).await;

    let error = result.unwrap_err();
    assert_eq!(error.machine_code(), "config.error");
    assert!(error
        .to_string()
        .contains("does not match selected network 42161"));
}
