use onchain::swap::{self, Command, QuoteArgs, SwapArgs};
use serde_json::{json, Value};
use wiremock::matchers::{method, path, query_param};
use wiremock::{Mock, MockServer, Request, ResponseTemplate};

const NATIVE_ZEC: &str = "nep141:zec.omft.near";
const WRAPPED_ZEC: &str = "nep141:wzec.example.near";
const DESTINATION_ASSET: &str = "nep141:eth.omft.near";
const EXACT_AMOUNT: &str = "100000001";

fn args(server: &MockServer, command: Command) -> SwapArgs {
    SwapArgs {
        command,
        api_url: server.uri(),
        timeout_ms: 1_000,
    }
}

fn swap_error(
    result: Result<swap::SwapResult, onchain::errors::EvmError>,
) -> onchain::errors::EvmError {
    result.expect_err("swap request must fail")
}

fn quote_args() -> QuoteArgs {
    QuoteArgs {
        from: NATIVE_ZEC.to_string(),
        to: DESTINATION_ASSET.to_string(),
        amount: EXACT_AMOUNT.to_string(),
        recipient: "0x1111111111111111111111111111111111111111".to_string(),
        refund_to: "t1RefundAddress".to_string(),
        slippage_bps: 125,
        deadline_minutes: 30,
    }
}

fn quote_response(request: &Value) -> Value {
    json!({
        "quoteRequest": request,
        "quote": {
            "amountIn": EXACT_AMOUNT,
            "minAmountIn": EXACT_AMOUNT,
            "amountOut": "99000000",
            "minAmountOut": "98000000"
        }
    })
}

#[tokio::test]
async fn tokens_chain_filter_distinguishes_native_zec_from_wrapped_zec() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/v0/tokens"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!([
            {
                "assetId": NATIVE_ZEC,
                "blockchain": "zec",
                "symbol": "ZEC",
                "decimals": 8
            },
            {
                "assetId": WRAPPED_ZEC,
                "blockchain": "near",
                "symbol": "ZEC",
                "decimals": 8
            }
        ])))
        .expect(2)
        .mount(&server)
        .await;

    let native = swap::run(&args(
        &server,
        Command::Tokens {
            chain: Some("zec".to_string()),
            symbol: None,
        },
    ))
    .await
    .unwrap();
    assert_eq!(native.operation, "tokens");
    assert_eq!(native.result.as_array().unwrap().len(), 1);
    assert_eq!(native.result[0]["assetId"], NATIVE_ZEC);

    let wrapped = swap::run(&args(
        &server,
        Command::Tokens {
            chain: Some("near".to_string()),
            symbol: Some("zec".to_string()),
        },
    ))
    .await
    .unwrap();
    assert_eq!(wrapped.result.as_array().unwrap().len(), 1);
    assert_eq!(wrapped.result[0]["assetId"], WRAPPED_ZEC);
}

#[tokio::test]
async fn quote_posts_dry_exact_integer_request_and_accepts_matching_echo() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v0/quote"))
        .and(|request: &Request| {
            request.body_json::<Value>().is_ok_and(|body| {
                body["dry"] == true
                    && body["swapType"] == "EXACT_INPUT"
                    && body["amount"] == EXACT_AMOUNT
                    && body["amount"].is_string()
            })
        })
        .respond_with(|request: &Request| {
            let body: Value = request.body_json().unwrap();
            ResponseTemplate::new(200).set_body_json(quote_response(&body))
        })
        .expect(1)
        .mount(&server)
        .await;

    let result = swap::run(&args(&server, Command::Quote(quote_args())))
        .await
        .unwrap();

    assert_eq!(result.operation, "quote_preview");
    assert_eq!(result.result["quoteRequest"]["dry"], true);
    assert_eq!(result.result["quoteRequest"]["amount"], EXACT_AMOUNT);
}

#[tokio::test]
async fn quote_rejects_echo_with_different_recipient() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v0/quote"))
        .respond_with(|request: &Request| {
            let mut body: Value = request.body_json().unwrap();
            body["recipient"] = json!("0x2222222222222222222222222222222222222222");
            ResponseTemplate::new(200).set_body_json(quote_response(&body))
        })
        .expect(1)
        .mount(&server)
        .await;

    let error = swap_error(swap::run(&args(&server, Command::Quote(quote_args()))).await);

    assert_eq!(error.machine_code(), "explorer.error");
    assert!(error
        .to_string()
        .contains("does not match requested recipient"));
}

#[tokio::test]
async fn unauthorized_response_explains_swap_auth_configuration() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/v0/tokens"))
        .respond_with(ResponseTemplate::new(401))
        .expect(1)
        .mount(&server)
        .await;

    let error = swap_error(
        swap::run(&args(
            &server,
            Command::Tokens {
                chain: None,
                symbol: None,
            },
        ))
        .await,
    );

    assert_eq!(error.machine_code(), "config.error");
    let message = error.to_string();
    assert!(message.contains("authentication failed"));
    assert!(message.contains("ONCHAIN_SWAP_API_KEY"));
    assert!(message.contains("ONCHAIN_SWAP_JWT"));
}

#[tokio::test]
async fn rate_limited_response_is_not_reported_as_success() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/v0/tokens"))
        .respond_with(ResponseTemplate::new(429))
        .expect(1)
        .mount(&server)
        .await;

    let error = swap_error(
        swap::run(&args(
            &server,
            Command::Tokens {
                chain: None,
                symbol: None,
            },
        ))
        .await,
    );

    assert_eq!(error.machine_code(), "explorer.error");
    assert!(error.to_string().contains("rate limit reached (HTTP 429)"));
}

#[tokio::test]
async fn status_preserves_deposit_memo_and_rejects_unknown_status() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/v0/status"))
        .and(query_param("depositAddress", "deposit-address"))
        .and(query_param("depositMemo", "memo with spaces/+"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"status": "MYSTERY_STATUS"})))
        .expect(1)
        .mount(&server)
        .await;

    let error = swap_error(
        swap::run(&args(
            &server,
            Command::Status {
                deposit_address: "deposit-address".to_string(),
                deposit_memo: Some("memo with spaces/+".to_string()),
            },
        ))
        .await,
    );

    assert_eq!(error.machine_code(), "explorer.error");
    assert!(error.to_string().contains("unknown swap status"));
}
