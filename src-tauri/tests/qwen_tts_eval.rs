//! Qwen3-TTS with the real model files (spec 081 T020, T021). Skipped by default (`#[ignore]`):
//! they need the two GGUF files of research R2 in one folder.
//!
//!   QWEN_TTS_DIR=<folder with the GGUF files> QWEN_TTS_OUT=<folder for WAVs and timings> \
//!     cargo test -j 4 --release --test qwen_tts_eval \
//!       --no-default-features --features llm-llamacpp,voice,voice-qwen -- --ignored --nocapture
//!
//! `QWEN_TTS_QUANT=Q4_K_M` speaks with the Q4_K_M backbone instead of Q8_0 (research R2, the file
//! must be in the folder). The voice is the shipped `de-lena`.

#![cfg(feature = "voice-qwen")]

use std::cell::Cell;
use std::path::PathBuf;
use std::time::{Duration, Instant};

use holzi_lib::tts::catalog::{get, CatalogFile, TtsCatalogEntry};
use holzi_lib::tts::local::{Engine, Reference};
use holzi_lib::tts::qwen::generate::FRAMES_PER_SECOND;
use holzi_lib::tts::qwen::{Placement, QwenModel};
use holzi_lib::tts::voices::{builtin, reference};

/// The six sentences of the spec 066 R23 listening comparison.
const R23: [&str; 6] = [
    "Dein Termin beim Zahnarzt ist morgen um 14:30 Uhr in der Bahnhofstraße 12.",
    "Die Rechnung über 1.249,90 € ist am 04.11.2026 fällig, das sind 3,5 % mehr als im Vorjahr.",
    "Jonas Müller hat dir geschrieben: Er kommt am Samstag mit Sophie und Kevin zum Grillen.",
    "Ich habe drei neue E-Mails von GitHub, Amazon und der Deutschen Bahn für dich gefunden.",
    "Das Wetter in München: heute bis 18 °C, morgen Regen mit 80 % Wahrscheinlichkeit.",
    "Soll ich die Präsentation an Frau Dr. Schneider schicken, oder lieber erst morgen früh?",
];

fn dir() -> PathBuf {
    PathBuf::from(std::env::var("QWEN_TTS_DIR").expect("QWEN_TTS_DIR names the model folder"))
}

/// The catalog entry, or the same entry with the Q4_K_M backbone (`QWEN_TTS_QUANT`).
fn entry() -> TtsCatalogEntry {
    let mut entry = get("qwen3-tts-1.7b").expect("in the catalog").clone();
    if std::env::var("QWEN_TTS_QUANT").as_deref() == Ok("Q4_K_M") {
        let files = entry.files.as_mut().expect("pinned files");
        files[0] = CatalogFile {
            name: "Qwen3-TTS-12Hz-1.7B-Base-Q4_K_M.gguf".to_owned(),
            bytes: 1_035_965_280,
            sha256: "8d18c94acb2addd042f97da63c98be144eafa76d0d9495177eab65130cf85129".to_owned(),
        };
    }
    entry
}

fn lena() -> Reference {
    let voice = builtin()
        .expect("voices")
        .iter()
        .find(|v| v.id == "de-lena")
        .expect("Lena is shipped");
    reference(voice).expect("Lena's recording")
}

fn load(placement: Placement) -> QwenModel {
    let started = Instant::now();
    let model = QwenModel::load(&dir(), &entry(), placement).expect("model loads");
    eprintln!("loaded in {:.1} s", started.elapsed().as_secs_f64());
    model
}

/// Speaks the R23 sentences, writes `<tag>-<n>.wav` and `timing-<tag>.txt` to `QWEN_TTS_OUT` and
/// returns computing time and audio length per sentence.
fn speak_r23(model: &QwenModel, tag: &str) -> Vec<(Duration, f64)> {
    let out = std::env::var("QWEN_TTS_OUT").ok().map(PathBuf::from);
    let voice = lena();
    let mut timings = Vec::new();
    let mut report = String::new();
    for (i, sentence) in R23.iter().enumerate() {
        let started = Instant::now();
        let samples = model.generate(sentence, &voice, 1.0).expect("speaks");
        let took = started.elapsed();
        let audio = samples.len() as f64 / f64::from(model.sample_rate());
        eprintln!(
            "{tag} {}: {:.2} s for {audio:.2} s of speech ({:.2})",
            i + 1,
            took.as_secs_f64(),
            took.as_secs_f64() / audio
        );
        report.push_str(&format!(
            "{tag} {} {:.2} {audio:.2}\n",
            i + 1,
            took.as_secs_f64()
        ));
        if let Some(out) = &out {
            let path = out.join(format!("{tag}-{}.wav", i + 1));
            assert!(
                sherpa_onnx::write(
                    path.to_str().expect("UTF-8"),
                    &samples,
                    model.sample_rate() as i32,
                ),
                "write {}",
                path.display()
            );
        }
        timings.push((took, audio));
    }
    if let Some(out) = &out {
        std::fs::write(out.join(format!("timing-{tag}.txt")), report).expect("timing");
    }
    timings
}

