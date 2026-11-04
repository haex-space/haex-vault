//! T013/T014: `search_huggingface_models` / `get_huggingface_model_details`
//! contract tests against a wiremock Hub double.

use wiremock::matchers::{method, path, path_regex, query_param};
use wiremock::{Mock, MockServer, ResponseTemplate};

use super::huggingface::*;
use crate::error::HolziError;
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

#[tokio::test]
async fn search_rejects_a_too_short_query_without_any_http_call() {
    let server = MockServer::start().await;
    // No mock registered — a network call here would fail wiremock's
    // "no matching mock" and surface as a different error kind, proving
    // the short-circuit happened before any request was sent.
    let hf = HfClient::new(server.uri()).expect("client");
    let err = search_models(&hf, Some("x"), None, None)
        .await
        .expect_err("too short");
    assert!(matches!(err, HolziError::InvalidInput { .. }));
}

#[tokio::test]
async fn search_filters_out_repos_without_gguf_and_normalizes_hits() {
    let server = MockServer::start().await;
    let sha_a = "a".repeat(40);
    let sha_b = "b".repeat(40);
    Mock::given(method("GET"))
        .and(path("/api/models"))
        .and(query_param("search", "llama"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!([
            model_json(
                "owner/has-gguf",
                &sha_a,
                &["model.Q4_K_M.gguf", "README.md"]
            ),
            model_json("owner/no-gguf", &sha_b, &["model.safetensors"]),
        ])))
        .mount(&server)
        .await;

    let hf = HfClient::new(server.uri()).expect("client");
    let results = search_models(&hf, Some("llama"), None, None)
        .await
        .expect("search ok")
        .results;
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].repo_id, "owner/has-gguf");
    assert_eq!(results[0].files.len(), 1);
    assert_eq!(results[0].files[0].filename, "model.Q4_K_M.gguf");
    assert_eq!(results[0].license.as_deref(), Some("apache-2.0"));
}

#[tokio::test]
async fn search_returns_an_empty_list_for_no_hits() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/api/models"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!([])))
        .mount(&server)
        .await;

    let hf = HfClient::new(server.uri()).expect("client");
    let results = search_models(&hf, Some("nothing matches"), None, None)
        .await
        .expect("search ok")
        .results;
    assert!(results.is_empty());
}

#[tokio::test]
async fn search_caps_results_at_the_default_limit_of_20() {
    let server = MockServer::start().await;
    let hits: Vec<serde_json::Value> = (0..25)
        .map(|i| {
            model_json(
                &format!("owner/repo-{i:02}"),
                &"a".repeat(40),
                &["model.gguf"],
            )
        })
        .collect();
    Mock::given(method("GET"))
        .and(path("/api/models"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!(hits)))
        .mount(&server)
        .await;

    let hf = HfClient::new(server.uri()).expect("client");
    let results = search_models(&hf, Some("many"), None, None)
        .await
        .expect("search ok")
        .results;
    assert_eq!(results.len(), DEFAULT_LIMIT);
}

#[tokio::test]
async fn search_without_query_returns_the_top_ten_downloaded_gguf_repositories() {
    let server = MockServer::start().await;
    let hits: Vec<serde_json::Value> = (0..12)
        .map(|i| {
            let mut hit = model_json(
                &format!("owner/top-{i:02}"),
                &"a".repeat(40),
                &["model.gguf"],
            );
            hit["downloads"] = serde_json::json!(i);
            hit
        })
        .collect();
    Mock::given(method("GET"))
        .and(path("/api/models"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!(hits)))
        .mount(&server)
        .await;

    let hf = HfClient::new(server.uri()).expect("client");
    let results = search_models(&hf, None, None, None)
        .await
        .expect("top-model search ok")
        .results;
    assert_eq!(results.len(), DEFAULT_TOP_LIMIT);
    assert_eq!(results[0].repo_id, "owner/top-11");

    let results = search_models(&hf, None, Some(DEFAULT_LIMIT), None)
        .await
        .expect("top-model search remains capped")
        .results;
    assert_eq!(results.len(), DEFAULT_TOP_LIMIT);
}

#[tokio::test]
async fn search_maps_http_error_status_and_rate_limit() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/api/models"))
        .respond_with(ResponseTemplate::new(503))
        .mount(&server)
        .await;
    let hf = HfClient::new(server.uri()).expect("client");
    let err = search_models(&hf, Some("llama"), None, None)
        .await
        .expect_err("503");
    assert!(matches!(err, HolziError::HttpStatus { status: 503, .. }));
}

