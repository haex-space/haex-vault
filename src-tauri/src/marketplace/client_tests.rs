//! The HTTP side of the protocol against the scripted test marketplace (T015).

use std::time::Duration;

use serde_json::{json, Value};

use super::*;
use crate::marketplace::test_support::TestMarketplace;

fn url(text: &str) -> Url {
    Url::parse(text).expect("url")
}

async fn health(base: &str) -> Result<Value, SourceFailure> {
    Http::default().get_json(base, "/health", &[], None).await
}

#[tokio::test]
async fn a_good_answer_is_parsed_and_sends_only_the_headers_of_the_contract() {
    let market = TestMarketplace::start().await;
    market.healthy().await;
    let answer = health(&market.base_url()).await.expect("answer");
    assert_eq!(answer, json!({ "status": "ok" }));

    let requests = market.requests().await;
    assert_eq!(requests.len(), 1);
    let sent = &requests[0].headers;
    let header = |name: &str| {
        sent.get(name)
            .and_then(|v| v.to_str().ok())
            .map(str::to_owned)
    };
    assert_eq!(header("accept").as_deref(), Some("application/json"));
    assert_eq!(header("x-platform").as_deref(), Some(platform()));
    assert_eq!(header("x-app-version"), Some(app_version().to_string()));
    assert_eq!(
        header("user-agent"),
        Some(format!("HaexVault/{}", app_version()))
    );
    // Nothing that identifies the user, the vault or the device (FR-007).
    let allowed = [
        "host",
        "accept",
        "x-platform",
        "x-app-version",
        "user-agent",
        "accept-encoding",
        "connection",
    ];
    for (name, _) in sent.iter() {
        assert!(
            allowed.contains(&name.as_str()),
            "the header {name} is not in the contract"
        );
    }
}

#[tokio::test]
async fn the_protocol_paths_hang_behind_the_path_prefix() {
    let market = TestMarketplace::with_prefix("/market/api").await;
    market.healthy().await;
    health(&market.base_url()).await.expect("answer");
    assert_eq!(market.hits("/health").await, 1);
    assert_eq!(
        join_url(
            "https://example.org/market/",
            "/extensions",
            &[("page", "2".into())]
        )
        .expect("url")
        .as_str(),
        "https://example.org/market/extensions?page=2"
    );
}

#[tokio::test]
async fn query_values_are_encoded() {
    let joined = join_url(
        "https://example.org",
        "extensions",
        &[
            ("search", "a b&c=d".into()),
            ("category", "dev tools".into()),
        ],
    )
    .expect("url");
    assert_eq!(
        joined.query(),
        Some("search=a+b%26c%3Dd&category=dev+tools")
    );
}

#[tokio::test]
async fn statuses_map_to_the_kinds_of_a_source() {
    let market = TestMarketplace::start().await;
    market.json("/a", 401, json!({})).await;
    market.json("/b", 403, json!({})).await;
    market.json("/c", 404, json!({})).await;
    market.json("/d", 500, json!({})).await;
    let http = Http::default();
    for (route, kind, detail) in [
        ("/a", SourceErrorKind::AuthRejected, "HTTP 401"),
        ("/b", SourceErrorKind::AuthRejected, "HTTP 403"),
        ("/c", SourceErrorKind::Unreachable, "HTTP 404"),
        ("/d", SourceErrorKind::Unreachable, "HTTP 500"),
    ] {
        let failure = http
            .get_json::<Value>(&market.base_url(), route, &[], None)
            .await
            .expect_err(route);
        assert_eq!(failure.kind, kind, "{route}");
        assert_eq!(failure.detail.as_deref(), Some(detail), "{route}");
    }
}

