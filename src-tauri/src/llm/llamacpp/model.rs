//! Loads a GGUF through llama.cpp: the backend of the process, the memory check, the chat template and the
//! tool-call format of the file (spec 070, plan D6).

use std::fs::File;
use std::io::BufReader;
use std::num::NonZeroU32;
use std::path::{Path, PathBuf};
use std::sync::{Arc, OnceLock};

use llama_cpp_2::context::params::LlamaContextParams;
use llama_cpp_2::llama_backend::LlamaBackend;
use llama_cpp_2::model::params::LlamaModelParams;
use llama_cpp_2::model::LlamaModel;
use llama_cpp_2::{list_llama_ggml_backend_devices, LlamaBackendDeviceType, LogOptions};

use super::choice::{CacheType, ContextChoice, PlanError};
use super::cpu::{default_threads, missing_features};
use super::formats::{detect, ChatFormat};
#[cfg(not(any(target_os = "android", target_os = "ios")))]
use super::memory::DrafterShape;
use super::memory::{
    plan_with, FreeMemory, ModelShape, Placement, DEFAULT_CONTEXT, TARGET_CONTEXT,
};
use crate::llm::gguf_header::read_summary;

/// The context of the check of a cache type at load.
const CHECK_CONTEXT: u32 = 512;
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
    #[error("{0}")]
    Plan(#[from] PlanError),
    #[error("llama.cpp could not load the model: {0}")]
    Load(String),
    #[error("the file has no chat template: {0}")]
    NoTemplate(String),
}

/// What the caller may decide about a load. The catalog's context window goes in `context_window` and an
/// installed drafter in `drafter`; the layer and thread counts are for tests and the evaluation.
#[derive(Debug, Clone, Default)]
pub struct LoadOptions {
    /// Context wanted, from the catalog; [`DEFAULT_CONTEXT`] without it.
    pub context_window: Option<u32>,
    /// Forces the number of layers on the GPU instead of letting the memory plan choose.
    pub gpu_layers: Option<u32>,
    /// Forces the number of CPU threads a generation uses instead of [`default_threads`].
    pub threads: Option<u32>,
    /// The model's MTP drafter (spec 079). It is loaded only where the memory plan allows it, or always
    /// when `gpu_layers` is forced.
    pub drafter: Option<PathBuf>,
    /// The context length a person chose for this model (spec 086): the planner's own, or a fixed one.
    pub choice: ContextChoice,
    /// The type of the KV cache a person chose.
    pub cache: CacheType,
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
    /// Loaded only when `placement.drafter` is set on desktop targets.
    #[cfg(not(any(target_os = "android", target_os = "ios")))]
    pub(super) drafter: Option<LlamaModel>,
    pub(super) threads: u32,
    /// Where it computes; each model call claims these (spec 081 FR-016).
    pub(super) units: Vec<crate::compute::Unit>,
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
        let placement = placement_for(path, &options)?;

