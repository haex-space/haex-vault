//! Spec 021 T089: the checks every request of the HTTP way passes first (contracts/http-auth.md,
//! research R14). The listener itself, the 1 MiB body and the session limit run in
//! tests/mcp_server_http.rs.

use hyper::http::{header, HeaderMap, HeaderValue};

use super::http::{host_allowed, origin_refused, too_large, MAX_BODY_BYTES};

#[test]
fn only_loopback_hosts_with_holzis_port_pass() {
    for host in [
        "127.0.0.1:4711",
        "[::1]:4711",
        "localhost:4711",
        "LOCALHOST:4711",
    ] {
        assert!(host_allowed(Some(host), 4711), "{host}");
    }
    for host in [
        None,
        Some("127.0.0.1"),
        Some("127.0.0.1:80"),
        Some("evil.example:4711"),
        Some("127.0.0.1.evil.example:4711"),
        Some("192.168.1.2:4711"),
        Some("0.0.0.0:4711"),
    ] {
        assert!(!host_allowed(host, 4711), "{host:?}");
    }
}

#[test]
fn an_origin_is_refused_on_mcp_and_the_token_endpoint_only() {
    assert!(origin_refused("/mcp", true));
    assert!(origin_refused("/oauth/token", true));
    assert!(!origin_refused("/mcp", false));
    // The authorization page is opened in the browser.
    assert!(!origin_refused("/oauth/authorize", true));
    assert!(!origin_refused("/oauth/authorize/wait", true));
}

#[test]
fn an_announced_body_over_one_mebibyte_is_refused() {
    let mut headers = HeaderMap::new();
    assert!(!too_large(&headers));
    headers.insert(
        header::CONTENT_LENGTH,
        HeaderValue::from(MAX_BODY_BYTES as u64),
    );
    assert!(!too_large(&headers));
    headers.insert(
        header::CONTENT_LENGTH,
        HeaderValue::from(MAX_BODY_BYTES as u64 + 1),
    );
    assert!(too_large(&headers));
}
