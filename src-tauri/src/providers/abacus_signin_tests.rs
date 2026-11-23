//! The Abacus sign-in client against a stand-in of the service (spec 096, T014).

use std::time::Duration;

use serde_json::{json, Value};
use tokio_util::sync::CancellationToken;
use wiremock::matchers::{body_json, method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

use super::{
    interval_from, Client, Prompt, Secret, SigninError, DEFAULT_INTERVAL, POLL_PATH, START_PATH,
    WAIT_LIMIT,
};

const EMAIL: &str = "placeholder-person@example.invalid";
const DEVICE_ID: &str = "placeholder-device-id-of-signin-tests";
const USER_CODE: &str = "placeholder-user-code-of-signin-tests";
const DEVICE_CODE: &str = "placeholder-device-code-of-signin-tests";
const VERIFICATION_URI: &str = "https://signin.example.invalid/device";

fn started(interval: Value) -> Value {
    json!({
        "success": true,
        "result": {
            "verificationUri": VERIFICATION_URI,
            "userCode": USER_CODE,
            "pollingInterval": interval,
        }
    })
}

fn not_yet() -> Value {
    json!({ "success": false, "error": "not confirmed" })
}

fn confirmed() -> Value {
    json!({
        "success": true,
        "result": { "deviceCode": DEVICE_CODE, "chatWebUrl": "https://chat.example.invalid" }
    })
}

fn prompt(interval: Duration) -> Prompt {
    Prompt {
        uri: VERIFICATION_URI.to_string(),
        code: Secret::new(USER_CODE.to_string()),
        interval,
    }
}

fn email() -> Secret {
    Secret::new(EMAIL.to_string())
}

fn client(server: &MockServer) -> Client {
    Client::with_base(&server.uri()).unwrap()
}

/// A client for an address nobody listens on: the port was free a moment ago. (Dropping a
/// `MockServer` is no good for this, it goes back to a pool and keeps answering.)
async fn client_without_server() -> Client {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    drop(listener);
    Client::with_base(&format!("http://{address}")).unwrap()
}

async fn mount_start(server: &MockServer, answer: Value) {
    Mock::given(method("POST"))
        .and(path(START_PATH))
        .respond_with(ResponseTemplate::new(200).set_body_json(answer))
        .mount(server)
        .await;
}

async fn mount_poll(server: &MockServer, answer: Value, times: Option<u64>) {
    let mock = Mock::given(method("POST"))
        .and(path(POLL_PATH))
        .respond_with(ResponseTemplate::new(200).set_body_json(answer));
    match times {
        Some(n) => mock.up_to_n_times(n).mount(server).await,
        None => mock.mount(server).await,
    }
}

async fn poll_requests(server: &MockServer) -> usize {
    server
        .received_requests()
        .await
        .unwrap()
        .iter()
        .filter(|r| r.url.path() == POLL_PATH)
        .count()
}

/// Every way the values can be printed; none may hold what is secret or personal.
fn assert_nothing_leaks(printed: &[String]) {
    for text in printed {
        for secret in [EMAIL, USER_CODE, DEVICE_CODE] {
            assert!(!text.contains(secret), "leaked in: {text}");
        }
    }
}

#[tokio::test]
async fn start_sends_email_and_device_id_and_returns_the_prompt() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path(START_PATH))
        .and(body_json(json!({ "email": EMAIL, "deviceId": DEVICE_ID })))
        .respond_with(ResponseTemplate::new(200).set_body_json(started(json!(7))))
        .expect(1)
        .mount(&server)
        .await;

    let prompt = client(&server).start(&email(), DEVICE_ID).await.unwrap();

    assert_eq!(prompt.uri, VERIFICATION_URI);
    assert_eq!(prompt.code.expose(), USER_CODE);
    assert_eq!(prompt.interval, Duration::from_secs(7));
}

