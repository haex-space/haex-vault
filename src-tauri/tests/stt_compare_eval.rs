//! Compares the local STT engines (spec 066, research R7, T135): candle Whisper against Parakeet
//! TDT 0.6B v3 through sherpa-onnx, on recordings with a reference text. Skipped by default
//! (`#[ignore]`): it downloads Whisper and needs the Parakeet export on disk.
//!
//!   HOLZI_STT_EVAL_DIR=<dir with <lang>/<n>.wav + <n>.txt> \
//!   HOLZI_PARAKEET_DIR=<unpacked sherpa-onnx-nemo-parakeet-tdt-0.6b-v3-int8> \
//!     cargo test --release --manifest-path src-tauri/Cargo.toml --test stt_compare_eval \
//!       -- --ignored --nocapture
//!
//! Optional: `HOLZI_STT_EVAL_MODELS` (default `parakeet,whisper-tiny,whisper-base,whisper-small`),
//! `HOLZI_STT_EVAL_CACHE` for the Whisper downloads (default `target/eval/stt-models`). Language
//! folders are Whisper codes (`de`, `en`, `fr`, `es`, `it`, `pt`); WAV files of any sample rate
//! are fine as long as they are 16 kHz mono. Whisper runs once with the six spoken languages as
//! candidates and, on de/en, once more with only de/en (today's setting), so the report shows what
//! widening the list costs. The report is printed and written to `target/eval/stt_compare.json`.

#![cfg(all(feature = "llm-cpu", feature = "voice"))]

use std::collections::BTreeMap;
use std::env;
use std::path::{Path, PathBuf};
use std::time::Instant;

use holzi_lib::stt::files::ensure_model_files;
use holzi_lib::stt::local::LocalWhisperAdapter;
use holzi_lib::stt::sherpa::ParakeetAdapter;
use holzi_lib::stt::{catalog, CanonicalPcm, SttAdapter, SAMPLE_RATE_HZ};
use serde_json::json;

const SPOKEN: [&str; 6] = ["de", "en", "fr", "es", "it", "pt"];

struct Sample {
    audio: CanonicalPcm,
    reference: String,
}

fn load_samples(dir: &Path) -> BTreeMap<String, Vec<Sample>> {
    let mut by_language = BTreeMap::new();
    for language in SPOKEN {
        let folder = dir.join(language);
        let Ok(entries) = std::fs::read_dir(&folder) else {
            continue;
        };
        let mut wavs: Vec<PathBuf> = entries
            .filter_map(|e| e.ok().map(|e| e.path()))
            .filter(|p| p.extension().is_some_and(|x| x == "wav"))
            .collect();
        wavs.sort();
        let samples = wavs
            .iter()
            .map(|wav| {
                let wave = sherpa_onnx::Wave::read(wav.to_str().expect("utf-8 path"))
                    .unwrap_or_else(|| panic!("read {}", wav.display()));
                assert_eq!(
                    wave.sample_rate(),
                    SAMPLE_RATE_HZ as i32,
                    "{}",
                    wav.display()
                );
                let reference = std::fs::read_to_string(wav.with_extension("txt"))
                    .unwrap_or_else(|_| panic!("reference text for {}", wav.display()));
                Sample {
                    audio: CanonicalPcm {
                        samples: wave.samples().to_vec(),
                    },
                    reference,
                }
            })
            .collect();
        by_language.insert(language.to_owned(), samples);
    }
    by_language
}

/// Lower case, every run of non-alphanumerics becomes one space.
fn words(text: &str) -> Vec<String> {
    text.to_lowercase()
        .split(|c: char| !c.is_alphanumeric())
        .filter(|w| !w.is_empty())
        .map(str::to_owned)
        .collect()
}

/// Word-level edit distance.
fn word_errors(reference: &[String], hypothesis: &[String]) -> usize {
    let mut previous: Vec<usize> = (0..=hypothesis.len()).collect();
    for (i, r) in reference.iter().enumerate() {
        let mut current = vec![i + 1; hypothesis.len() + 1];
        for (j, h) in hypothesis.iter().enumerate() {
            let substitution = previous[j] + usize::from(r != h);
            current[j + 1] = substitution.min(previous[j + 1] + 1).min(current[j] + 1);
        }
        previous = current;
    }
    previous[hypothesis.len()]
}

struct Score {
    errors: usize,
    words: usize,
    audio_secs: f64,
    compute_secs: f64,
    longest_secs: f64,
    count: usize,
}

