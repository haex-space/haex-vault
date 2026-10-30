//! Spike for local speech output (spec 066, research R8, T008): Kyutai Pocket TTS through
//! sherpa-onnx in the six spoken languages. Skipped by default (`#[ignore]`): it needs the models
//! and reference recordings on disk.
//!
//!   HOLZI_TTS_EVAL_MODELS="en=<dir>,de=<dir>,…" \
//!   HOLZI_TTS_EVAL_REFS=<dir with <lang>.wav, one native speaker each> \
//!   HOLZI_PARAKEET_DIR=<unpacked sherpa-onnx-nemo-parakeet-tdt-0.6b-v3-int8> \
//!     cargo test --release --manifest-path src-tauri/Cargo.toml --test tts_spike_eval \
//!       -- --ignored --nocapture
//!
//! A model dir is either the k2-fsa export (`lm_flow.int8.onnx` …) or a KevinAHM/pocket-tts-onnx
//! bundle (`flow_lm_flow_int8.onnx` …) with `vocab.json` and `token_scores.json` converted from
//! its `tokenizer.model` (sherpa-onnx `scripts/pocket-tts/convert_tokenizer.py`). Per sentence it
//! measures the time to the first audio and the real-time factor, writes the audio to
//! `target/eval/tts/<lang>/<n>.wav` for listening, and lets Parakeet transcribe it back: the word
//! error rate of that round trip is a rough measure of how understandable the speech is.

#![cfg(feature = "voice")]

use std::env;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::Instant;

use holzi_lib::stt::sherpa::ParakeetAdapter;
use holzi_lib::stt::{CanonicalPcm, SttAdapter, SAMPLE_RATE_HZ};
use serde_json::json;
use sherpa_onnx::{
    GenerationConfig, LinearResampler, OfflineTts, OfflineTtsConfig, OfflineTtsModelConfig,
    OfflineTtsPocketModelConfig, Wave,
};

fn sentences(language: &str) -> [&'static str; 5] {
    match language {
        "de" => [
            "Alles klar, ich schaue mir die Mails von Jonas an.",
            "Du hast morgen um halb zehn einen Termin beim Zahnarzt.",
            "Soll ich die Datei wirklich löschen? Sag bitte ja oder nein.",
            "Fertig. Ich habe drei Einträge gefunden und den neuesten geöffnet.",
            "Das Wetter in München: zwölf Grad, am Nachmittag leichter Regen.",
        ],
        "en" => [
            "Alright, I'll look through the emails from Jonas.",
            "You have a dentist appointment tomorrow at half past nine.",
            "Should I really delete the file? Please say yes or no.",
            "Done. I found three entries and opened the newest one.",
            "The weather in Munich: twelve degrees, light rain in the afternoon.",
        ],
        "fr" => [
            "D'accord, je regarde les e-mails de Jonas.",
            "Tu as rendez-vous chez le dentiste demain à neuf heures et demie.",
            "Dois-je vraiment supprimer le fichier ? Réponds oui ou non, s'il te plaît.",
            "C'est fait. J'ai trouvé trois entrées et ouvert la plus récente.",
            "La météo à Munich : douze degrés, un peu de pluie l'après-midi.",
        ],
        "es" => [
            "De acuerdo, voy a revisar los correos de Jonas.",
            "Mañana tienes cita con el dentista a las nueve y media.",
            "¿De verdad quieres que borre el archivo? Di sí o no, por favor.",
            "Listo. Encontré tres entradas y abrí la más reciente.",
            "El tiempo en Múnich: doce grados, lluvia ligera por la tarde.",
        ],
        "it" => [
            "Va bene, do un'occhiata alle email di Jonas.",
            "Domani alle nove e mezza hai un appuntamento dal dentista.",
            "Devo davvero cancellare il file? Rispondi sì o no, per favore.",
            "Fatto. Ho trovato tre voci e ho aperto la più recente.",
            "Il meteo a Monaco: dodici gradi, pioggia leggera nel pomeriggio.",
        ],
        "pt" => [
            "Tudo bem, vou ver os e-mails do Jonas.",
            "Amanhã você tem consulta no dentista às nove e meia.",
            "Devo mesmo apagar o arquivo? Diga sim ou não, por favor.",
            "Pronto. Encontrei três entradas e abri a mais recente.",
            "O tempo em Munique: doze graus, chuva fraca à tarde.",
        ],
        other => panic!("no sentences for {other}"),
    }
}

/// The first existing file of `names` in `dir`.
fn pick(dir: &Path, names: &[&str]) -> String {
    names
        .iter()
        .map(|name| dir.join(name))
        .find(|path| path.is_file())
        .unwrap_or_else(|| panic!("none of {names:?} in {}", dir.display()))
        .to_str()
        .expect("utf-8 path")
        .to_owned()
}