        let params = LlamaModelParams::default().with_n_gpu_layers(placement.gpu_layers);
        let model = LlamaModel::load_from_file(backend, path, &params)
            .map_err(|e| LoadError::Load(e.to_string()))?;
        #[cfg(not(any(target_os = "android", target_os = "ios")))]
        let drafter = match &options.drafter {
            Some(drafter) if placement.drafter => load_drafter(backend, drafter, &model),
            _ => None,
        };
        #[cfg(not(any(target_os = "android", target_os = "ios")))]
        let placement = Placement {
            drafter: drafter.is_some(),
            ..placement
        };
        #[cfg(any(target_os = "android", target_os = "ios"))]
        let placement = Placement {
            drafter: false,
            ..placement
        };
        check_cache(
            backend,
            &model,
            &placement,
            options.threads.unwrap_or_else(default_threads),
        )?;
        let template = model
            .chat_template(None)
            .map_err(|e| LoadError::NoTemplate(format!("{e:?}")))?
            .to_string()
            .map_err(|e| LoadError::NoTemplate(e.to_string()))?;
        let vocab = model.vocab();
        let piece =
            |token| String::from_utf8_lossy(&vocab.token_to_piece(token, true, None)).into_owned();
        let (bos, eos) = (piece(vocab.bos()), piece(vocab.eos()));
        let units = crate::compute::units_of_layers(
            placement.gpu_layers,
            model.n_layer(),
            free_memory().device.is_some(),
        );
        Ok(Self {
            format: detect(&template),
            model,
            template,
            bos,
            eos,
            placement,
            #[cfg(not(any(target_os = "android", target_os = "ios")))]
            drafter,
            threads: options.threads.unwrap_or_else(default_threads),
            units,
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

/// Makes one context of a few tokens with the chosen cache type, so a model the engine cannot run with it
/// (flash attention is not available for its head sizes, say) is refused here with the engine's own words,
/// not at the first message of a chat. Nothing to check with the standard cache, which the engine always runs.
fn check_cache(
    backend: &LlamaBackend,
    model: &LlamaModel,
    placement: &Placement,
    threads: u32,
) -> Result<(), LoadError> {
    if placement.cache == CacheType::Standard {
        return Ok(());
    }
    let n_ctx = CHECK_CONTEXT as usize;
    let params = LlamaContextParams::default()
        .with_swa_full(super::generate::SWA_FULL)
        .with_n_ctx(NonZeroU32::new(CHECK_CONTEXT))
        .with_n_batch(CHECK_CONTEXT)
        .with_n_threads(threads as i32)
        .with_n_threads_batch(threads as i32);
    let params = super::context_params::with_choice(params, placement, n_ctx);
    model
        .new_context(backend, params)
        .map(drop)
        .map_err(|e| LoadError::Load(format!("the model cannot run with the chosen cache: {e}")))
}

/// Loads the drafter whole onto the GPU and checks that it belongs to `model`: its architecture is the
/// model's with `-assistant`, and it reads the model's embeddings (`n_embd_out`). Any failure is logged
/// and the model runs without it (spec FR-008).
#[cfg(not(any(target_os = "android", target_os = "ios")))]
fn load_drafter(backend: &LlamaBackend, path: &Path, model: &LlamaModel) -> Option<LlamaModel> {
    let params = LlamaModelParams::default().with_n_gpu_layers(ALL_LAYERS);
    let drafter = match LlamaModel::load_from_file(backend, path, &params) {
        Ok(drafter) => drafter,
        Err(e) => {
            log::warn!("drafter {} not loaded: {e}", path.display());
            return None;
        }
    };
    let architecture = |m: &LlamaModel| m.meta_val_str("general.architecture").ok();
    let expected = architecture(model).map(|a| format!("{a}-assistant"));
    if architecture(&drafter).is_none() || architecture(&drafter) != expected {
        log::warn!(
            "drafter {} is {:?}, the model needs {:?}; loading without it",
            path.display(),
            architecture(&drafter),
            expected
        );
        return None;
    }
    if drafter.n_embd_out() != model.n_embd() {
        log::warn!(
            "drafter {} reads embeddings of width {}, the model has {}; loading without it",
            path.display(),
            drafter.n_embd_out(),
            model.n_embd()
        );
        return None;
    }
    Some(drafter)
}

/// The drafter's weights and cache for the plan; `None` (and no drafter) when its header is unreadable.
#[cfg(not(any(target_os = "android", target_os = "ios")))]
fn drafter_shape(path: &Path) -> Option<DrafterShape> {
    let read = || -> std::io::Result<Option<DrafterShape>> {
        let summary = read_summary(&mut BufReader::new(File::open(path)?))?;
        let file_bytes = std::fs::metadata(path)?.len();
        Ok(
            ModelShape::from_header(&summary, file_bytes).map(|shape| DrafterShape {
                file_bytes,
                kv: shape.kv,
            }),
        )
    };
    read().unwrap_or_else(|e| {
        log::warn!("drafter {} unreadable: {e}", path.display());
        None
    })
}

/// The context a load aims at: the target, or the catalog's number when that is larger. The catalog's
/// number only says what the model is known for; the file's own limit and the memory cap the plan.
pub fn wanted_context(catalog: Option<u32>) -> u32 {
    catalog.unwrap_or(0).max(TARGET_CONTEXT)
}

/// What the planner knows about the model in `path` from its header; `None` for a file without a layer
/// count, which cannot be sized. The drafter's shape is added when one is given.
pub fn read_shape(path: &Path, drafter: Option<&Path>) -> Result<Option<ModelShape>, LoadError> {
    let summary = File::open(path)
        .and_then(|file| read_summary(&mut BufReader::new(file)))
        .map_err(|e| LoadError::Header(e.to_string()))?;
    let file_bytes = std::fs::metadata(path)
        .map_err(|e| LoadError::Header(e.to_string()))?
        .len();
    let shape = ModelShape::from_header(&summary, file_bytes);
    #[cfg(not(any(target_os = "android", target_os = "ios")))]
    let shape = shape.map(|mut shape| {
        shape.drafter = drafter.and_then(drafter_shape);
        shape
    });
    // No drafter runs on a phone.
    #[cfg(any(target_os = "android", target_os = "ios"))]
    let _ = drafter;
    Ok(shape)
}

/// Reads the header, looks at what is free and plans; the forced layer count of `options` wins over the plan.
fn placement_for(path: &Path, options: &LoadOptions) -> Result<Placement, LoadError> {
    let Some(shape) = read_shape(path, options.drafter.as_deref())? else {
        // A file without a layer count cannot be sized; load it as asked (a fixed context of the person
        // included, spec 086 edge cases) and leave the rest to ggml.
        return Ok(Placement {
            gpu_layers: options.gpu_layers.unwrap_or(ALL_LAYERS),
            context: match options.choice {
                ContextChoice::Fixed(tokens) => tokens,
                ContextChoice::Auto => options.context_window.unwrap_or(DEFAULT_CONTEXT),
            },
            drafter: false,
            cache: options.cache,
            yarn_from: None,
        });
    };
    let mut placed = plan_with(
        &shape,
        free_memory(),
        Some(wanted_context(options.context_window)),
        options.choice,
        options.cache,
    )?;
    if let Some(layers) = options.gpu_layers {
        // Whoever forces the layers decides about the drafter too, as the evaluation of a partly placed
        // model needs (spec 079 T031).
        placed.gpu_layers = layers;
        placed.drafter = shape.drafter.is_some()
            && placed.cache == CacheType::Standard
            && placed.yarn_from.is_none();
    }
    Ok(placed)
}

/// Free memory of the best GPU and of the host, as llama.cpp and the OS see them right now.
pub fn free_memory() -> FreeMemory {
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
