//! Offline tests for the Hugging Face boundary: repository/revision/
//! filename validation, download-URL host pinning, quantization and
//! tokenizer normalization, deterministic model ids, and the GGUF header
//! parser. No HTTP double is needed for any of these.
//!
//! The Hub contract tests live in `huggingface_search_tests.rs` (search /
//! details) and `huggingface_install_tests.rs` (revision resolution,
//! install preview, update check, range fetch).

use super::*;

// ---------------------------------------------------------------------
// T005: repo id / revision / filename / host validation
// ---------------------------------------------------------------------

#[test]
fn validate_repo_id_accepts_owner_slash_name() {
    assert!(validate_repo_id("bartowski/Llama-3.2-1B-GGUF").is_ok());
}

#[test]
fn validate_repo_id_rejects_missing_or_extra_slashes() {
    assert!(validate_repo_id("no-slash-here").is_err());
    assert!(validate_repo_id("too/many/slashes").is_err());
}

#[test]
fn validate_repo_id_rejects_empty_segment_and_traversal() {
    assert!(validate_repo_id("/name").is_err());
    assert!(validate_repo_id("owner/").is_err());
    assert!(validate_repo_id("owner/..").is_err());
    assert!(validate_repo_id("../owner").is_err());
}

#[test]
fn validate_repo_id_rejects_control_chars_and_whitespace() {
    assert!(validate_repo_id("owner/na\nme").is_err());
    assert!(validate_repo_id("owner/na me").is_err());
    assert!(validate_repo_id("").is_err());
}

#[test]
fn validate_revision_ref_accepts_branch_tag_and_sha() {
    assert!(validate_revision_ref("main").is_ok());
    assert!(validate_revision_ref("v1.2.3").is_ok());
    assert!(validate_revision_ref("a".repeat(40).as_str()).is_ok());
}

#[test]
fn validate_revision_ref_rejects_traversal_and_control_chars() {
    assert!(validate_revision_ref("").is_err());
    assert!(validate_revision_ref("..").is_err());
    assert!(validate_revision_ref("has whitespace").is_err());
    assert!(validate_revision_ref("has\ttab").is_err());
    assert!(validate_revision_ref("has/slash").is_err());
    assert!(validate_revision_ref("has\\backslash").is_err());
}

#[test]
fn is_commit_sha_requires_exactly_40_lowercase_hex_chars() {
    assert!(is_commit_sha(&"a".repeat(40)));
    assert!(is_commit_sha(
        &"0123456789abcdef0123456789abcdef01234567"[..40]
    ));
    assert!(
        !is_commit_sha(&"A".repeat(40)),
        "uppercase must be rejected"
    );
    assert!(!is_commit_sha(&"a".repeat(39)), "too short");
    assert!(!is_commit_sha(&"a".repeat(41)), "too long");
    assert!(!is_commit_sha("main"));
}

#[test]
fn validate_gguf_filename_accepts_gguf_and_rejects_other_formats() {
    assert!(validate_gguf_filename("model.Q4_K_M.gguf").is_ok());
    let err = validate_gguf_filename("model.safetensors").expect_err("non-gguf must be rejected");
    assert!(matches!(err, HaexError::UnsupportedFormat { .. }));
}

#[test]
fn validate_gguf_filename_rejects_path_traversal_and_control_chars() {
    assert!(validate_gguf_filename("../../etc/passwd.gguf").is_err());
    assert!(validate_gguf_filename("../evil.gguf").is_err());
    assert!(validate_gguf_filename("").is_err());
    assert!(validate_gguf_filename(".hidden.gguf").is_err());
}

#[test]
fn resolve_download_url_requires_a_resolved_commit_sha() {
    let sha = "a".repeat(40);
    let ok = resolve_download_url(DEFAULT_BASE_URL, "owner/name", &sha, "model.gguf");
    assert!(ok.is_ok());
    assert_eq!(
        ok.unwrap(),
        format!("https://huggingface.co/owner/name/resolve/{sha}/model.gguf")
    );

    let rejected = resolve_download_url(DEFAULT_BASE_URL, "owner/name", "main", "model.gguf");
    assert!(
        rejected.is_err(),
        "a mutable ref must not reach the download URL"
    );
}