fn ratio(timings: &[(Duration, f64)]) -> f64 {
    let took: f64 = timings.iter().map(|(t, _)| t.as_secs_f64()).sum();
    let audio: f64 = timings.iter().map(|(_, a)| a).sum();
    took / audio
}

#[test]
#[ignore = "needs the Qwen3-TTS files, see the module doc"]
fn loads_and_reports_its_sample_rate() {
    let model = load(Placement::Cpu);
    assert_eq!(model.sample_rate(), 24_000);
    assert_eq!(model.part_words(), Some(20));
}

#[test]
#[ignore = "needs the Qwen3-TTS files, see the module doc"]
fn speaks_r23_sentences() {
    let model = load(Placement::Cpu);
    let tag = std::env::var("QWEN_TTS_QUANT").unwrap_or_else(|_| "Q8_0".to_owned());
    let timings = speak_r23(&model, &format!("qwen-{tag}-cpu"));
    for (sentence, (_, audio)) in R23.iter().zip(&timings) {
        // At least 0.035 s per character: nothing ended after a few words (research R6).
        assert!(
            *audio > 0.035 * sentence.chars().count() as f64,
            "{sentence}"
        );
    }
    eprintln!("ratio {:.2}", ratio(&timings));
}

#[test]
#[ignore = "needs the Qwen3-TTS files, see the module doc"]
fn cancel_stops_within_one_frame() {
    let model = load(Placement::Cpu);
    let voice = lena();
    // What one frame takes here: an uncancelled sentence's time over its frames (an upper bound,
    // since it includes reading the prompt).
    let started = Instant::now();
    let spoken = model.generate(R23[0], &voice, 1.0).expect("speaks");
    let frames = spoken.len() as f64 / f64::from(model.sample_rate()) * FRAMES_PER_SECOND;
    let per_frame = started.elapsed().as_secs_f64() / frames.max(1.0);
    // The stop comes at the 20th look, which is inside the frame loop (the prompt fits one step).
    let looks = Cell::new(0u32);
    let requested = Cell::new(None::<Instant>);
    let samples = model
        .speak_part(R23[1], &voice, 1.0, 0, &|| {
            looks.set(looks.get() + 1);
            if looks.get() == 20 {
                requested.set(Some(Instant::now()));
            }
            looks.get() >= 20
        })
        .expect("stops cleanly");
    let latency = requested.get().expect("the stop was requested").elapsed();
    assert!(samples.is_empty());
    assert_eq!(looks.get(), 20, "no frame ran after the stop");
    assert!(
        latency.as_secs_f64() < per_frame,
        "{latency:?} against {per_frame:.3} s per frame"
    );
}

/// Counts what the speaker asks of the engine, to see how often a part needed another attempt.
struct Counting<'a> {
    model: &'a QwenModel,
    calls: std::sync::Mutex<Vec<u32>>,
}

impl Engine for Counting<'_> {
    fn sample_rate(&self) -> u32 {
        self.model.sample_rate()
    }

    fn generate(
        &self,
        text: &str,
        reference: &Reference,
        rate: f32,
    ) -> Result<Vec<f32>, holzi_lib::tts::TtsError> {
        self.model.generate(text, reference, rate)
    }

    fn part_words(&self) -> Option<usize> {
        self.model.part_words()
    }

    fn speak_part(
        &self,
        words: &str,
        reference: &Reference,
        rate: f32,
        attempt: u32,
        cancelled: &dyn Fn() -> bool,
    ) -> Result<Vec<f32>, holzi_lib::tts::TtsError> {
        self.calls.lock().expect("calls").push(attempt);
        self.model
            .speak_part(words, reference, rate, attempt, cancelled)
    }
}