#[tokio::test]
async fn search_maps_429_to_rate_limited() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/api/models"))
        .respond_with(ResponseTemplate::new(429).insert_header("retry-after", "7"))
        .mount(&server)
        .await;
    let hf = HfClient::new(server.uri()).expect("client");
    let err = search_models(&hf, Some("llama"), None, None)
        .await
        .expect_err("429");
    match err {
        HolziError::RateLimited {
            retry_after_seconds,
        } => {
            assert_eq!(retry_after_seconds, Some(7));
        }
        other => panic!("expected RateLimited, got {other:?}"),
    }
}

#[tokio::test]
async fn details_rejects_a_repository_without_any_gguf_file() {
    let server = MockServer::start().await;
    let sha = "a".repeat(40);
    Mock::given(method("GET"))
        .and(path("/api/models/owner/no-gguf"))
        .respond_with(ResponseTemplate::new(200).set_body_json(model_json(
            "owner/no-gguf",
            &sha,
            &["model.safetensors"],
        )))
        .mount(&server)
        .await;
    let hf = HfClient::new(server.uri()).expect("client");
    let err = get_model_details(&hf, &generous_hardware(), "owner/no-gguf", None)
        .await
        .expect_err("no gguf");
    assert!(matches!(err, HolziError::UnsupportedFormat { .. }));
}

#[tokio::test]
async fn details_returns_multiple_files_with_size_and_fit_from_the_tree() {
    let server = MockServer::start().await;
    let sha = "a".repeat(40);
    Mock::given(method("GET"))
        .and(path("/api/models/owner/multi"))
        .respond_with(ResponseTemplate::new(200).set_body_json(model_json(
            "owner/multi",
            &sha,
            &["small.Q4_K_M.gguf", "huge.Q8_0.gguf", "tokenizer.json"],
        )))
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path(format!("/api/models/owner/multi/tree/{sha}")))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!([
            {"path": "small.Q4_K_M.gguf", "size": 1_000_000},
            {"path": "huge.Q8_0.gguf", "lfs": {"size": 999_999_999_999u64}},
        ])))
        .mount(&server)
        .await;

    let hf = HfClient::new(server.uri()).expect("client");
    let details = get_model_details(&hf, &tiny_hardware(), "owner/multi", None)
        .await
        .expect("details ok");
    assert_eq!(details.files.len(), 2);
    let small = details
        .files
        .iter()
        .find(|f| f.filename == "small.Q4_K_M.gguf")
        .unwrap();
    let huge = details
        .files
        .iter()
        .find(|f| f.filename == "huge.Q8_0.gguf")
        .unwrap();
    assert_eq!(small.size_bytes, Some(1_000_000));
    assert_eq!(huge.size_bytes, Some(999_999_999_999));
    assert_eq!(huge.fit, crate::hardware::Fit::TooBig);
    // Both files see the same repo-level tokenizer.json.
    assert_eq!(small.tokenizer_repo.as_deref(), Some("owner/multi"));
    assert!(!small.tokenizer_required);
}

#[tokio::test]
async fn details_missing_license_size_and_tokenizer_are_honestly_absent() {
    let server = MockServer::start().await;
    let sha = "a".repeat(40);
    let mut body = model_json("owner/sparse", &sha, &["model.gguf"]);
    body.as_object_mut().unwrap().remove("cardData");
    Mock::given(method("GET"))
        .and(path("/api/models/owner/sparse"))
        .respond_with(ResponseTemplate::new(200).set_body_json(body))
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path_regex(r"^/api/models/owner/sparse/tree/.*$"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!([])))
        .mount(&server)
        .await;

    let hf = HfClient::new(server.uri()).expect("client");
    let details = get_model_details(&hf, &generous_hardware(), "owner/sparse", None)
        .await
        .expect("details ok");
    assert_eq!(details.license, None);
    let file = &details.files[0];
    assert_eq!(file.size_bytes, None);
    assert_eq!(file.fit, crate::hardware::Fit::Unknown);
    assert!(file.tokenizer_required);
    assert_eq!(file.tokenizer_repo, None);
}