#[tokio::test]
async fn start_with_a_refusal_carries_the_vendors_text() {
    let server = MockServer::start().await;
    mount_start(
        &server,
        json!({ "success": false, "error": "No account for this address" }),
    )
    .await;

    let error = client(&server)
        .start(&email(), DEVICE_ID)
        .await
        .unwrap_err();

    assert_eq!(
        error,
        SigninError::Refused("No account for this address".to_string())
    );
    assert!(error.to_string().contains("No account for this address"));
}

#[tokio::test]
async fn start_with_a_refusal_that_echoes_the_email_does_not_expose_it() {
    let server = MockServer::start().await;
    mount_start(
        &server,
        json!({ "success": false, "error": format!("No account for {EMAIL}") }),
    )
    .await;

    let error = client(&server)
        .start(&email(), DEVICE_ID)
        .await
        .unwrap_err();

    assert_eq!(error, SigninError::Refused("no reason given".to_string()));
    assert!(!error.to_string().contains(EMAIL));
    assert!(!format!("{error:?}").contains(EMAIL));
}

#[tokio::test]
async fn start_with_a_refusal_without_text_uses_a_fixed_one() {
    let server = MockServer::start().await;
    mount_start(&server, json!({ "success": false })).await;

    let error = client(&server)
        .start(&email(), DEVICE_ID)
        .await
        .unwrap_err();

    assert!(matches!(error, SigninError::Refused(text) if !text.is_empty()));
}

#[tokio::test]
async fn start_with_an_error_status_and_the_refusal_in_the_body_is_a_refusal() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path(START_PATH))
        .respond_with(
            ResponseTemplate::new(400).set_body_json(json!({ "success": false, "error": "bad" })),
        )
        .mount(&server)
        .await;

    let error = client(&server)
        .start(&email(), DEVICE_ID)
        .await
        .unwrap_err();

    assert_eq!(error, SigninError::Refused("bad".to_string()));
}

#[tokio::test]
async fn start_with_a_malformed_answer_is_unexpected_and_echoes_nothing() {
    let server = MockServer::start().await;
    // Valid JSON of the wrong shape: it holds the user code but no address.
    mount_start(
        &server,
        json!({ "success": true, "result": { "userCode": USER_CODE } }),
    )
    .await;
    let shaped = client(&server)
        .start(&email(), DEVICE_ID)
        .await
        .unwrap_err();

    let other = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path(START_PATH))
        .respond_with(ResponseTemplate::new(200).set_body_string(format!("<html>{USER_CODE}")))
        .mount(&other)
        .await;
    let not_json = client(&other).start(&email(), DEVICE_ID).await.unwrap_err();

    assert_eq!(shaped, SigninError::Unexpected);
    assert_eq!(not_json, SigninError::Unexpected);
    assert_nothing_leaks(&[
        format!("{shaped}"),
        format!("{shaped:?}"),
        format!("{not_json}"),
        format!("{not_json:?}"),
    ]);
}

#[tokio::test]
async fn start_when_the_server_is_gone_is_a_network_error() {
    let gone = client_without_server().await;

    let error = gone.start(&email(), DEVICE_ID).await.unwrap_err();

    assert_eq!(error, SigninError::Network);
}

#[test]
fn interval_missing_or_zero_falls_back_to_five_seconds_and_the_rest_is_clamped() {
    assert_eq!(DEFAULT_INTERVAL, Duration::from_secs(5));
    assert_eq!(interval_from(None), Duration::from_secs(5));
    assert_eq!(interval_from(Some(0.0)), Duration::from_secs(5));
    assert_eq!(interval_from(Some(-3.0)), Duration::from_secs(5));
    assert_eq!(interval_from(Some(f64::NAN)), Duration::from_secs(5));
    assert_eq!(interval_from(Some(0.2)), Duration::from_secs(1));
    assert_eq!(interval_from(Some(9.0)), Duration::from_secs(9));
    assert_eq!(interval_from(Some(1000.0)), Duration::from_secs(60));
}