/// 180 sentences of the kind an agent says, with names, times, dates and amounts, and 20 of
/// more than 40 words (spec 081 SC-006).
fn two_hundred() -> Vec<String> {
    let names = [
        "Jonas",
        "Sophie",
        "Frau Dr. Schneider",
        "Kevin",
        "Herr Yilmaz",
        "Lena",
    ];
    let things = [
        "die Rechnung über 1.249,90 €",
        "den Termin um 14:30 Uhr",
        "das Paket vom 04.11.2026",
        "die Präsentation mit 23 Folien",
        "den Vertrag über 3,5 % Zinsen",
        "die Reise am 12.03. nach München",
    ];
    let verbs = [
        "hat dir geschrieben wegen",
        "fragt nach",
        "erinnert dich an",
        "hat abgesagt:",
        "bestätigt",
    ];
    let mut out = Vec::new();
    for (i, name) in names.iter().cycle().enumerate().take(180) {
        let thing = things[i % things.len()];
        let verb = verbs[i % verbs.len()];
        out.push(format!("{name} {verb} {thing}."));
    }
    for i in 0..20 {
        let name = names[i % names.len()];
        out.push(format!(
            "{name} hat heute Morgen angerufen und gesagt, dass {} noch nicht bezahlt ist, \
             dass er deshalb am Freitag um 9:15 Uhr noch einmal vorbeikommen möchte, \
             und ob du ihm bis dahin die Unterlagen von {} schicken kannst, \
             damit er alles in Ruhe vorbereiten und mit dem Steuerberater besprechen kann.",
            things[i % things.len()],
            names[(i + 1) % names.len()]
        ));
    }
    out
}

#[test]
#[ignore = "needs the Qwen3-TTS files, see the module doc; takes about half an hour on a processor"]
fn two_hundred_sentences() {
    use holzi_lib::tts::check::Level;
    use holzi_lib::tts::speaker::{speak_sentence, Voices};

    let model = load(Placement::Cpu);
    let counting = Counting {
        model: &model,
        calls: std::sync::Mutex::new(Vec::new()),
    };
    let speech = Level;
    let reference = lena();
    let sentences = two_hundred();
    let mut parts = 0;
    let mut audio = 0.0;
    let started = Instant::now();
    for sentence in &sentences {
        let voices = Voices {
            engine: &counting,
            reserve: None,
            reference: &reference,
            rate: 1.0,
            speech: Some(&speech),
            reserve_notice: None,
            part_spoken: None,
        };
        speak_sentence(voices, sentence, &|| false, &mut |samples, rate| {
            parts += 1;
            audio += samples.len() as f64 / f64::from(rate);
            true
        })
        .expect("speaks");
    }
    let calls = counting.calls.lock().expect("calls").clone();
    let retries = calls.iter().filter(|a| **a > 0).count();
    let extra_calls = calls.len() - parts;
    eprintln!(
        "{} sentences, {parts} parts played, {} engine calls, {retries} second attempts, \
         {extra_calls} calls more than parts, {audio:.0} s of speech in {:.0} s",
        sentences.len(),
        calls.len(),
        started.elapsed().as_secs_f64()
    );
    assert!(
        retries <= 2,
        "SC-006 allows at most 2 second attempts, got {retries}"
    );
}

/// Why parts fail their check: every part of a sample of the 200 sentences once, with its length
/// against the bounds and its share of speech (spec 081 research R6).
#[test]
#[ignore = "needs the Qwen3-TTS files, see the module doc"]
fn verdicts() {
    use holzi_lib::tts::check::{judge, max_secs, min_secs, Level, SpeechShare};
    use holzi_lib::tts::parts::{plain, split};
    use holzi_lib::tts::spoken::spoken_protected;

    let model = load(Placement::Cpu);
    let speech = Level;
    let reference = lena();
    let out = std::env::var("QWEN_TTS_OUT").ok().map(PathBuf::from);
    let sentences = two_hundred();
    let sample: Vec<&String> = sentences
        .iter()
        .step_by(15)
        .chain(sentences.iter().skip(180).step_by(7))
        .collect();
    for (n, sentence) in sample.iter().enumerate() {
        let words = spoken_protected(sentence, "de");
        for (p, part) in split(&words, "de", 20).iter().enumerate() {
            let text = plain(part);
            let samples = model
                .speak_part(&text, &reference, 1.0, 0, &|| false)
                .expect("speaks");
            let rate = model.sample_rate();
            let chars = text.chars().count();
            let secs = samples.len() as f64 / f64::from(rate);
            let share = speech.speech_share(&samples, rate);
            let verdict = judge(&samples, rate, chars, Some(&speech));
            eprintln!(
                "{n}.{p} {verdict:?}: {secs:.2} s for {chars} chars (min {:.2}, max {:.2}), speech {share:?}: {text}",
                min_secs(chars),
                max_secs(chars)
            );
            if let Some(out) = &out {
                let path = out.join(format!("verdict-{n}-{p}.wav"));
                assert!(
                    sherpa_onnx::write(path.to_str().expect("UTF-8"), &samples, rate as i32),
                    "write {}",
                    path.display()
                );
            }
        }
    }
}
