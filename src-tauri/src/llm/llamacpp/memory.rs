//! Decides, before a load, how much of a model goes on the GPU and how large the context is (spec 070,
//! plan D6, research R2).
//!
//! ggml aborts the whole process when an allocation fails, so a model that does not fit has to be refused
//! before it is loaded. llama.cpp's own fitting routine (`common_fit_params`) sits behind the `common`
//! feature of the binding, which pulls in a second library; the numbers needed here are few and sit in
//! the GGUF header, so the estimate is made in this module instead and can be tested without a model.

use crate::llm::gguf_header::GgufSummary;

/// Context size used when the file says nothing about its shape and the catalog gives none.
pub const DEFAULT_CONTEXT: u32 = 8192;
/// The context a model is planned for when its file allows it (operator 2026-11-09: 128k is the least a
/// useful agent works with). The model's own limit and the memory cap it.
pub const TARGET_CONTEXT: u32 = 131_072;
/// A full offload to the GPU is tried down to this context before any layer moves to the host: a window
/// this long still holds an agent's conversation, and a half-placed model is several times slower.
const FULL_GPU_FLOOR: u32 = 32_768;
/// A layer that looks at a window keeps the window and what a batch adds to it.
const WINDOW_MARGIN: u64 = 1024;
/// More layers than any model has: a header that claims more is not a model the planner sizes (the walk over
/// its layers would take as long as the header says).
const MAX_LAYERS: u32 = 4096;
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

/// What the KV cache takes (f16): the layers that keep every token grow with the context, the layers
/// that look at a window only stop growing at the window.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Kv {
    /// Bytes per token across the layers that keep all tokens.
    pub growing_per_token: u64,
    /// Bytes per token across the layers that look at a window only.
    pub windowed_per_token: u64,
    /// How many tokens those layers keep at the most.
    pub window_tokens: u64,
    /// Bytes that do not depend on the context: the state of the layers of a hybrid model that keep
    /// one instead of a cache.
    pub fixed: u64,
}

impl Kv {
    /// Every layer keeps every token.
    pub const fn dense(bytes_per_token: u64) -> Self {
        Self {
            growing_per_token: bytes_per_token,
            windowed_per_token: 0,
            window_tokens: 0,
            fixed: 0,
        }
    }

    /// The cache at `context` tokens.
    pub fn bytes(&self, context: u32) -> u64 {
        let context = u64::from(context);
        // Saturating: a header with absurd numbers must not panic the planner, it only never fits.
        self.fixed
            .saturating_add(self.growing_per_token.saturating_mul(context))
            .saturating_add(
                self.windowed_per_token
                    .saturating_mul(context.min(self.window_tokens)),
            )
    }
}

/// What the planner knows about the model.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModelShape {
    pub file_bytes: u64,
    pub layers: u32,
    /// The KV cache; an upper bound for models that mix attention with other layer kinds.
    pub kv: Kv,
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
    pub kv: Kv,
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
            .filter(|n| (1..=MAX_LAYERS).contains(n))?;
        let head_count = summary.head_count.filter(|n| *n > 0);
        let head_dim = summary
            .key_length
            .or_else(|| Some(summary.embedding_length? / head_count?))?;
        let kv = kv_of(summary, layers, head_count, head_dim)?;
        Some(Self {
            file_bytes,
            layers,
            kv,
            drafter: None,
            trained_context: summary
                .context_length
                .and_then(|n| u32::try_from(n).ok())
                .filter(|n| *n > 0),
        })
    }
}

/// The KV cache of `layers` layers from the header. Per layer: its number of KV heads (a per-layer array
/// where the layers differ, else one number, else every head) and the size of its heads (the `_swa` sizes
/// for a layer that looks at a window only). A layer the pattern marks as windowed stops growing at the
/// window; without a pattern every layer grows, which only errs on the safe side.
fn kv_of(summary: &GgufSummary, layers: u32, head_count: Option<u64>, head_dim: u64) -> Option<Kv> {
    let count = layers as usize;
    let uniform_heads = summary.head_count_kv.or(head_count);
    let heads_of = |layer: usize| -> Option<u64> {
        match &summary.head_count_kv_layers {
            Some(per_layer) if per_layer.len() == count => Some(per_layer[layer]),
            _ => uniform_heads,
        }
    };
    let windowed_flags = summary
        .sliding_window_pattern
        .as_ref()
        .filter(|pattern| pattern.len() == count)
        .zip(summary.sliding_window.filter(|window| *window > 0));
    let value_dim = summary.value_length.unwrap_or(head_dim);
    // A hybrid model: only every n-th layer has a cache, the others a state of fixed size.
    let interval = summary.full_attention_interval.filter(|n| *n > 1);
    let state_bytes = summary
        .ssm_inner_size
        .unwrap_or(0)
        .saturating_mul(summary.ssm_state_size.unwrap_or(0))
        .saturating_mul(4);
    let (mut growing, mut windowed, mut fixed) = (0u64, 0u64, 0u64);
    for layer in 0..count {
        if let Some(interval) = interval {
            if !(layer as u64 + 1).is_multiple_of(interval) {
                fixed = fixed.saturating_add(state_bytes);
                continue;
            }
        }
        let heads = heads_of(layer)?;
        let is_windowed = windowed_flags.is_some_and(|(pattern, _)| pattern[layer]);
        let (key, value) = if is_windowed {
            (
                summary.key_length_swa.unwrap_or(head_dim),
                summary.value_length_swa.unwrap_or(value_dim),
            )
        } else {
            (head_dim, value_dim)
        };
        // K and V, two bytes each.
        let bytes = heads
            .saturating_mul(key.saturating_add(value))
            .saturating_mul(KV_BYTES_PER_VALUE / 2);
        if is_windowed {
            windowed = windowed.saturating_add(bytes);
        } else {
            growing = growing.saturating_add(bytes);
        }
    }
    Some(Kv {
        growing_per_token: growing,
        windowed_per_token: windowed,
        window_tokens: windowed_flags.map_or(0, |(_, window)| window.saturating_add(WINDOW_MARGIN)),
        fixed,
    })
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
    let model = shape
        .file_bytes
        .saturating_add(shape.kv.bytes(placed.context))
        .saturating_add(DEVICE_MARGIN);
    let helper = drafter
        .file_bytes
        .saturating_add(drafter.kv.bytes(placed.context))
        .saturating_add(DRAFTER_MARGIN);
    model.saturating_add(helper) <= device
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
    let kv = |context: u32| shape.kv.bytes(context);

    if let Some(device) = free.device {
        // The first three halvings, and further ones while the context is still worth keeping whole.
        let attempts = contexts(start, minimum)
            .enumerate()
            .take_while(|(attempt, context)| *attempt < 3 || *context >= FULL_GPU_FLOOR)
            .map(|(_, context)| context);
        for context in attempts {
            if shape
                .file_bytes
                .saturating_add(kv(context))
                .saturating_add(DEVICE_MARGIN)
                <= device
            {
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
        let per_layer_total = per_layer.saturating_add(kv(context) / layers);
        // At least one layer stays on the host, or the full-offload case above would have taken it.
        let on_device = free.device.map_or(0, |device| {
            (device.saturating_sub(DEVICE_MARGIN) / per_layer_total.max(1)).min(layers - 1)
        });
        if (layers - on_device)
            .saturating_mul(per_layer_total)
            .saturating_add(remainder)
            <= host_budget
        {
            return Ok(Placement {
                gpu_layers: on_device as u32,
                context,
                drafter: false,
            });
        }
    }

    Err(MemoryRefusal {
        need_mib: shape.file_bytes.saturating_add(kv(last)) >> 20,
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
