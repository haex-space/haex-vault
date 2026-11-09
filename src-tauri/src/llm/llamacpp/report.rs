//! What the settings page of a model shows (spec 086 FR-006): what the file says, what the device allows
//! with a given choice, and what is in effect. Made from the same planner the load uses, so the page and
//! the load cannot disagree about a choice except by the memory that changed in between.

use serde::Serialize;

use super::choice::{limits, CacheType, ContextChoice, Limits, PlanError};
use super::memory::{plan_with, FreeMemory, ModelShape, Placement, TARGET_CONTEXT};
use crate::adapters::PlacementInfo;

/// How a choice would be placed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum Verdict {
    /// Whole on the GPU.
    Fits,
    /// Some layers on the host: slower.
    FitsOnHost,
    /// Nowhere on this machine.
    TooLarge,
    /// More than four times what the model was trained for.
    BeyondTraining,
    /// Beyond the training, and the model's file declares a rope scaling of its own.
    NotStretchable,
    /// Below the least that is planned.
    BelowFloor,
    /// The header does not allow an estimate.
    Unsized,
}

/// Why the context in effect is what it is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum Reason {
    /// The target of the planner: the model and the memory allow it.
    Target,
    /// The model was trained for no more.
    Trained,
    /// The memory allows no more.
    Memory,
    /// A fixed length somebody chose.
    Choice,
}

/// What a loaded model runs with, and why.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Effective {
    #[serde(flatten)]
    pub info: PlacementInfo,
    pub reason: Reason,
}

/// The answer of the settings page's question.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ContextReport {
    #[serde(flatten)]
    pub limits: Limits,
    /// Whether the header allowed an estimate at all.
    pub sized: bool,
    pub verdict: Verdict,
    /// What the model and its cache need, with [`Verdict::TooLarge`].
    pub need_mib: Option<u64>,
    pub free_mib: Option<u64>,
    /// The factor the model is stretched by when the choice is beyond its training.
    pub yarn: Option<f32>,
    /// What the loaded model runs with, when this model is the loaded one.
    pub effective: Option<Effective>,
}

fn reason_of(info: &PlacementInfo, shape: Option<&ModelShape>, choice: ContextChoice) -> Reason {
    if matches!(choice, ContextChoice::Fixed(_)) {
        return Reason::Choice;
    }
    if info.context >= TARGET_CONTEXT {
        return Reason::Target;
    }
    match shape.and_then(|shape| shape.trained_context) {
        Some(trained) if info.context >= trained => Reason::Trained,
        _ => Reason::Memory,
    }
}

/// Answers for `choice` and `cache` on a machine with `free` memory. `wanted` is the context the load aims
/// at (`wanted_context`); `loaded` is what the model runs with now, when it is loaded.
pub fn report(
    shape: Option<&ModelShape>,
    free: FreeMemory,
    wanted: u32,
    choice: ContextChoice,
    cache: CacheType,
    loaded: Option<PlacementInfo>,
) -> ContextReport {
    let effective = loaded.map(|info| Effective {
        reason: reason_of(&info, shape, choice),
        info,
    });
    let Some(shape) = shape else {
        return ContextReport {
            limits: Limits {
                trained: None,
                max_on_gpu: None,
                max_any: None,
                max_stretched: None,
            },
            sized: false,
            verdict: Verdict::Unsized,
            need_mib: None,
            free_mib: None,
            yarn: None,
            effective,
        };
    };
    let mut report = ContextReport {
        limits: limits(shape, free, cache),
        sized: true,
        verdict: Verdict::Fits,
        need_mib: None,
        free_mib: None,
        yarn: None,
        effective,
    };
    match plan_with(shape, free, Some(wanted), choice, cache) {
        Ok(Placement {
            gpu_layers,
            context,
            yarn_from,
            ..
        }) => {
            report.verdict = if gpu_layers > shape.layers {
                Verdict::Fits
            } else {
                Verdict::FitsOnHost
            };
            report.yarn = yarn_from.map(|trained| context as f32 / trained as f32);
        }
        Err(PlanError::Memory(refusal)) => {
            report.verdict = Verdict::TooLarge;
            report.need_mib = Some(refusal.need_mib);
            report.free_mib = Some(refusal.free_mib);
        }
        Err(PlanError::BelowFloor { .. }) => report.verdict = Verdict::BelowFloor,
        Err(PlanError::NotStretchable { .. }) => report.verdict = Verdict::NotStretchable,
        Err(PlanError::BeyondTraining { .. }) => report.verdict = Verdict::BeyondTraining,
    }
    report
}

#[cfg(test)]
#[path = "report_tests.rs"]
mod tests;