async fn score(adapter: &dyn SttAdapter, samples: &[Sample]) -> Score {
    let mut score = Score {
        errors: 0,
        words: 0,
        audio_secs: 0.0,
        compute_secs: 0.0,
        longest_secs: 0.0,
        count: samples.len(),
    };
    for sample in samples {
        let started = Instant::now();
        let text = adapter
            .transcribe(&sample.audio)
            .await
            .expect("transcribes");
        let took = started.elapsed().as_secs_f64();
        let reference = words(&sample.reference);
        score.errors += word_errors(&reference, &words(&text));
        score.words += reference.len();
        score.audio_secs += sample.audio.samples.len() as f64 / SAMPLE_RATE_HZ as f64;
        score.compute_secs += took;
        score.longest_secs = score.longest_secs.max(took);
    }
    score
}

async fn whisper(id: &str, cache: &Path) -> LocalWhisperAdapter {
    let entry = catalog::get(id)
        .expect("catalog parses")
        .unwrap_or_else(|| panic!("{id} is in the STT catalog"));
    let dir = cache.join(id);
    std::fs::create_dir_all(&dir).expect("cache dir");
    ensure_model_files(&dir, entry)
        .await
        .expect("Whisper download");
    LocalWhisperAdapter::load_from_dir(&dir).expect("Whisper loads")
}

#[tokio::test]
#[ignore = "downloads Whisper and needs Parakeet and recordings, see the module doc"]
async fn compare_local_stt_engines() {
    let samples = load_samples(Path::new(
        &env::var("HOLZI_STT_EVAL_DIR").expect("HOLZI_STT_EVAL_DIR is set"),
    ));
    assert!(!samples.is_empty(), "no language folders found");
    let models = env::var("HOLZI_STT_EVAL_MODELS")
        .unwrap_or_else(|_| "parakeet,whisper-tiny,whisper-base,whisper-small".to_owned());
    let cache = env::var("HOLZI_STT_EVAL_CACHE")
        .map(PathBuf::from)
        .unwrap_or_else(|_| Path::new(env!("CARGO_MANIFEST_DIR")).join("target/eval/stt-models"));

    let mut rows = Vec::new();
    for model in models.split(',').map(str::trim) {
        let load_started = Instant::now();
        let runs: Vec<(String, Box<dyn SttAdapter>, &[&str])> = if model == "parakeet" {
            let dir = env::var("HOLZI_PARAKEET_DIR").expect("HOLZI_PARAKEET_DIR is set");
            let adapter = ParakeetAdapter::load_from_dir(Path::new(&dir)).expect("Parakeet loads");
            vec![(model.to_owned(), Box::new(adapter), &SPOKEN[..])]
        } else {
            let six = whisper(model, &cache).await.with_languages(&SPOKEN);
            let two = whisper(model, &cache).await.with_languages(&["de", "en"]);
            vec![
                (format!("{model} (6 langs)"), Box::new(six), &SPOKEN[..]),
                (format!("{model} (de/en)"), Box::new(two), &["de", "en"][..]),
            ]
        };
        let load_secs = load_started.elapsed().as_secs_f64();
        for (label, adapter, languages) in runs {
            for language in languages {
                let Some(set) = samples.get(*language) else {
                    continue;
                };
                let s = score(adapter.as_ref(), set).await;
                let wer = 100.0 * s.errors as f64 / s.words.max(1) as f64;
                let rtf = s.compute_secs / s.audio_secs.max(f64::EPSILON);
                let mean = s.compute_secs / s.count.max(1) as f64;
                println!(
                    "{label:<24} {language}  WER {wer:5.1} %  RTF {rtf:5.3}  mean {mean:5.2} s  max {:5.2} s  ({} utterances, {:.0} s audio)",
                    s.longest_secs, s.count, s.audio_secs
                );
                rows.push(json!({
                    "model": label, "language": language, "wer": wer, "rtf": rtf,
                    "meanSecs": mean, "maxSecs": s.longest_secs, "utterances": s.count,
                    "audioSecs": s.audio_secs, "loadSecs": load_secs,
                }));
            }
        }
    }
    let out = Path::new(env!("CARGO_MANIFEST_DIR")).join("target/eval");
    std::fs::create_dir_all(&out).expect("eval dir");
    std::fs::write(
        out.join("stt_compare.json"),
        serde_json::to_string_pretty(&rows).expect("json"),
    )
    .expect("write report");
}

#[test]
fn word_errors_counts_substitutions_insertions_and_deletions() {
    let r = words("Das ist ein Test.");
    assert_eq!(word_errors(&r, &words("das ist ein test")), 0);
    assert_eq!(word_errors(&r, &words("das war ein test")), 1);
    assert_eq!(word_errors(&r, &words("das ist test")), 1);
    assert_eq!(word_errors(&r, &words("das ist ein kleiner test")), 1);
    assert_eq!(word_errors(&r, &[]), 4);
}
