//! Loads a GGUF through llama.cpp: the backend of the process, the memory check, the chat template and the
//! tool-call format of the file (spec 070, plan D6).

use std::fs::File;
use std::io::BufReader;
use std::path::{Path, PathBuf};
use std::sync::{Arc, OnceLock};

use llama_cpp_2::llama_backend::LlamaBackend;
use llama_cpp_2::model::params::LlamaModelParams;
use llama_cpp_2::model::LlamaModel;
use llama_cpp_2::{list_llama_ggml_backend_devices, LlamaBackendDeviceType, LogOptions};

use super::cpu::{default_threads, missing_features};
use super::formats::{detect, ChatFormat};
use super::memory::{plan, FreeMemory, MemoryRefusal, ModelShape, Placement, DEFAULT_CONTEXT};
use crate::llm::local::gguf_header::read_summary;

/// More layers than any model has: everything goes on the GPU.
const ALL_LAYERS: u32 = 999;

/// What a load can say no with. The texts reach the user as they are.
#[derive(Debug, thiserror::Error)]
pub enum LoadError {
    #[error("gguf file not found: {0}")]
    NotFound(PathBuf),
    #[error("gguf header unreadable: {0}")]
    Header(String),
    #[error("llama.cpp could not start: {0}")]
    Backend(String),
    #[error("this processor lacks {}, which the llama.cpp engine of this build needs", .0.join(", "))]
    Cpu(Vec<&'static str>),
    #[error("not enough memory to load the model: {0}")]
    Memory(#[from] MemoryRefusal),
    #[error("llama.cpp could not load the model: {0}")]
    Load(String),
    #[error("the file has no chat template: {0}")]
    NoTemplate(String),
}

/// What the caller may decide about a load. Both stay `None` in production; the catalog's context window
/// goes in `context_window`, and the layer count is for tests and the evaluation.
#[derive(Debug, Clone, Copy, Default)]
pub struct LoadOptions {
    /// Context wanted, from the catalog; [`DEFAULT_CONTEXT`] without it.
    pub context_window: Option<u32>,
    /// Forces the number of layers on the GPU instead of letting the memory plan choose.
    pub gpu_layers: Option<u32>,
    /// Forces the number of CPU threads a generation uses instead of [`default_threads`].
    pub threads: Option<u32>,
}

/// The llama.cpp backend of this process. It may be initialised once only, and two engines or two loads
/// share it.
pub fn backend() -> Result<&'static LlamaBackend, LoadError> {
    static BACKEND: OnceLock<Result<LlamaBackend, String>> = OnceLock::new();
    BACKEND
        .get_or_init(|| {
            let backend = LlamaBackend::init().map_err(|e| e.to_string())?;
            llama_cpp_2::send_logs_to_tracing(LogOptions::default());
            Ok(backend)
        })
        .as_ref()
        .map_err(|e| LoadError::Backend(e.clone()))
}

/// A model in memory, with what chatting with it needs. Shared by every generation of one adapter.
pub struct LoadedModel {
    pub(super) model: LlamaModel,
    pub(super) template: String,
    pub(super) format: Option<&'static dyn ChatFormat>,
    pub(super) bos: String,
    pub(super) eos: String,
    pub(super) placement: Placement,
    pub(super) threads: u32,
    /// One generation at a time: a context is cheap to make but the weights and the device are shared.
    pub(super) gate: Arc<tokio::sync::Mutex<()>>,
}

impl LoadedModel {
    /// Loads `path`. Blocking and slow (seconds); call it off the async executor.
    pub fn load(path: &Path, options: LoadOptions) -> Result<Self, LoadError> {
        if !path.exists() {
            return Err(LoadError::NotFound(path.to_path_buf()));
        }
        let missing = missing_features();
        if !missing.is_empty() {
            return Err(LoadError::Cpu(missing));
        }
        let backend = backend()?;
        let placement = placement_for(path, options)?;

        let params = LlamaModelParams::default().with_n_gpu_layers(placement.gpu_layers);
        let model = LlamaModel::load_from_file(backend, path, &params)
            .map_err(|e| LoadError::Load(e.to_string()))?;
        let template = model
            .chat_template(None)
            .map_err(|e| LoadError::NoTemplate(format!("{e:?}")))?
            .to_string()
            .map_err(|e| LoadError::NoTemplate(e.to_string()))?;
        let vocab = model.vocab();
        let piece =
            |token| String::from_utf8_lossy(&vocab.token_to_piece(token, true, None)).into_owned();
        let (bos, eos) = (piece(vocab.bos()), piece(vocab.eos()));
        Ok(Self {
            format: detect(&template),
            model,
            template,
            bos,
            eos,
            placement,
            threads: options.threads.unwrap_or_else(default_threads),
            gate: Arc::new(tokio::sync::Mutex::new(())),
        })
    }

    /// The tool-call format of the file's chat template, `None` when the engine does not know its syntax.
    pub fn format(&self) -> Option<&'static dyn ChatFormat> {
        self.format
    }

    /// Where the model went and the longest context a generation may use.
    pub fn placement(&self) -> Placement {
        self.placement
    }
}

/// Reads the header, looks at what is free and plans; the forced layer count of `options` wins over the plan.
fn placement_for(path: &Path, options: LoadOptions) -> Result<Placement, LoadError> {
    let summary = File::open(path)
        .and_then(|file| read_summary(&mut BufReader::new(file)))
        .map_err(|e| LoadError::Header(e.to_string()))?;
    let file_bytes = std::fs::metadata(path)
        .map_err(|e| LoadError::Header(e.to_string()))?
        .len();
    let Some(shape) = ModelShape::from_header(&summary, file_bytes) else {
        // A file without a layer count cannot be sized; load it as asked and leave the rest to ggml.
        return Ok(Placement {
            gpu_layers: options.gpu_layers.unwrap_or(ALL_LAYERS),
            context: options.context_window.unwrap_or(DEFAULT_CONTEXT),
        });
    };
    let mut placed = plan(&shape, free_memory(), options.context_window)?;
    if let Some(layers) = options.gpu_layers {
        placed.gpu_layers = layers;
    }
    Ok(placed)
}

/// Free memory of the best GPU and of the host, as llama.cpp and the OS see them right now.
fn free_memory() -> FreeMemory {
    let device = list_llama_ggml_backend_devices()
        .into_iter()
        .filter(|d| {
            matches!(
                d.device_type,
                LlamaBackendDeviceType::Gpu | LlamaBackendDeviceType::IntegratedGpu
            )
        })
        .map(|d| d.memory_free as u64)
        .max();
    let mut system = sysinfo::System::new();
    system.refresh_memory();
    FreeMemory {
        device,
        host: system.available_memory(),
    }
}