#[tokio::test]
async fn start_reads_the_interval_from_the_answer() {
    for (sent, expected) in [
        (json!(0), 5),
        (json!(null), 5),
        (json!(1000), 60),
        (json!(12), 12),
    ] {
        let server = MockServer::start().await;
        mount_start(&server, started(sent)).await;
        let prompt = client(&server).start(&email(), DEVICE_ID).await.unwrap();
        assert_eq!(prompt.interval, Duration::from_secs(expected));
    }
    // A missing field, not only a null one.
    let server = MockServer::start().await;
    mount_start(
        &server,
        json!({ "success": true, "result": { "verificationUri": VERIFICATION_URI, "userCode": USER_CODE } }),
    )
    .await;
    let prompt = client(&server).start(&email(), DEVICE_ID).await.unwrap();
    assert_eq!(prompt.interval, Duration::from_secs(5));
}

#[tokio::test]
async fn poll_goes_on_until_the_code_is_confirmed_and_returns_the_device_code() {
    let server = MockServer::start().await;
    mount_poll(&server, not_yet(), Some(2)).await;
    mount_poll(&server, confirmed(), None).await;

    let code = client(&server)
        .poll(
            &prompt(Duration::from_millis(20)),
            &CancellationToken::new(),
        )
        .await
        .unwrap();

    assert_eq!(code.expose(), DEVICE_CODE);
    assert_eq!(poll_requests(&server).await, 3);
    // Each request carries the user code and nothing else.
    let requests = server.received_requests().await.unwrap();
    let sent: Value = serde_json::from_slice(&requests[0].body).unwrap();
    assert_eq!(sent, json!({ "userCode": USER_CODE }));
}

#[test]
fn the_wait_limit_is_ten_minutes() {
    assert_eq!(WAIT_LIMIT, Duration::from_secs(600));
}

// Real time, not a paused clock: the request timeout of the HTTP client runs on the same clock and
// would fire while a stand-in answer is still on its way.
#[tokio::test]
async fn poll_without_confirmation_until_the_limit_expires() {
    let server = MockServer::start().await;
    mount_poll(&server, not_yet(), None).await;
    let limit = Duration::from_millis(400);
    let started_at = std::time::Instant::now();

    let error = client(&server)
        .with_wait_limit(limit)
        .poll(
            &prompt(Duration::from_millis(50)),
            &CancellationToken::new(),
        )
        .await
        .unwrap_err();

    assert_eq!(error, SigninError::Expired);
    let waited = started_at.elapsed();
    assert!(waited >= limit, "gave up after {waited:?}");
    assert!(waited < Duration::from_secs(5), "waited {waited:?}");
    assert!(poll_requests(&server).await >= 2);
}

#[tokio::test]
async fn cancelling_stops_polling_at_once_and_sends_no_further_request() {
    let server = MockServer::start().await;
    mount_poll(&server, not_yet(), None).await;
    let cancel = CancellationToken::new();
    let signin = client(&server);
    let waiting = prompt(Duration::from_millis(50));

    let task = tokio::spawn({
        let cancel = cancel.clone();
        async move { signin.poll(&waiting, &cancel).await }
    });
    tokio::time::sleep(Duration::from_millis(300)).await;
    assert!(
        poll_requests(&server).await >= 1,
        "polling should have begun"
    );

    cancel.cancel();
    let result = task.await.unwrap();
    assert_eq!(result.unwrap_err(), SigninError::Cancelled);
    let after_cancel = poll_requests(&server).await;

    tokio::time::sleep(Duration::from_millis(400)).await;
    assert_eq!(poll_requests(&server).await, after_cancel);
}

