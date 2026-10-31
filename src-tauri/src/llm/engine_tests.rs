use std::path::PathBuf;

use super::{engine_for, is_loadable, load_local, Engine, EngineError, LoadRequest};

/// Writes a GGUF header that declares only `general.architecture`.
fn write_header_only_gguf(name: &str, architecture: &str) -> PathBuf {
    let mut bytes = b"GGUF".to_vec();
    bytes.extend_from_slice(&3u32.to_le_bytes());
    bytes.extend_from_slice(&0u64.to_le_bytes());
    bytes.extend_from_slice(&1u64.to_le_bytes());
    let key = "general.architecture";
    bytes.extend_from_slice(&(key.len() as u64).to_le_bytes());
    bytes.extend_from_slice(key.as_bytes());
    bytes.extend_from_slice(&8u32.to_le_bytes());
    bytes.extend_from_slice(&(architecture.len() as u64).to_le_bytes());
    bytes.extend_from_slice(architecture.as_bytes());
    let path =
        std::env::temp_dir().join(format!("holzi-engine-{}-{name}.gguf", std::process::id()));
    std::fs::write(&path, bytes).unwrap();
    path
}

/// Spec 070 US5: with llama.cpp compiled in, it takes every architecture, the ones mistralrs knows too.
/// The test machines run the AVX2 baseline (`llm/llamacpp/cpu_tests.rs`).
#[cfg(feature = "llm-llamacpp")]
#[test]
fn llama_cpp_takes_every_architecture_where_it_runs() {
    for architecture in ["llama", "qwen2", "qwen3", "phi3", "qwen35", "gemma4"] {
        assert_eq!(
            engine_for(architecture).unwrap(),
            Engine::LlamaCpp,
            "{architecture}"
        );
        assert!(is_loadable(architecture));
    }
}

/// Without llama.cpp (iOS), mistralrs keeps every architecture it knows.
#[cfg(not(feature = "llm-llamacpp"))]
#[test]
fn without_llama_cpp_mistralrs_keeps_what_it_knows() {
    for architecture in ["llama", "qwen2", "qwen3", "phi3", "starcoder2"] {
        assert_eq!(
            engine_for(architecture).unwrap(),
            Engine::Mistralrs,
            "{architecture}"
        );
    }
}

#[cfg(not(feature = "llm-llamacpp"))]
#[test]
fn the_rest_is_refused_with_a_clear_message_without_llama_cpp() {
    for architecture in ["qwen35", "gemma4"] {
        let error = engine_for(architecture).unwrap_err();
        assert!(matches!(&error, EngineError::UnsupportedArchitecture(a) if a == architecture));
        assert!(error.to_string().contains(architecture));
        assert!(!is_loadable(architecture));
    }
}

#[tokio::test]
async fn a_missing_file_is_not_found() {
    let missing = PathBuf::from("/tmp/holzi-nonexistent-engine-model.gguf");
    let error = load_local(&missing, LoadRequest::default())
        .await
        .err()
        .unwrap();
    assert!(matches!(error, EngineError::NotFound(p) if p == missing));
}

#[tokio::test]
async fn a_file_that_is_not_gguf_is_a_header_error() {
    let path = std::env::temp_dir().join(format!("holzi-engine-{}-junk.gguf", std::process::id()));
    std::fs::write(&path, b"this is not a model").unwrap();
    let result = load_local(&path, LoadRequest::default()).await;
    let _ = std::fs::remove_file(&path);
    assert!(matches!(result.err().unwrap(), EngineError::Header(_)));
}

#[cfg(not(feature = "llm-llamacpp"))]
#[tokio::test]
async fn an_unknown_architecture_fails_at_once_instead_of_hanging() {
    let path = write_header_only_gguf("qwen35", "qwen35");
    let result = load_local(&path, LoadRequest::default()).await;
    let _ = std::fs::remove_file(&path);
    let error = result.err().unwrap();
    assert!(
        matches!(&error, EngineError::UnsupportedArchitecture(a) if a == "qwen35"),
        "{error:?}"
    );
}

#[cfg(feature = "llm-llamacpp")]
#[tokio::test]
async fn an_unknown_architecture_reaches_llama_cpp_which_refuses_a_file_without_weights() {
    // The header-only file names `qwen35`: the choice is llama.cpp, and its own load says no.
    let path = write_header_only_gguf("qwen35", "qwen35");
    let result = load_local(&path, LoadRequest::default()).await;
    let _ = std::fs::remove_file(&path);
    assert!(matches!(result.err().unwrap(), EngineError::LlamaCpp(_)));
}
