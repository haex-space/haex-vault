//! The real models in tract against values computed with onnxruntime in the training pipeline
//! (haex-wakeword, run7). Needs the two downloaded files: set `HOLZI_WAKEWORD_DIR` to a folder
//! with `speech_embedding.onnx` and `hex.onnx` and run with `--ignored`.

use super::super::{CLASSIFIER, EMBEDDING as EMBEDDING_FILE};
use super::*;
use crate::assistant::wakeword::detector::{Scorer, BACKFILL_SAMPLES, HOP, STREAM_SAMPLES};

/// 3 s of a deterministic signal the pipeline computed the same way.
fn signal() -> Vec<f32> {
    use std::f64::consts::PI;
    (0..48_000)
        .map(|n| {
            let t = f64::from(n) / 16_000.0;
            (0.3 * (2.0 * PI * 220.0 * t).sin() * (0.5 + 0.5 * (2.0 * PI * 3.0 * t).sin())
                + 0.05 * (2.0 * PI * 1234.0 * t).sin()) as f32
        })
        .collect()
}

fn models() -> Option<TractScorer> {
    let dir = std::path::PathBuf::from(std::env::var_os("HOLZI_WAKEWORD_DIR")?);
    let embedding = dir.join(EMBEDDING_FILE.file);
    let classifier = dir.join(CLASSIFIER.file);
    Some(TractScorer(
        Models::load(&embedding, &classifier).expect("load the models"),
    ))
}

#[test]
#[ignore = "needs the downloaded models in HOLZI_WAKEWORD_DIR"]
fn tract_gives_what_the_pipeline_computed() {
    let Some(mut scorer) = models() else {
        panic!("set HOLZI_WAKEWORD_DIR");
    };
    let audio = signal();
    // Streaming: one call per hop, the last frame of each (frame k ends at sample (k + 10) * HOP).
    let frame = |scorer: &mut TractScorer, k: usize| {
        let end = (k + 10) * HOP;
        scorer.embed(&audio[end - STREAM_SAMPLES..end])[0]
    };
    let expected: [(usize, [f32; 4]); 3] = [
        (0, [-7.46468, 27.1293, 7.93805, -2.36542]),
        (10, [-6.62125, 32.94535, 0.54676, -3.16346]),
        (27, [-6.81947, 33.37506, 0.48395, -2.28843]),
    ];
    for (k, values) in expected {
        let got = frame(&mut scorer, k);
        for (g, e) in got.iter().zip(values) {
            assert!((g - e).abs() < 1e-3, "frame {k}: {g} vs {e}");
        }
    }
    // The window at once equals the streamed frames.
    let backfill = scorer.embed(&audio[..BACKFILL_SAMPLES]);
    assert_eq!(backfill.len(), 16);
    let streamed: Vec<_> = (0..16).map(|k| frame(&mut scorer, k)).collect();
    for (a, b) in backfill.iter().zip(&streamed) {
        for (x, y) in a.iter().zip(b) {
            assert!((x - y).abs() < 1e-3);
        }
    }
    let score = scorer.score(&streamed);
    assert!((score - 0.017958).abs() < 1e-4, "score {score}");
}