#[test]
fn resolve_download_url_only_accepts_the_real_huggingface_host() {
    let sha = "a".repeat(40);
    assert!(
        resolve_download_url("http://huggingface.co", "owner/name", &sha, "model.gguf").is_err()
    );
    assert!(
        resolve_download_url("https://evil.example.com", "owner/name", &sha, "model.gguf").is_err()
    );
}

#[test]
fn resolve_download_url_host_check_is_not_fooled_by_userinfo() {
    // `https://huggingface.co@evil.example.com/...` parses to host
    // `evil.example.com` — a naive `starts_with("https://huggingface.co")`
    // string check would be fooled by this; the real `Url` parser is not.
    let sha = "a".repeat(40);
    let result = resolve_download_url(
        "https://huggingface.co@evil.example.com",
        "owner/name",
        &sha,
        "model.gguf",
    );
    assert!(result.is_err());
}

// ---------------------------------------------------------------------
// T006: quantization normalization, tokenizer resolution, dedup/sort
// ---------------------------------------------------------------------

#[test]
fn normalize_quantization_prefers_the_longest_boundary_match() {
    assert_eq!(
        normalize_quantization_from_filename("model-Q3_K_M.gguf").as_deref(),
        Some("Q3_K_M")
    );
    assert_eq!(
        normalize_quantization_from_filename("model-Q4_K_S.gguf").as_deref(),
        Some("Q4_K_S")
    );
    assert_eq!(
        normalize_quantization_from_filename("model-Q8_0.gguf").as_deref(),
        Some("Q8_0")
    );
    assert_eq!(
        normalize_quantization_from_filename("model-f16.gguf").as_deref(),
        Some("F16")
    );
}

#[test]
fn normalize_quantization_returns_none_when_no_known_token_present() {
    assert_eq!(normalize_quantization_from_filename("model.gguf"), None);
}

#[test]
fn normalize_quantization_does_not_match_inside_a_larger_word() {
    // "f16" must not spuriously match inside an unrelated token like
    // "af1600" — the boundary check is what prevents that.
    assert_eq!(normalize_quantization_from_filename("af1600.gguf"), None);
}

#[test]
fn resolve_tokenizer_repo_uses_same_repo_only_when_tokenizer_json_present() {
    let with_tokenizer = vec!["model.gguf".to_string(), "tokenizer.json".to_string()];
    assert_eq!(
        resolve_tokenizer_repo("owner/name", &with_tokenizer),
        (Some("owner/name".to_string()), false)
    );

    let without_tokenizer = vec!["model.gguf".to_string()];
    assert_eq!(
        resolve_tokenizer_repo("owner/name", &without_tokenizer),
        (None, true)
    );
}

fn sample_result(repo_id: &str, downloads: Option<u64>) -> HuggingFaceModelResult {
    HuggingFaceModelResult {
        repo_id: repo_id.to_string(),
        display_name: repo_id.to_string(),
        author: None,
        license: None,
        downloads,
        files: vec![],
        source_revision: Some("a".repeat(40)),
        revision_ref: None,
    }
}

#[test]
fn dedupe_and_sort_removes_duplicates_and_orders_deterministically() {
    let mut results = vec![
        sample_result("b/low", Some(1)),
        sample_result("a/high", Some(100)),
        sample_result("a/high", Some(100)), // duplicate repoId, dropped
        sample_result("c/none", None),
        sample_result("b/also_none", None),
    ];
    dedupe_and_sort(&mut results);
    let ids: Vec<&str> = results.iter().map(|r| r.repo_id.as_str()).collect();
    // Descending downloads first, then ascending repoId as tie-breaker
    // (None treated as 0, so "c/none" sorts before "b/also_none").
    assert_eq!(ids, vec!["a/high", "b/low", "b/also_none", "c/none"]);
}

// ---------------------------------------------------------------------
// Deterministic model id
// ---------------------------------------------------------------------