fn pocket(dir: &Path) -> OfflineTts {
    let config = OfflineTtsConfig {
        model: OfflineTtsModelConfig {
            pocket: OfflineTtsPocketModelConfig {
                lm_flow: Some(pick(dir, &["lm_flow.int8.onnx", "flow_lm_flow_int8.onnx"])),
                lm_main: Some(pick(dir, &["lm_main.int8.onnx", "flow_lm_main_int8.onnx"])),
                encoder: Some(pick(dir, &["encoder.onnx", "mimi_encoder_int8.onnx"])),
                decoder: Some(pick(dir, &["decoder.int8.onnx", "mimi_decoder_int8.onnx"])),
                text_conditioner: Some(pick(
                    dir,
                    &["text_conditioner.onnx", "text_conditioner_int8.onnx"],
                )),
                vocab_json: Some(pick(dir, &["vocab.json"])),
                token_scores_json: Some(pick(dir, &["token_scores.json"])),
                voice_embedding_cache_capacity: 8,
            },
            num_threads: std::thread::available_parallelism()
                .map(|n| n.get().min(4) as i32)
                .unwrap_or(1),
            ..Default::default()
        },
        ..Default::default()
    };
    OfflineTts::create(&config).unwrap_or_else(|| panic!("Pocket loads from {}", dir.display()))
}

fn words(text: &str) -> Vec<String> {
    text.to_lowercase()
        .split(|c: char| !c.is_alphanumeric())
        .filter(|w| !w.is_empty())
        .map(str::to_owned)
        .collect()
}

fn word_errors(reference: &[String], hypothesis: &[String]) -> usize {
    let mut previous: Vec<usize> = (0..=hypothesis.len()).collect();
    for (i, r) in reference.iter().enumerate() {
        let mut current = vec![i + 1; hypothesis.len() + 1];
        for (j, h) in hypothesis.iter().enumerate() {
            current[j + 1] = (previous[j] + usize::from(r != h))
                .min(previous[j + 1] + 1)
                .min(current[j] + 1);
        }
        previous = current;
    }
    previous[hypothesis.len()]
}

#[tokio::test]
#[ignore = "needs Pocket TTS models, reference recordings and Parakeet, see the module doc"]
async fn pocket_tts_in_six_languages() {
    let models = env::var("HOLZI_TTS_EVAL_MODELS").expect("HOLZI_TTS_EVAL_MODELS is set");
    let refs = PathBuf::from(env::var("HOLZI_TTS_EVAL_REFS").expect("HOLZI_TTS_EVAL_REFS is set"));
    let parakeet = ParakeetAdapter::load_from_dir(Path::new(
        &env::var("HOLZI_PARAKEET_DIR").expect("HOLZI_PARAKEET_DIR is set"),
    ))
    .expect("Parakeet loads");
    let out = Path::new(env!("CARGO_MANIFEST_DIR")).join("target/eval/tts");

    let mut rows = Vec::new();
    for pair in models.split(',') {
        let (language, dir) = pair.split_once('=').expect("lang=dir");
        let load_started = Instant::now();
        let tts = pocket(Path::new(dir));
        let load_secs = load_started.elapsed().as_secs_f64();
        let reference = Wave::read(
            refs.join(format!("{language}.wav"))
                .to_str()
                .expect("utf-8"),
        )
        .unwrap_or_else(|| panic!("reference recording for {language}"));
        let to_16k =
            LinearResampler::create(tts.sample_rate(), SAMPLE_RATE_HZ as i32).expect("resampler");
        std::fs::create_dir_all(out.join(language)).expect("output dir");

        for (n, sentence) in sentences(language).iter().enumerate() {
            let config = GenerationConfig {
                reference_audio: Some(reference.samples().to_vec()),
                reference_sample_rate: reference.sample_rate(),
                num_steps: 1,
                extra: Some(
                    [("temperature".to_owned(), json!("0.7"))]
                        .into_iter()
                        .collect(),
                ),
                ..Default::default()
            };
            let first_audio: Arc<Mutex<Option<f64>>> = Arc::new(Mutex::new(None));
            let started = Instant::now();
            let seen = Arc::clone(&first_audio);
            let audio = tts
                .generate_with_config(
                    sentence,
                    &config,
                    Some(move |samples: &[f32], _progress: f32| {
                        if !samples.is_empty() {
                            let mut seen = seen.lock().expect("lock");
                            seen.get_or_insert(started.elapsed().as_secs_f64());
                        }
                        true
                    }),
                )
                .unwrap_or_else(|| panic!("{language} sentence {n} synthesizes"));
            let total = started.elapsed().as_secs_f64();
            let audio_secs = audio.samples().len() as f64 / audio.sample_rate() as f64;
            let wav = out.join(language).join(format!("{n}.wav"));
            audio.save(wav.to_str().expect("utf-8"));

            to_16k.reset();
            let heard = parakeet
                .transcribe(&CanonicalPcm {
                    samples: to_16k.resample(audio.samples(), true),
                })
                .await
                .expect("Parakeet transcribes");
            let reference_words = words(sentence);
            let wer = 100.0 * word_errors(&reference_words, &words(&heard)) as f64
                / reference_words.len() as f64;
            let first = first_audio.lock().expect("lock").unwrap_or(total);
            println!(
                "{language} {n}  first audio {first:5.2} s  total {total:5.2} s  audio {audio_secs:5.2} s  RTF {:5.2}  round-trip WER {wer:5.1} %  heard: {heard}",
                total / audio_secs.max(f64::EPSILON)
            );
            rows.push(json!({
                "language": language, "sentence": n, "text": sentence, "heard": heard,
                "firstAudioSecs": first, "totalSecs": total, "audioSecs": audio_secs,
                "rtf": total / audio_secs.max(f64::EPSILON), "roundTripWer": wer, "loadSecs": load_secs,
            }));
        }
    }
    std::fs::write(
        out.join("report.json"),
        serde_json::to_string_pretty(&rows).expect("json"),
    )
    .expect("write report");
}
