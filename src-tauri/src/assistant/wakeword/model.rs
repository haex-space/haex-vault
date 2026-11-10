//! The wake word models in tract (pure-Rust ONNX, research R3/T007): the speech embedding with
//! the two input lengths the detector uses, and the classifier. Loaded once per conversation
//! audio and shared; running them needs no lock.

use std::path::Path;
use std::sync::Arc;

use tract_onnx::prelude::*;

use super::detector::{Scorer, BACKFILL_SAMPLES, EMBEDDING, FIRST_REAL, STREAM_SAMPLES, WINDOW};
use crate::error::{HolziError, Result};

type Plan = TypedSimplePlan<TypedModel>;

pub struct Models {
    /// The embedding for [`STREAM_SAMPLES`] and for [`BACKFILL_SAMPLES`] of input.
    stream: Plan,
    backfill: Plan,
    classifier: Plan,
}

fn failed(error: impl std::fmt::Display) -> HolziError {
    HolziError::SpeechFailed {
        reason: format!("load the wake word models: {error}"),
    }
}

fn plan(path: &Path, shape: &[usize]) -> Result<Plan> {
    tract_onnx::onnx()
        .model_for_path(path)
        .and_then(|model| model.with_input_fact(0, f32::fact(shape).into()))
        .and_then(|model| model.into_optimized())
        .and_then(|model| model.into_runnable())
        .map_err(failed)
}

impl Models {
    /// Loads `embedding` (speech_embedding.onnx) and `classifier` (hex.onnx). Takes a few hundred
    /// milliseconds; callers load off the async runtime.
    pub fn load(embedding: &Path, classifier: &Path) -> Result<Arc<Self>> {
        Ok(Arc::new(Self {
            stream: plan(embedding, &[STREAM_SAMPLES])?,
            backfill: plan(embedding, &[BACKFILL_SAMPLES])?,
            classifier: plan(classifier, &[1, WINDOW, EMBEDDING])?,
        }))
    }

    fn embed_with(plan: &Plan, audio: &[f32]) -> TractResult<Vec<[f32; EMBEDDING]>> {
        let input = tract_ndarray::Array1::from(audio.to_vec()).into_tensor();
        let out = plan.run(tvec!(input.into()))?;
        let view = out[0].to_array_view::<f32>()?;
        let mut frames = Vec::new();
        for row in view.outer_iter().skip(FIRST_REAL) {
            let mut frame = [0.0; EMBEDDING];
            for (slot, value) in frame.iter_mut().zip(row.iter()) {
                *slot = *value;
            }
            frames.push(frame);
        }
        Ok(frames)
    }
}

/// A [`Scorer`] over shared [`Models`].
#[derive(Clone)]
pub struct TractScorer(pub Arc<Models>);

impl Scorer for TractScorer {
    fn embed(&mut self, audio: &[f32]) -> Vec<[f32; EMBEDDING]> {
        let plan = match audio.len() {
            STREAM_SAMPLES => &self.0.stream,
            BACKFILL_SAMPLES => &self.0.backfill,
            other => {
                log::warn!("assistant: wake word embedding of {other} samples is not planned");
                return Vec::new();
            }
        };
        Models::embed_with(plan, audio).unwrap_or_else(|error| {
            log::warn!("assistant: wake word embedding failed: {error}");
            Vec::new()
        })
    }

    fn score(&mut self, window: &[[f32; EMBEDDING]]) -> f32 {
        let flat: Vec<f32> = window.iter().flatten().copied().collect();
        let result = tract_ndarray::Array3::from_shape_vec((1, WINDOW, EMBEDDING), flat)
            .map_err(TractError::from)
            .and_then(|input| self.0.classifier.run(tvec!(input.into_tensor().into())))
            .and_then(|out| {
                out[0]
                    .to_array_view::<f32>()
                    .map(|view| view.iter().next().copied().unwrap_or(0.0))
            });
        result.unwrap_or_else(|error| {
            log::warn!("assistant: wake word classifier failed: {error}");
            0.0
        })
    }
}

#[cfg(test)]
#[path = "model_tests.rs"]
mod tests;
