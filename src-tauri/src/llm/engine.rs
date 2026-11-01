//! Which inference engine loads a GGUF (spec 070, plan D1) and the one entry point that loads it.
//!
//! holzi has one local engine, llama.cpp (spec 070 US5, ADR 0023). mistralrs stays only for where llama.cpp
//! cannot run yet: a processor without the AVX2 baseline of the build, and iOS. There the choice follows
//! from `general.architecture` in the file's own header, not from a list in the catalog (FR-003), and an
//! architecture mistralrs does not know is refused at once (FR-004) instead of hanging inside mistralrs.

use std::path::Path;
use std::sync::Arc;

use mistralrs::core::GGUFArchitecture;

use crate::adapters::local::LocalAdapter;
use crate::adapters::ProviderAdapter;
use crate::llm::local::gguf_header::read_architecture;
use crate::llm::local::{LocalModel, LocalModelError};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Engine {
    Mistralrs,
    #[cfg(feature = "llm-llamacpp")]
    LlamaCpp,
}

impl Engine {
    pub fn name(self) -> &'static str {
        match self {
            Engine::Mistralrs => "mistralrs",
            #[cfg(feature = "llm-llamacpp")]
            Engine::LlamaCpp => "llama.cpp",
        }
    }
}

#[derive(Debug, thiserror::Error)]
pub enum EngineError {
    #[error("gguf file not found: {0}")]
    NotFound(std::path::PathBuf),
    #[error("gguf header unreadable: {0}")]
    Header(String),
    #[error(
        "the model architecture `{0}` is not supported by any inference engine built into this app"
    )]
    UnsupportedArchitecture(String),
    #[error("{0}")]
    Mistralrs(#[from] LocalModelError),
    #[cfg(feature = "llm-llamacpp")]
    #[error("{0}")]
    LlamaCpp(#[from] crate::llm::llamacpp::LoadError),
}

/// The engine for an architecture (spec 070 US5, ADR 0023): llama.cpp wherever it is compiled in and the
/// processor runs it; mistralrs only where llama.cpp cannot run (a processor without AVX2, a build without
/// the engine such as iOS), and there only for the architectures it knows.
pub fn engine_for(architecture: &str) -> Result<Engine, EngineError> {
    llama_cpp_here(architecture).or_else(|refusal| {
        if GGUFArchitecture::from_value(architecture).is_ok() {
            Ok(Engine::Mistralrs)
        } else {
            Err(refusal)
        }
    })
}

/// llama.cpp, if it is compiled in and the processor runs the ggml of this build (spec 070 R3); otherwise
/// why not.
#[cfg(feature = "llm-llamacpp")]
fn llama_cpp_here(_architecture: &str) -> Result<Engine, EngineError> {
    let missing = crate::llm::llamacpp::cpu::missing_features();
    if !missing.is_empty() {
        return Err(crate::llm::llamacpp::LoadError::Cpu(missing).into());
    }
    Ok(Engine::LlamaCpp)
}

#[cfg(not(feature = "llm-llamacpp"))]
fn llama_cpp_here(architecture: &str) -> Result<Engine, EngineError> {
    Err(EngineError::UnsupportedArchitecture(
        architecture.to_owned(),
    ))
}

/// Whether some compiled engine takes this architecture.
pub fn is_loadable(architecture: &str) -> bool {
    engine_for(architecture).is_ok()
}

/// What a load needs beyond the path.
#[derive(Debug, Clone, Default)]
pub struct LoadRequest<'a> {
    /// The tokenizer repository mistralrs falls back to when the file has no tokenizer; llama.cpp reads
    /// the tokenizer from the file and ignores it.
    pub tokenizer_repo: Option<&'a str>,
    /// The context the catalog wants, in tokens.
    pub context_window: Option<u32>,
}

/// Loads `path` with the engine its architecture selects. Slow: seconds to a minute.
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
    match engine {
        Engine::Mistralrs => {
            let model = LocalModel::load(path, request.tokenizer_repo).await?;
            Ok((Arc::new(LocalAdapter::new(model)), engine))
        }
        #[cfg(feature = "llm-llamacpp")]
        Engine::LlamaCpp => {
            let adapter = crate::llm::llamacpp::LlamaCppAdapter::load(
                path,
                crate::llm::llamacpp::LoadOptions {
                    context_window: request.context_window,
                    gpu_layers: None,
                    threads: None,
                },
            )
            .await?;
            Ok((Arc::new(adapter), engine))
        }
    }
}

#[cfg(test)]
#[path = "engine_tests.rs"]
mod tests;
