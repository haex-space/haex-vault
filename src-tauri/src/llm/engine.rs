//! The local inference engine and the one entry point that loads a GGUF.

use std::path::Path;
use std::sync::Arc;

use crate::adapters::ProviderAdapter;
use crate::llm::gguf_header::read_architecture;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Engine {
    LlamaCpp,
}

impl Engine {
    pub fn name(self) -> &'static str {
        "llama.cpp"
    }
}

#[derive(Debug, thiserror::Error)]
pub enum EngineError {
    #[error("gguf file not found: {0}")]
    NotFound(std::path::PathBuf),
    #[error("gguf header unreadable: {0}")]
    Header(String),
    #[error("{0}")]
    LlamaCpp(#[from] crate::llm::llamacpp::LoadError),
}

/// The only local engine is llama.cpp. It is loadable on this processor when the compiled ggml
/// backend's required CPU features are present. The architecture itself is validated by llama.cpp
/// when the model is loaded.
pub fn engine_for(_architecture: &str) -> Result<Engine, EngineError> {
    let missing = crate::llm::llamacpp::cpu::missing_features();
    if !missing.is_empty() {
        return Err(crate::llm::llamacpp::LoadError::Cpu(missing).into());
    }
    Ok(Engine::LlamaCpp)
}

/// Whether this processor can run the compiled llama.cpp backend.
pub fn is_loadable(architecture: &str) -> bool {
    engine_for(architecture).is_ok()
}

/// What a load needs beyond the path.
#[derive(Debug, Clone, Default)]
pub struct LoadRequest<'a> {
    /// The context the catalog wants, in tokens.
    pub context_window: Option<u32>,
    /// The model's installed MTP drafter (spec 079).
    pub drafter: Option<&'a Path>,
    /// The context length a person chose for the model (spec 086).
    pub choice: crate::llm::llamacpp::choice::ContextChoice,
    /// The KV cache type a person chose.
    pub cache: crate::llm::llamacpp::choice::CacheType,
}

/// Loads `path` with llama.cpp. Slow: seconds to a minute.
pub async fn load_local(
    path: &Path,
    request: LoadRequest<'_>,
) -> Result<(Arc<dyn ProviderAdapter>, Engine), EngineError> {
    if !path.exists() {
        return Err(EngineError::NotFound(path.to_path_buf()));
    }
    let architecture = std::fs::File::open(path)
        .and_then(|file| read_architecture(&mut std::io::BufReader::new(file)))
        .map_err(|e| EngineError::Header(e.to_string()))?;
    let engine = engine_for(&architecture)?;
    log::info!("loading {} with {}", path.display(), engine.name());
    let adapter = crate::llm::llamacpp::LlamaCppAdapter::load(
        path,
        crate::llm::llamacpp::LoadOptions {
            context_window: request.context_window,
            gpu_layers: None,
            threads: None,
            drafter: request.drafter.map(Path::to_path_buf),
            choice: request.choice,
            cache: request.cache,
        },
    )
    .await?;
    Ok((Arc::new(adapter), engine))
}

#[cfg(test)]
#[path = "engine_tests.rs"]
mod tests;