#[test]
fn derive_model_id_is_deterministic_and_prefixed() {
    let a = derive_model_id("owner/name", "model.gguf");
    let b = derive_model_id("owner/name", "model.gguf");
    assert_eq!(a, b);
    assert!(a.starts_with("hf-"));
    assert_eq!(a.len(), 3 + 64);
}

#[test]
fn derive_model_id_length_prefixing_prevents_boundary_collisions() {
    // Naive concatenation would make ("ab", "c") collide with ("a", "bc").
    let first = derive_model_id("ab", "c");
    let second = derive_model_id("a", "bc");
    assert_ne!(first, second);
}

#[test]
fn derive_model_id_differs_for_different_filenames_in_same_repo() {
    let a = derive_model_id("owner/name", "model-q4.gguf");
    let b = derive_model_id("owner/name", "model-q8.gguf");
    assert_ne!(a, b);
}

// ---------------------------------------------------------------------
// GGUF header parsing (research.md Entscheidung 9)
// ---------------------------------------------------------------------

/// Builds a minimal synthetic GGUF v3 header buffer with one metadata KV
/// pair for `llama.context_length` = `value` (uint32), preceded by an
/// unrelated array-typed KV pair so the parser has to correctly skip a
/// nested array before reaching the key under test.
fn synthetic_gguf_header(context_length: u32) -> Vec<u8> {
    let mut buf = Vec::new();
    buf.extend_from_slice(b"GGUF");
    buf.extend_from_slice(&3u32.to_le_bytes()); // version
    buf.extend_from_slice(&0u64.to_le_bytes()); // tensor_count
    buf.extend_from_slice(&2u64.to_le_bytes()); // kv_count

    // KV 1: an array of 3 uint8 values under an unrelated key, to prove
    // array-skipping advances the cursor correctly.
    push_gguf_string(&mut buf, "unrelated.array");
    buf.extend_from_slice(&9u32.to_le_bytes()); // type = ARRAY
    buf.extend_from_slice(&0u32.to_le_bytes()); // element type = UINT8
    buf.extend_from_slice(&3u64.to_le_bytes()); // element count
    buf.extend_from_slice(&[1u8, 2u8, 3u8]);

    // KV 2: the key under test.
    push_gguf_string(&mut buf, "llama.context_length");
    buf.extend_from_slice(&4u32.to_le_bytes()); // type = UINT32
    buf.extend_from_slice(&context_length.to_le_bytes());

    buf
}

fn push_gguf_string(buf: &mut Vec<u8>, s: &str) {
    buf.extend_from_slice(&(s.len() as u64).to_le_bytes());
    buf.extend_from_slice(s.as_bytes());
}

#[test]
fn parse_gguf_context_length_finds_the_key_after_skipping_an_array() {
    let buf = synthetic_gguf_header(8192);
    assert_eq!(parse_gguf_context_length(&buf), Some(8192));
}

#[test]
fn parse_gguf_context_length_rejects_wrong_magic() {
    let mut buf = synthetic_gguf_header(4096);
    buf[0..4].copy_from_slice(b"NOPE");
    assert_eq!(parse_gguf_context_length(&buf), None);
}

#[test]
fn parse_gguf_context_length_handles_truncated_buffer_safely() {
    let buf = synthetic_gguf_header(4096);
    // Cut the buffer off mid-header — must return None, never panic.
    for cut in [4usize, 8, 16, 24, 30, buf.len() - 1] {
        assert_eq!(parse_gguf_context_length(&buf[..cut]), None);
    }
}

#[test]
fn parse_gguf_context_length_returns_none_when_key_absent() {
    let mut buf = Vec::new();
    buf.extend_from_slice(b"GGUF");
    buf.extend_from_slice(&3u32.to_le_bytes());
    buf.extend_from_slice(&0u64.to_le_bytes());
    buf.extend_from_slice(&1u64.to_le_bytes());
    push_gguf_string(&mut buf, "some.other_key");
    buf.extend_from_slice(&4u32.to_le_bytes());
    buf.extend_from_slice(&42u32.to_le_bytes());
    assert_eq!(parse_gguf_context_length(&buf), None);
}
