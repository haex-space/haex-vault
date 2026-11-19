//! Installation-side Hub contract tests against a wiremock double:
//! revision resolution, install preview, tracked-ref update checks and
//! the range-request GGUF header fetch.

use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

use super::huggingface::*;
use crate::error::HaexError;
use crate::hardware::{Backend, HardwareInfo};

fn generous_hardware() -> HardwareInfo {
    HardwareInfo {
        backend: Backend::Cpu,
        total_ram_bytes: 64 * 1024 * 1024 * 1024,
        available_ram_bytes: 64 * 1024 * 1024 * 1024,
        vram_bytes: None,
    }
}

fn tiny_hardware() -> HardwareInfo {
    HardwareInfo {
        backend: Backend::Cpu,
        total_ram_bytes: 512 * 1024 * 1024,
        available_ram_bytes: 256 * 1024 * 1024,
        vram_bytes: None,
    }
}

fn model_json(id: &str, sha: &str, files: &[&str]) -> serde_json::Value {
    serde_json::json!({
        "id": id,
        "author": "someone",
        "downloads": 42,
        "gated": false,
        "private": false,
        "sha": sha,
        "cardData": { "license": "apache-2.0" },
        "siblings": files.iter().map(|f| serde_json::json!({"rfilename": f})).collect::<Vec<_>>(),
    })
}

// ---------------------------------------------------------------------
// resolve_revision / preview_install
// ---------------------------------------------------------------------

#[tokio::test]
async fn resolve_revision_resolves_a_branch_name_to_a_commit_sha_and_tracks_the_ref() {
    let server = MockServer::start().await;
    let sha = "c".repeat(40);
    Mock::given(method("GET"))
        .and(path("/api/models/owner/name/revision/main"))
        .respond_with(ResponseTemplate::new(200).set_body_json(model_json(
            "owner/name",
            &sha,
            &["model.gguf"],
        )))
        .mount(&server)
        .await;
    let hf = HfClient::new(server.uri()).expect("client");
    let resolved = resolve_revision(&hf, "owner/name", Some("main"))
        .await
        .expect("resolve ok");
    assert_eq!(resolved.sha, sha);
    assert_eq!(resolved.revision_ref.as_deref(), Some("main"));
}

#[tokio::test]
async fn resolve_revision_with_no_input_defaults_to_main_and_tracks_it() {
    let server = MockServer::start().await;
    let sha = "d".repeat(40);
    Mock::given(method("GET"))
        .and(path("/api/models/owner/name"))
        .respond_with(ResponseTemplate::new(200).set_body_json(model_json(
            "owner/name",
            &sha,
            &["model.gguf"],
        )))
        .mount(&server)
        .await;
    let hf = HfClient::new(server.uri()).expect("client");
    let resolved = resolve_revision(&hf, "owner/name", None)
        .await
        .expect("resolve ok");
    assert_eq!(resolved.sha, sha);
    assert_eq!(resolved.revision_ref.as_deref(), Some("main"));
}

#[tokio::test]
async fn resolve_revision_direct_sha_pin_does_not_track_a_ref() {
    let server = MockServer::start().await;
    let sha = "e".repeat(40);
    Mock::given(method("GET"))
        .and(path(format!("/api/models/owner/name/revision/{sha}")))
        .respond_with(ResponseTemplate::new(200).set_body_json(model_json(
            "owner/name",
            &sha,
            &["model.gguf"],
        )))
        .mount(&server)
        .await;
    let hf = HfClient::new(server.uri()).expect("client");
    let resolved = resolve_revision(&hf, "owner/name", Some(&sha))
        .await
        .expect("resolve ok");
    assert_eq!(resolved.sha, sha);
    assert_eq!(resolved.revision_ref, None);
}

#[tokio::test]
async fn resolve_revision_offline_error_surfaces_as_network_error() {
    // No server started at all — connecting must fail as `Network`, not
    // panic or hang.
    let hf = HfClient::new("http://127.0.0.1:1").expect("client");
    let err = resolve_revision(&hf, "owner/name", Some("main"))
        .await
        .expect_err("offline");
    assert!(matches!(err, HaexError::Network { .. }));
}

#[tokio::test]
async fn preview_install_flags_too_big_and_missing_tokenizer_requires_confirmation() {
    let server = MockServer::start().await;
    let sha = "f".repeat(40);
    Mock::given(method("GET"))
        .and(path("/api/models/owner/name"))
        .respond_with(ResponseTemplate::new(200).set_body_json(model_json(
            "owner/name",
            &sha,
            &["model.Q8_0.gguf"],
        )))
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path(format!("/api/models/owner/name/revision/{sha}")))
        .respond_with(ResponseTemplate::new(200).set_body_json(model_json(
            "owner/name",
            &sha,
            &["model.Q8_0.gguf"],
        )))
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path(format!("/api/models/owner/name/tree/{sha}")))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!([
            {"path": "model.Q8_0.gguf", "size": 900_000_000_000u64},
        ])))
        .mount(&server)
        .await;

    let hf = HfClient::new(server.uri()).expect("client");
    let preview = preview_install(
        &hf,
        &tiny_hardware(),
        "owner/name",
        "model.Q8_0.gguf",
        None,
        None,
        None,
    )
    .await
    .expect("preview ok");
    assert_eq!(preview.fit, crate::hardware::Fit::TooBig);
    assert!(preview.requires_explicit_too_big_confirmation);
    assert!(preview.tokenizer_required);
    assert_eq!(preview.revision, sha);
    assert_eq!(preview.revision_ref.as_deref(), Some("main"));
    assert_eq!(
        preview.model_id,
        derive_model_id("owner/name", "model.Q8_0.gguf")
    );
}

