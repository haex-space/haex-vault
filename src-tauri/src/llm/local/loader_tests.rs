//! Unit tests that do not require a real GGUF on disk. Tests that
//! actually load and generate live in `src-tauri/tests/local_inference.rs`
//! and are gated on the `HOLZI_TEST_GGUF` env var.

use super::{LocalModel, LocalModelError};
use std::path::PathBuf;

#[tokio::test]
async fn load_returns_not_found_for_missing_path() {
    // `LocalModel` wraps a `mistralrs::Model` which does not implement
    // `Debug`, so we cannot use `unwrap_err`. Match the two arms directly.
    let missing = PathBuf::from("/tmp/holzi-nonexistent-model.gguf");
    match LocalModel::load(&missing, None).await {
        Err(LocalModelError::NotFound(p)) => assert_eq!(p, missing),
        Err(other) => panic!("expected NotFound, got: {other:?}"),
        Ok(_) => panic!("expected error, got Ok"),
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
    let path = std::env::temp_dir().join(format!("holzi-{}-{name}.gguf", std::process::id()));
    std::fs::write(&path, bytes).unwrap();
    path
}

#[tokio::test]
async fn load_refuses_an_unknown_architecture_instead_of_hanging() {
    // mistralrs would panic inside the build on `qwen35` and the load would never return.
    let path = write_header_only_gguf("qwen35", "qwen35");
    let result = LocalModel::load(&path, None).await;
    let _ = std::fs::remove_file(&path);
    match result {
        Err(LocalModelError::UnsupportedArchitecture(arch)) => assert_eq!(arch, "qwen35"),
        Err(other) => panic!("expected UnsupportedArchitecture, got: {other:?}"),
        Ok(_) => panic!("expected error, got Ok"),
    }
}
