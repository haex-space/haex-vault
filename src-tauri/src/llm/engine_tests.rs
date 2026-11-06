use std::path::PathBuf;

use super::{engine_for, is_loadable, load_local, Engine, EngineError, LoadRequest};

#[test]
fn llama_cpp_is_the_only_engine_for_every_architecture() {
    for architecture in ["llama", "qwen2", "qwen3", "phi3", "qwen35", "gemma4"] {
        if crate::llm::llamacpp::cpu::missing_features().is_empty() {
            assert_eq!(engine_for(architecture).unwrap(), Engine::LlamaCpp);
            assert!(is_loadable(architecture));
        } else {
            assert!(engine_for(architecture).is_err(), "{architecture}");
            assert!(!is_loadable(architecture));
        }
    }
}

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

#[cfg(feature = "llm-llamacpp")]
#[tokio::test]
async fn llama_cpp_rejects_a_header_only_file_without_model_weights() {
    let path = write_header_only_gguf("qwen35", "qwen35");
    let result = load_local(&path, LoadRequest::default()).await;
    let _ = std::fs::remove_file(&path);
    assert!(matches!(result.err().unwrap(), EngineError::LlamaCpp(_)));
}
