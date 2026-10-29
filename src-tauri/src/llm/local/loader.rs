//! GGUF loader that wraps [`mistralrs::GgufModelBuilder`] with the
//! configuration holzi needs and no more. See sibling `stream.rs` for the
//! cancellable streaming API used at chat time.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use mistralrs::core::GGUFArchitecture;
#[cfg(feature = "llm-cuda")]
use mistralrs::PagedAttentionMetaBuilder;
use mistralrs::{GgufModelBuilder, Model};
use thiserror::Error;

use super::gguf_header::read_architecture;

/// Loaded GGUF model, kept alive by the [`Arc`] so multiple in-flight
/// generations share the same weights and KV pools.
#[derive(Clone)]
pub struct LocalModel {
    inner: Arc<Model>,
    file_path: PathBuf,
}

/// Errors surfaced by the local inference path. All variants preserve
/// the underlying `mistralrs` message as a string — the source enum is
/// not `Clone`, and holzi's own error path shuttles messages across the
/// Tauri IPC boundary as `String` anyway.
#[derive(Debug, Error)]
pub enum LocalModelError {
    #[error("gguf file not found: {0}")]
    NotFound(PathBuf),
    #[error("gguf path has no parent directory: {0}")]
    MissingParent(PathBuf),
    #[error("gguf filename is not valid utf-8: {0}")]
    NonUtf8Filename(PathBuf),
    #[error("gguf header unreadable: {0}")]
    Header(String),
    #[error(
        "the model architecture `{0}` is not supported by the built-in inference engine (mistralrs 0.8.1)"
    )]
    UnsupportedArchitecture(String),
    #[error("mistralrs build error: {0}")]
    Build(String),
    #[cfg(feature = "llm-cuda")]
    #[error("paged-attention configuration failed: {0}")]
    PagedAttention(String),
}

impl LocalModel {
    /// Loads a GGUF from `file_path`. `tokenizer_repo` is a HuggingFace
    /// repo id like `"Qwen/Qwen2.5-0.5B-Instruct"`; it must be supplied
    /// when the GGUF has no embedded tokenizer (the common case for
    /// community quantizations).
    ///
    /// This is `async` because `mistralrs` performs weight decode inside
    /// the builder. Under CUDA the first load per host takes 30-45s
    /// while nvcc's JIT cache warms — Etappe 3 owns showing a dedicated
    /// UI state for that (Etappe 0 finding #4). Subsequent loads are
    /// ~4s on the same hardware.
    pub async fn load(
        file_path: &Path,
        tokenizer_repo: Option<&str>,
    ) -> Result<Self, LocalModelError> {
        if !file_path.exists() {
            return Err(LocalModelError::NotFound(file_path.to_path_buf()));
        }
        let dir = file_path
            .parent()
            .ok_or_else(|| LocalModelError::MissingParent(file_path.to_path_buf()))?;
        let filename = file_path
            .file_name()
            .and_then(|n| n.to_str())
            .ok_or_else(|| LocalModelError::NonUtf8Filename(file_path.to_path_buf()))?;

        let dir_str = dir
            .to_str()
            .ok_or_else(|| LocalModelError::NonUtf8Filename(dir.to_path_buf()))?;

        // mistralrs panics (and the load never returns) on an architecture it does not know.
        let architecture = std::fs::File::open(file_path)
            .and_then(|file| read_architecture(&mut std::io::BufReader::new(file)))
            .map_err(|e| LocalModelError::Header(e.to_string()))?;
        if GGUFArchitecture::from_value(&architecture).is_err() {
            return Err(LocalModelError::UnsupportedArchitecture(architecture));
        }

        let mut builder = GgufModelBuilder::new(dir_str, vec![filename.to_string()]);
        if let Some(tok) = tokenizer_repo {
            builder = builder.with_tok_model_id(tok.to_string());
        }

        // PagedAttention is a CUDA-only optimization; skip on CPU/Metal.
        #[cfg(feature = "llm-cuda")]
        {
            let cfg = PagedAttentionMetaBuilder::default()
                .build()
                .map_err(|e| LocalModelError::PagedAttention(e.to_string()))?;
            builder = builder.with_paged_attn(cfg);
        }

        let inner = builder
            .build()
            .await
            .map_err(|e| LocalModelError::Build(e.to_string()))?;

        Ok(Self {
            inner: Arc::new(inner),
            file_path: file_path.to_path_buf(),
        })
    }

    /// Path of the GGUF file this model was loaded from.
    pub fn file_path(&self) -> &Path {
        &self.file_path
    }

    /// Internal accessor for the streaming module. Not exported outside
    /// the crate — callers use [`crate::llm::local::GenerationHandle`].
    pub(super) fn inner(&self) -> Arc<Model> {
        Arc::clone(&self.inner)
    }
}