#[tokio::test]
async fn cancelling_ends_a_request_that_is_under_way() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path(POLL_PATH))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_json(not_yet())
                .set_delay(Duration::from_secs(10)),
        )
        .mount(&server)
        .await;
    let cancel = CancellationToken::new();
    let signin = client(&server);
    let waiting = prompt(Duration::from_secs(5));

    let task = tokio::spawn({
        let cancel = cancel.clone();
        async move { signin.poll(&waiting, &cancel).await }
    });
    tokio::time::sleep(Duration::from_millis(300)).await;
    cancel.cancel();

    let result = tokio::time::timeout(Duration::from_secs(3), task)
        .await
        .expect("cancel must not wait for the answer")
        .unwrap();
    assert_eq!(result.unwrap_err(), SigninError::Cancelled);
}

#[tokio::test]
async fn a_token_cancelled_before_polling_sends_nothing() {
    let server = MockServer::start().await;
    mount_poll(&server, not_yet(), None).await;
    let cancel = CancellationToken::new();
    cancel.cancel();

    let error = client(&server)
        .poll(&prompt(Duration::from_secs(5)), &cancel)
        .await
        .unwrap_err();

    assert_eq!(error, SigninError::Cancelled);
    assert_eq!(poll_requests(&server).await, 0);
}

#[tokio::test]
async fn poll_survives_a_failed_answer_between_good_ones() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path(POLL_PATH))
        .respond_with(ResponseTemplate::new(502).set_body_string("bad gateway"))
        .up_to_n_times(2)
        .mount(&server)
        .await;
    mount_poll(&server, confirmed(), None).await;

    let code = client(&server)
        .poll(
            &prompt(Duration::from_millis(10)),
            &CancellationToken::new(),
        )
        .await
        .unwrap();

    assert_eq!(code.expose(), DEVICE_CODE);
}

#[tokio::test]
async fn poll_gives_up_after_three_failed_answers_in_a_row() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path(POLL_PATH))
        .respond_with(ResponseTemplate::new(502).set_body_string(format!("{USER_CODE} down")))
        .mount(&server)
        .await;

    let error = client(&server)
        .poll(
            &prompt(Duration::from_millis(10)),
            &CancellationToken::new(),
        )
        .await
        .unwrap_err();

    assert_eq!(error, SigninError::Unexpected);
    assert_eq!(poll_requests(&server).await, 3);
    assert_nothing_leaks(&[format!("{error}"), format!("{error:?}")]);
}

#[tokio::test]
async fn poll_with_the_server_gone_is_a_network_error() {
    let gone = client_without_server().await;

    let error = gone
        .poll(
            &prompt(Duration::from_millis(10)),
            &CancellationToken::new(),
        )
        .await
        .unwrap_err();

    assert_eq!(error, SigninError::Network);
}

#[tokio::test]
async fn nothing_printable_holds_the_email_the_user_code_or_the_device_code() {
    let server = MockServer::start().await;
    mount_start(&server, started(json!(5))).await;
    mount_poll(&server, confirmed(), None).await;
    let signin = client(&server);

    let prompt = signin.start(&email(), DEVICE_ID).await.unwrap();
    let device_code = signin
        .poll(&prompt, &CancellationToken::new())
        .await
        .unwrap();

    let secrets = [email(), prompt.code.clone(), device_code];
    let mut printed = vec![format!("{signin:?}"), format!("{prompt:?}")];
    for secret in &secrets {
        printed.push(format!("{secret:?}"));
        printed.push(format!("{secret}"));
        printed.push(format!("{secret:#?}"));
    }
    for error in [
        SigninError::Refused("vendor words".to_string()),
        SigninError::Network,
        SigninError::Expired,
        SigninError::Cancelled,
        SigninError::Unexpected,
    ] {
        printed.push(format!("{error}"));
        printed.push(format!("{error:?}"));
    }
    assert_nothing_leaks(&printed);

    // The address and the interval are for the screen, so they may be printed.
    assert!(format!("{prompt:?}").contains(VERIFICATION_URI));
}