#[tokio::test]
async fn details_returns_exact_catalog_match_annotation() {
    let entry = crate::catalog::entries()
        .first()
        .expect("built-in catalog is never empty");
    let (repo_id, filename) = (entry.hf_repo.clone(), entry.hf_filename.clone());
    let server = MockServer::start().await;
    let sha = "a".repeat(40);
    Mock::given(method("GET"))
        .and(path(format!("/api/models/{repo_id}")))
        .respond_with(ResponseTemplate::new(200).set_body_json(model_json(
            &repo_id,
            &sha,
            &[filename.as_str()],
        )))
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path_regex(format!(r"^/api/models/{repo_id}/tree/.*$")))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!([])))
        .mount(&server)
        .await;

    let hf = HfClient::new(server.uri()).expect("client");
    let details = get_model_details(&hf, &generous_hardware(), &repo_id, None)
        .await
        .expect("details ok");
    assert!(details.files[0].catalog_match);
    assert_eq!(
        details.files[0].catalog_entry_id.as_deref(),
        Some(entry.id.as_str())
    );
}

#[tokio::test]
async fn search_asks_the_hub_for_gguf_and_for_chat_models_only_without_a_query() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/api/models"))
        .and(query_param("filter", "gguf"))
        .and(query_param("filter", "conversational"))
        .respond_with(
            ResponseTemplate::new(200).set_body_json(serde_json::json!([model_json(
                "owner/top",
                &"a".repeat(40),
                &["model.gguf"]
            )])),
        )
        .expect(1)
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/api/models"))
        .and(query_param("filter", "gguf"))
        .and(query_param("search", "llama"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!([])))
        .expect(1)
        .mount(&server)
        .await;

    let hf = HfClient::new(server.uri()).expect("client");
    let top = search_models(&hf, None, None, None).await.expect("top ok");
    assert_eq!(top.results.len(), 1);
    // A search with a term does not narrow to chat models.
    search_models(&hf, Some("llama"), None, None)
        .await
        .expect("search ok");
    let requests = server.received_requests().await.expect("requests");
    assert!(!requests[1]
        .url
        .query_pairs()
        .any(|(key, value)| key == "filter" && value == "conversational"));
}

#[tokio::test]
async fn search_hands_out_the_next_cursor_and_sends_it_back() {
    let server = MockServer::start().await;
    let next = format!(
        "<{}/api/models?filter=gguf&cursor=abc%3D%3D&limit=10>; rel=\"next\"",
        server.uri()
    );
    Mock::given(method("GET"))
        .and(path("/api/models"))
        .and(query_param("cursor", "abc=="))
        .respond_with(
            ResponseTemplate::new(200).set_body_json(serde_json::json!([model_json(
                "owner/second",
                &"b".repeat(40),
                &["model.gguf"]
            )])),
        )
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/api/models"))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header("link", next.as_str())
                .set_body_json(serde_json::json!([model_json(
                    "owner/first",
                    &"a".repeat(40),
                    &["model.gguf"]
                )])),
        )
        .mount(&server)
        .await;

    let hf = HfClient::new(server.uri()).expect("client");
    let first = search_models(&hf, None, None, None).await.expect("page 1");
    assert_eq!(first.results[0].repo_id, "owner/first");
    assert_eq!(first.next_cursor.as_deref(), Some("abc=="));

    let second = search_models(&hf, None, None, first.next_cursor.as_deref())
        .await
        .expect("page 2");
    assert_eq!(second.results[0].repo_id, "owner/second");
    assert_eq!(second.next_cursor, None);
}

#[tokio::test]
async fn search_ends_the_list_without_a_next_link() {
    for link in [
        "<https://huggingface.co/api/models?cursor=old>; rel=\"prev\"",
        "not a link header",
        "<not a url>; rel=\"next\"",
    ] {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/api/models"))
            .respond_with(
                ResponseTemplate::new(200)
                    .insert_header("link", link)
                    .set_body_json(serde_json::json!([model_json(
                        "owner/only",
                        &"a".repeat(40),
                        &["model.gguf"]
                    )])),
            )
            .mount(&server)
            .await;
        let hf = HfClient::new(server.uri()).expect("client");
        let page = search_models(&hf, None, None, None).await.expect("page");
        assert_eq!(page.results.len(), 1);
        assert_eq!(page.next_cursor, None, "{link}");
    }
}
