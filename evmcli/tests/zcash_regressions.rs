use onchain::zcash::client::{Client, ReadRequest};
use serde_json::{json, Value};
use std::time::{Duration, Instant};
use wiremock::matchers::method;
use wiremock::{Mock, MockServer, Request, ResponseTemplate};

fn body(request: &Request) -> Value {
    request.body_json().expect("request body must be JSON")
}

fn rpc_error(result: Result<Vec<Value>, onchain::errors::EvmError>) -> onchain::errors::EvmError {
    result.expect_err("request should fail")
}

#[tokio::test]
async fn unordered_batch_is_restored_to_request_order_with_one_injected_chain_check() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(|request: &Request| {
            let payload = body(request);
            let Some(calls) = payload.as_array() else {
                return false;
            };
            calls.len() == 3
                && calls[0]
                    == json!({
                        "jsonrpc": "1.0",
                        "id": 0,
                        "method": "getblockcount",
                        "params": []
                    })
                && calls[1]
                    == json!({
                        "jsonrpc": "1.0",
                        "id": 1,
                        "method": "getbestblockhash",
                        "params": []
                    })
                && calls[2]
                    == json!({
                        "jsonrpc": "1.0",
                        "id": 2,
                        "method": "getblockchaininfo",
                        "params": []
                    })
        })
        .respond_with(ResponseTemplate::new(200).set_body_json(json!([
            {"result": {"chain": "main"}, "error": null, "id": 2},
            {"result": "0000000000000000000best", "error": null, "id": 1},
            {"result": 2_999_999, "error": null, "id": 0}
        ])))
        .expect(1)
        .mount(&server)
        .await;

    let client = Client::new(&server.uri(), "main", 1_000, None).unwrap();
    let requests = [
        ReadRequest::new("getblockcount", json!([])),
        ReadRequest::new("getbestblockhash", json!([])),
    ];
    let results = client.read(&requests).await.unwrap();

    assert_eq!(
        results,
        vec![json!(2_999_999), json!("0000000000000000000best")]
    );
    assert_eq!(server.received_requests().await.unwrap().len(), 1);
}

#[tokio::test]
async fn wrong_chain_is_rejected() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(|request: &Request| {
            body(request)
                == json!({
                    "jsonrpc": "1.0",
                    "id": 0,
                    "method": "getblockchaininfo",
                    "params": []
                })
        })
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "result": {"chain": "test"},
            "error": null,
            "id": 0
        })))
        .expect(1)
        .mount(&server)
        .await;

    let client = Client::new(&server.uri(), "main", 1_000, None).unwrap();
    let error = rpc_error(
        client
            .read(&[ReadRequest::new("getblockchaininfo", json!([]))])
            .await,
    );

    assert!(error
        .to_string()
        .contains("Zcash network mismatch: expected main, received test"));
}

#[tokio::test]
async fn duplicate_response_ids_are_rejected() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!([
            {"result": 1, "error": null, "id": 0},
            {"result": 2, "error": null, "id": 0},
            {"result": {"chain": "main"}, "error": null, "id": 2}
        ])))
        .expect(1)
        .mount(&server)
        .await;

    let client = Client::new(&server.uri(), "main", 1_000, None).unwrap();
    let error = rpc_error(
        client
            .read(&[
                ReadRequest::new("getblockcount", json!([])),
                ReadRequest::new("getdifficulty", json!([])),
            ])
            .await,
    );

    assert!(error.to_string().contains("duplicate response IDs"));
}

#[tokio::test]
async fn missing_response_ids_are_rejected() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!([
            {"result": 1, "error": null, "id": 0},
            {"result": {"chain": "main"}, "error": null, "id": 2}
        ])))
        .expect(1)
        .mount(&server)
        .await;

    let client = Client::new(&server.uri(), "main", 1_000, None).unwrap();
    let error = rpc_error(
        client
            .read(&[
                ReadRequest::new("getblockcount", json!([])),
                ReadRequest::new("getdifficulty", json!([])),
            ])
            .await,
    );

    assert!(error.to_string().contains("omitted a response"));
}

#[tokio::test]
async fn http_500_with_a_json_rpc_error_reports_the_node_error() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .respond_with(ResponseTemplate::new(500).set_body_json(json!([
            {
                "result": null,
                "error": {"code": -8, "message": "Block height out of range"},
                "id": 0
            },
            {"result": {"chain": "main"}, "error": null, "id": 1}
        ])))
        .expect(1)
        .mount(&server)
        .await;

    let client = Client::new(&server.uri(), "main", 1_000, None).unwrap();
    let error = rpc_error(
        client
            .read(&[ReadRequest::new("getblockhash", json!([4_000_000]))])
            .await,
    );

    let message = error.to_string();
    assert!(message.contains("getblockhash (-8): Block height out of range"));
    assert!(!message.contains("HTTP 500"));
}

#[tokio::test]
async fn whole_response_stall_hits_the_client_deadline_without_leaking_credentials() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_delay(Duration::from_secs(1))
                .set_body_json(json!({
                    "result": {"chain": "main"},
                    "error": null,
                    "id": 0
                })),
        )
        .expect(1)
        .mount(&server)
        .await;

    let credentialed_url =
        server
            .uri()
            .replacen("http://", "http://rpc-secret-user:rpc-secret-password@", 1);
    let client = Client::new(
        &credentialed_url,
        "main",
        100,
        Some(("header-secret-user".into(), "header-secret-password".into())),
    )
    .unwrap();
    let started = Instant::now();
    let error = rpc_error(
        client
            .read(&[ReadRequest::new("getblockchaininfo", json!([]))])
            .await,
    );
    let elapsed = started.elapsed();

    assert!(
        (Duration::from_millis(75)..Duration::from_millis(900)).contains(&elapsed),
        "configured 100 ms deadline was not observed: {elapsed:?}"
    );
    let message = error.to_string();
    assert_eq!(error.machine_code(), "rpc.error");
    for secret in [
        "rpc-secret-user",
        "rpc-secret-password",
        "header-secret-user",
        "header-secret-password",
    ] {
        assert!(!message.contains(secret), "credential leaked: {message}");
    }
}

#[tokio::test]
async fn forbidden_method_is_rejected_before_any_http_request() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .respond_with(ResponseTemplate::new(500))
        .expect(0)
        .mount(&server)
        .await;

    let client = Client::new(&server.uri(), "main", 1_000, None).unwrap();
    let error = rpc_error(
        client
            .read(&[ReadRequest::new("sendtoaddress", json!(["t1", 1]))])
            .await,
    );

    assert_eq!(error.machine_code(), "validation.error");
    assert!(server.received_requests().await.unwrap().is_empty());
}
