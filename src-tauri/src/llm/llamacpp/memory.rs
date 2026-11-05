//! Decides, before a load, how much of a model goes on the GPU and how large the context is (spec 070,
//! plan D6, research R2).
//!
//! ggml aborts the whole process when an allocation fails, so a model that does not fit has to be refused
//! before it is loaded. llama.cpp's own fitting routine (`common_fit_params`) sits behind the `common`
//! feature of the binding, which pulls in a second library; the numbers needed here are few and sit in
//! the GGUF header, so the estimate is made in this module instead and can be tested without a model.

use crate::llm::local::gguf_header::GgufSummary;

/// Context size used when the catalog gives none.
pub const DEFAULT_CONTEXT: u32 = 8192;
/// The smallest context the planner shrinks to before it refuses.
const MIN_CONTEXT: u32 = 2048;
/// Compute buffers and CUDA/Metal runtime state on a device, next to weights and cache.
const DEVICE_MARGIN: u64 = 768 << 20;
/// The same on the host side.
const HOST_MARGIN: u64 = 512 << 20;
/// How much of the free host memory a placement may use.
const HOST_USABLE: f64 = 0.9;
/// The KV cache is stored as f16: K and V, two bytes each.
const KV_BYTES_PER_VALUE: u64 = 4;
/// Compute buffers of the drafter's own context on the device (spec 079), next to its weights and cache.
const DRAFTER_MARGIN: u64 = 256 << 20;

/// What the planner knows about the model.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModelShape {
    pub file_bytes: u64,
    pub layers: u32,
    /// Bytes of KV cache per token across all layers (an upper bound for models that mix attention
    /// with other layer kinds).
    pub kv_bytes_per_token: u64,
    /// The context the model was trained for.
    pub trained_context: Option<u32>,
    /// The model's MTP drafter, when one is to be loaded with it (spec 079).
    pub drafter: Option<DrafterShape>,
}

/// What the planner knows about a drafter: its weights and the cache of its own context, which has the
/// model's context length.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DrafterShape {
    pub file_bytes: u64,
    pub kv_bytes_per_token: u64,
}

/// What the machine has free.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FreeMemory {
    /// Free memory of the best GPU, `None` without one.
    pub device: Option<u64>,
    pub host: u64,
}

/// What the load is told.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Placement {
    /// Layers on the GPU; the count of layers plus the output layer means everything.
    pub gpu_layers: u32,
    pub context: u32,
    /// Whether this load uses the drafter (spec 079 research R7).
    pub drafter: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error(
    "the model needs about {need_mib} MiB at a context of {context} tokens and the machine has {free_mib} MiB free"
)]
pub struct MemoryRefusal {
    pub need_mib: u64,
    pub free_mib: u64,
    pub context: u32,
}

impl ModelShape {
    /// Reads the shape from the header numbers; `None` when the file lacks the layer count.
    pub fn from_header(summary: &GgufSummary, file_bytes: u64) -> Option<Self> {
        let layers = u32::try_from(summary.block_count?)
            .ok()
            .filter(|n| *n > 0)?;
        let head_count = summary.head_count.filter(|n| *n > 0);
        let head_dim = summary
            .key_length
            .or_else(|| Some(summary.embedding_length? / head_count?))?;
        // A per-layer array of KV head counts is not read; every head counts then, which only errs on
        // the safe side.
        let kv_heads = summary.head_count_kv.or(head_count)?;
        let kv_bytes_per_token = u64::from(layers) * kv_heads * head_dim * KV_BYTES_PER_VALUE;
        Some(Self {
            file_bytes,
            layers,
            kv_bytes_per_token,
            drafter: None,
            trained_context: summary
                .context_length
                .and_then(|n| u32::try_from(n).ok())
                .filter(|n| *n > 0),
        })
    }
}

