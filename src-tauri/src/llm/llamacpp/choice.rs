//! What a person may decide about the context of a local model (spec 086): its length and the type of the
//! KV cache. The planner in [`super::memory`] turns the choice into a placement; this module holds the
//! choice, the rules it must obey and the limits a settings page shows.

use serde::{Deserialize, Serialize};

use super::memory::{
    fits_on_device, place_at, FreeMemory, MemoryRefusal, ModelShape, MIN_CONTEXT, TARGET_CONTEXT,
};

/// How far beyond its training a model may be stretched (YaRN, spec 086 FR-008).
pub const MAX_STRETCH: u32 = 4;

/// The type of the KV cache. A smaller one takes less memory and can lower the quality of long contexts.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum CacheType {
    /// f16, what the engine does without a choice.
    #[default]
    #[serde(rename = "standard")]
    Standard,
    /// 8 bits per value (`Q8_0`).
    #[serde(rename = "q8")]
    Q8,
    /// 4 bits per value (`Q4_0`).
    #[serde(rename = "q4")]
    Q4,
}

impl CacheType {
    /// The size of `f16_bytes` of cache in this type. `Q8_0` stores 32 values and a two-byte scale in 34
    /// bytes, `Q4_0` in 18, f16 in 64: 17/32 and 9/32 of the f16 size.
    pub fn scale(self, f16_bytes: u64) -> u64 {
        let sixty_fourths = match self {
            Self::Standard => 64,
            Self::Q8 => 34,
            Self::Q4 => 18,
        };
        f16_bytes.saturating_mul(sixty_fourths) / 64
    }

    /// The name the settings and the placement report use.
    pub fn name(self) -> &'static str {
        match self {
            Self::Standard => "standard",
            Self::Q8 => "q8",
            Self::Q4 => "q4",
        }
    }

    /// Whether the engine needs flash attention for it (a quantised V cache does).
    pub fn needs_flash_attention(self) -> bool {
        self != Self::Standard
    }
}

/// The context length of a load: left to the planner, or asked for.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ContextChoice {
    /// The planner aims at [`TARGET_CONTEXT`], capped by the model and the memory, whole on the GPU.
    #[default]
    Auto,
    /// Exactly this many tokens: it is placed as asked or refused, never shrunk.
    Fixed(u32),
}

/// Why a choice cannot be placed. A memory refusal is the planner's own; the others are the rules of the
/// choice, checked before any memory is looked at.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum PlanError {
    #[error("not enough memory to load the model: {0}")]
    Memory(#[from] MemoryRefusal),
    #[error("a context of {asked} tokens is below the least that is planned ({floor})")]
    BelowFloor { asked: u32, floor: u32 },
    #[error(
        "the model was trained for {trained} tokens and its file declares its own rope scaling, so it cannot be stretched to {asked}"
    )]
    NotStretchable { asked: u32, trained: u32 },
    #[error(
        "{asked} tokens is more than {MAX_STRETCH} times what the model was trained for ({trained}, so at most {max})"
    )]
    BeyondTraining { asked: u32, trained: u32, max: u32 },
}

/// The rules a fixed context obeys, before memory: at least the floor, and beyond the training only where
/// the model can be stretched and not further than [`MAX_STRETCH`] times.
pub fn check_fixed(shape: &ModelShape, asked: u32) -> Result<(), PlanError> {
    if asked < MIN_CONTEXT {
        return Err(PlanError::BelowFloor {
            asked,
            floor: MIN_CONTEXT,
        });
    }
    let Some(trained) = shape.trained_context.filter(|trained| asked > *trained) else {
        return Ok(());
    };
    if !shape.stretchable {
        return Err(PlanError::NotStretchable { asked, trained });
    }
    let max = trained.saturating_mul(MAX_STRETCH);
    if asked > max {
        return Err(PlanError::BeyondTraining {
            asked,
            trained,
            max,
        });
    }
    Ok(())
}

/// What the file says and the device allows, for one cache type (spec 086, Key Entities).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Limits {
    /// The context the file declares, `None` when it declares none.
    pub trained: Option<u32>,
    /// The largest context with every layer on the GPU; `None` when not even the floor fits so.
    pub max_on_gpu: Option<u32>,
    /// The largest context that fits at all, with layers on the host; `None` when not even the floor fits.
    pub max_any: Option<u32>,
    /// The largest context the model can be stretched to; `None` where it cannot be.
    pub max_stretched: Option<u32>,
}

/// The largest context in `MIN_CONTEXT..=ceiling` for which `fits` holds, `None` when not even the floor
/// does. `fits` only gets harder as the context grows (the cache does).
fn largest(ceiling: u32, fits: impl Fn(u32) -> bool) -> Option<u32> {
    if ceiling < MIN_CONTEXT || !fits(MIN_CONTEXT) {
        return None;
    }
    let (mut low, mut high) = (MIN_CONTEXT, ceiling);
    while low < high {
        let middle = low + (high - low).div_ceil(2);
        if fits(middle) {
            low = middle;
        } else {
            high = middle - 1;
        }
    }
    Some(low)
}

/// The limits of `shape` on a machine with `free` memory, with the cache of type `cache`.
pub fn limits(shape: &ModelShape, free: FreeMemory, cache: CacheType) -> Limits {
    let max_stretched = shape
        .trained_context
        .filter(|_| shape.stretchable)
        .map(|trained| trained.saturating_mul(MAX_STRETCH));
    // A model of unknown length is looked at up to the target; one of known length up to what it can
    // be stretched to, or its own length where it cannot.
    let ceiling = max_stretched
        .or(shape.trained_context)
        .unwrap_or(TARGET_CONTEXT);
    Limits {
        trained: shape.trained_context,
        max_on_gpu: free.device.and_then(|_| {
            largest(ceiling, |context| {
                fits_on_device(shape, free, cache, context)
            })
        }),
        max_any: largest(ceiling, |context| {
            place_at(shape, free, cache, context).is_some()
        }),
        max_stretched,
    }
}

#[cfg(test)]
#[path = "choice_tests.rs"]
mod tests;