#[tokio::test]
async fn broken_answers_are_bad_responses() {
    let market = TestMarketplace::start().await;
    market.raw("/broken", 200, "this is not json").await;
    market.raw("/missing", 200, r#"{"extensions": []}"#).await;
    #[derive(serde::Deserialize, Debug)]
    struct NeedsPagination {
        #[allow(dead_code)]
        pagination: Value,
    }
    let http = Http::default();
    let broken = http
        .get_json::<Value>(&market.base_url(), "/broken", &[], None)
        .await
        .expect_err("broken");
    assert_eq!(broken.kind, SourceErrorKind::BadResponse);
    let missing = http
        .get_json::<NeedsPagination>(&market.base_url(), "/missing", &[], None)
        .await
        .expect_err("missing field");
    assert_eq!(missing.kind, SourceErrorKind::BadResponse);
}

#[tokio::test]
async fn text_of_a_failed_answer_never_reaches_the_error() {
    // An authenticated marketplace may echo the login in its error text (SC-004).
    let secret = "Bearer s3cr3t-token-for-the-test";
    let market = TestMarketplace::start().await;
    market
        .json(
            "/leaky",
            500,
            json!({ "error": format!("bad header: {secret}") }),
        )
        .await;
    market
        .raw("/leaky-ok", 200, &format!("not json, but it says {secret}"))
        .await;
    let http = Http::default();
    for route in ["/leaky", "/leaky-ok"] {
        let failure = http
            .get_json::<Value>(&market.base_url(), route, &[], None)
            .await
            .expect_err(route);
        let shown = format!("{failure:?}");
        assert!(!shown.contains("s3cr3t"), "{route}: {shown}");
        assert!(
            !shown.contains(&market.origin()),
            "{route}: the address is not in the error"
        );
    }
}

#[tokio::test]
async fn a_server_that_never_answers_runs_into_the_timeout() {
    let market = TestMarketplace::start().await;
    market
        .delayed("/slow", json!({}), Duration::from_secs(30))
        .await;
    let http = Http::with_timeout(Duration::from_millis(150));
    let failure = http
        .get_json::<Value>(&market.base_url(), "/slow", &[], None)
        .await
        .expect_err("timeout");
    assert_eq!(failure.kind, SourceErrorKind::Timeout);
}

#[tokio::test]
async fn nothing_listening_is_unreachable() {
    // Port 1 has no listener; a port a test server just gave up could be taken by another test.
    let failure = Http::with_timeout(Duration::from_secs(2))
        .get_json::<Value>("http://127.0.0.1:1", "/health", &[], None)
        .await
        .expect_err("down");
    assert_eq!(failure.kind, SourceErrorKind::Unreachable);
}

#[tokio::test]
async fn a_login_is_never_sent_over_http() {
    // Not a loopback address: a debug build takes a login to one of those (research R15), so
    // nothing here could tell that a request went out; this host cannot even be resolved.
    let auth = AuthHeader::bearer("test-token");
    let failure = Http::default()
        .get_json::<Value>("http://marketplace.invalid", "/health", &[], Some(&auth))
        .await
        .expect_err("http with a login");
    assert_eq!(
        failure.kind,
        SourceErrorKind::InsecureCredentials,
        "refused before any lookup or request"
    );
}

#[tokio::test]
async fn a_redirect_without_a_login_is_followed() {
    let target = TestMarketplace::start().await;
    target.healthy().await;
    let source = TestMarketplace::start().await;
    source
        .redirect("/health", &format!("{}/health", target.origin()))
        .await;
    let answer = health(&source.base_url()).await.expect("followed");
    assert_eq!(answer, json!({ "status": "ok" }));
    assert_eq!(target.hits("/health").await, 1);
}

#[test]
fn a_redirect_chain_is_cut_at_five() {
    let first = url("https://a.example/x");
    let chain: Vec<Url> = (0..6)
        .map(|i| url(&format!("https://a.example/{i}")))
        .collect();
    assert!(redirect_allowed(&chain[..5], &first, false));
    assert!(!redirect_allowed(&chain, &first, false));
}

#[test]
fn a_redirect_never_goes_from_https_to_http() {
    let from = [url("https://a.example/x")];
    assert!(!redirect_allowed(&from, &url("http://a.example/y"), false));
    assert!(redirect_allowed(&from, &url("https://b.example/y"), false));
    let plain = [url("http://a.example/x")];
    assert!(redirect_allowed(&plain, &url("http://b.example/y"), false));
}

#[test]
fn with_a_login_a_redirect_stays_on_the_origin() {
    let from = [url("https://a.example/x")];
    assert!(redirect_allowed(&from, &url("https://a.example/y"), true));
    assert!(
        !redirect_allowed(&from, &url("https://b.example/y"), true),
        "other host"
    );
    assert!(
        !redirect_allowed(&from, &url("https://a.example:8443/y"), true),
        "other port"
    );
    let chain = [url("https://a.example/x"), url("https://a.example/y")];
    assert!(redirect_allowed(&chain, &url("https://a.example/z"), true));
    assert!(!redirect_allowed(
        &chain,
        &url("https://evil.example/z"),
        true
    ));
}

#[test]
fn the_login_header_holds_a_secret_and_shows_none() {
    let bearer = AuthHeader::bearer("tok");
    assert_eq!(bearer.0.as_str(), "Bearer tok");
    let basic = AuthHeader::basic("user", "pass");
    assert_eq!(basic.0.as_str(), "Basic dXNlcjpwYXNz");
    assert_eq!(format!("{basic:?}"), "AuthHeader(..)");
    let value = basic.header_value().expect("value");
    assert!(value.is_sensitive(), "reqwest keeps it out of its own logs");
}