/// Picks the context, the layers on the GPU and whether the drafter is used, or refuses.
///
/// The drafter never changes the model's own placement (spec 079 research R7):
/// 1. the model sits whole on the GPU and the drafter fits next to it at the same context: both are used;
/// 2. the model sits whole on the GPU but the drafter does not fit as well: no drafter;
/// 3. the model sits only partly on the GPU: no drafter (a measured exception follows with spec 079 US4);
/// 4. no GPU: no drafter.
///
/// ponytail: the rules come from one measurement on one laptop (research R1), not from a speed probe of
/// this device. A faster placement with fewer layers plus the drafter is never tried (spec FR-009).
pub fn plan(
    shape: &ModelShape,
    free: FreeMemory,
    wanted_context: Option<u32>,
) -> Result<Placement, MemoryRefusal> {
    let placed = plan_model(shape, free, wanted_context)?;
    Ok(Placement {
        drafter: drafter_fits(shape, free, placed),
        ..placed
    })
}

/// Rules 1 to 4 of [`plan`] for a placement made without the drafter.
fn drafter_fits(shape: &ModelShape, free: FreeMemory, placed: Placement) -> bool {
    let (Some(drafter), Some(device)) = (shape.drafter, free.device) else {
        return false;
    };
    if placed.gpu_layers <= shape.layers {
        return false;
    }
    let context = u64::from(placed.context);
    let model = shape.file_bytes + shape.kv_bytes_per_token * context + DEVICE_MARGIN;
    let helper = drafter.file_bytes + drafter.kv_bytes_per_token * context + DRAFTER_MARGIN;
    model + helper <= device
}

/// Picks the context and the layers on the GPU for the model alone, or refuses.
///
/// `wanted_context` is the catalog's `context_window` (or [`DEFAULT_CONTEXT`]); it is capped by what the
/// model was trained for. Everything on the GPU wins over a long context: the context is halved twice
/// before layers move to the host. Then layers move to the host until the GPU part fits, and the context
/// shrinks further only when the host cannot take the rest either.
fn plan_model(
    shape: &ModelShape,
    free: FreeMemory,
    wanted_context: Option<u32>,
) -> Result<Placement, MemoryRefusal> {
    let wanted = wanted_context.unwrap_or(DEFAULT_CONTEXT);
    let start = shape
        .trained_context
        .map_or(wanted, |trained| wanted.min(trained))
        .max(1);
    // Keep a smaller requested/trained context when it is already below the planner's normal floor.
    let minimum = start.min(MIN_CONTEXT);
    let kv = |context: u32| shape.kv_bytes_per_token * u64::from(context);

    if let Some(device) = free.device {
        for context in contexts(start, minimum).take(3) {
            if shape.file_bytes + kv(context) + DEVICE_MARGIN <= device {
                return Ok(Placement {
                    gpu_layers: shape.layers + 1,
                    context,
                    drafter: false,
                });
            }
        }
    }

    let layers = u64::from(shape.layers);
    let per_layer = shape.file_bytes / layers;
    // The output layer and the embeddings are a share of the file that no block carries.
    let remainder = shape.file_bytes - per_layer * layers;
    let host_budget = (free.host.saturating_sub(HOST_MARGIN) as f64 * HOST_USABLE) as u64;
    let mut last = start;
    for context in contexts(start, minimum) {
        last = context;
        let per_layer_total = per_layer + kv(context) / layers;
        // At least one layer stays on the host, or the full-offload case above would have taken it.
        let on_device = free.device.map_or(0, |device| {
            (device.saturating_sub(DEVICE_MARGIN) / per_layer_total.max(1)).min(layers - 1)
        });
        if (layers - on_device) * per_layer_total + remainder <= host_budget {
            return Ok(Placement {
                gpu_layers: on_device as u32,
                context,
                drafter: false,
            });
        }
    }

    Err(MemoryRefusal {
        need_mib: (shape.file_bytes + kv(last)) >> 20,
        free_mib: free.host.saturating_add(free.device.unwrap_or(0)) >> 20,
        context: last,
    })
}

/// `start`, then half of it, down to `minimum`.
fn contexts(start: u32, minimum: u32) -> impl Iterator<Item = u32> {
    let mut next = Some(start);
    std::iter::from_fn(move || {
        let current = next?;
        next = (current > minimum).then(|| (current / 2).max(minimum));
        Some(current)
    })
}

#[cfg(test)]
#[path = "memory_tests.rs"]
mod tests;