#[tokio::test]
async fn preview_install_rejects_a_filename_not_present_in_the_repository() {
    let server = MockServer::start().await;
    let sha = "1".repeat(40);
    Mock::given(method("GET"))
        .and(path("/api/models/owner/name"))
        .respond_with(ResponseTemplate::new(200).set_body_json(model_json(
            "owner/name",
            &sha,
            &["other.gguf"],
        )))
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path(format!("/api/models/owner/name/revision/{sha}")))
        .respond_with(ResponseTemplate::new(200).set_body_json(model_json(
            "owner/name",
            &sha,
            &["other.gguf"],
        )))
        .mount(&server)
        .await;
    let hf = HfClient::new(server.uri()).expect("client");
    let err = preview_install(
        &hf,
        &generous_hardware(),
        "owner/name",
        "model.gguf",
        None,
        None,
        None,
    )
    .await
    .expect_err("missing file");
    assert!(matches!(err, HaexError::InvalidInput { .. }));
}

// ---------------------------------------------------------------------
// check_update
// ---------------------------------------------------------------------

#[tokio::test]
async fn check_update_returns_the_current_sha_for_the_tracked_ref() {
    let server = MockServer::start().await;
    let sha = "2".repeat(40);
    Mock::given(method("GET"))
        .and(path("/api/models/owner/name/revision/main"))
        .respond_with(ResponseTemplate::new(200).set_body_json(model_json(
            "owner/name",
            &sha,
            &["model.gguf"],
        )))
        .mount(&server)
        .await;
    let hf = HfClient::new(server.uri()).expect("client");
    let latest = check_update(&hf, "owner/name", "main")
        .await
        .expect("check ok");
    assert_eq!(latest, sha);
}

#[tokio::test]
async fn check_update_http_error_does_not_panic_and_is_retryable() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/api/models/owner/name/revision/main"))
        .respond_with(ResponseTemplate::new(404))
        .mount(&server)
        .await;
    let hf = HfClient::new(server.uri()).expect("client");
    let err = check_update(&hf, "owner/name", "main")
        .await
        .expect_err("404");
    assert!(matches!(err, HaexError::HttpStatus { status: 404, .. }));
}

// ---------------------------------------------------------------------
// Range-request header fetch
// ---------------------------------------------------------------------

#[tokio::test]
async fn fetch_header_bytes_sends_a_range_request_and_returns_the_body() {
    let server = MockServer::start().await;
    let sha = "3".repeat(40);
    Mock::given(method("GET"))
        .and(path(format!("/owner/name/resolve/{sha}/model.gguf")))
        .respond_with(ResponseTemplate::new(206).set_body_bytes(b"GGUF-HEADER-BYTES".to_vec()))
        .mount(&server)
        .await;
    let hf = HfClient::new(server.uri()).expect("client");
    let bytes = hf
        .fetch_header_bytes("owner/name", &sha, "model.gguf", 4096)
        .await
        .expect("range fetch ok");
    assert_eq!(bytes, b"GGUF-HEADER-BYTES".to_vec());
}

#[tokio::test]
async fn fetch_header_bytes_rejects_a_full_response_that_ignored_the_range() {
    let server = MockServer::start().await;
    let sha = "4".repeat(40);
    Mock::given(method("GET"))
        .and(path(format!("/owner/name/resolve/{sha}/model.gguf")))
        .respond_with(ResponseTemplate::new(200).set_body_bytes(b"too much body".to_vec()))
        .mount(&server)
        .await;
    let hf = HfClient::new(server.uri()).expect("client");

    let err = hf
        .fetch_header_bytes("owner/name", &sha, "model.gguf", 4)
        .await
        .expect_err("full response must not be accepted as a range response");
    assert!(matches!(err, HaexError::HttpStatus { status: 200, .. }));
}

#[tokio::test]
async fn fetch_header_bytes_rejects_a_partial_response_over_the_header_limit() {
    let server = MockServer::start().await;
    let sha = "5".repeat(40);
    Mock::given(method("GET"))
        .and(path(format!("/owner/name/resolve/{sha}/model.gguf")))
        .respond_with(ResponseTemplate::new(206).set_body_bytes(b"too much body".to_vec()))
        .mount(&server)
        .await;
    let hf = HfClient::new(server.uri()).expect("client");

    let err = hf
        .fetch_header_bytes("owner/name", &sha, "model.gguf", 4)
        .await
        .expect_err("oversized range response must be rejected");
    assert!(matches!(err, HaexError::HttpStatus { status: 206, .. }));
}
