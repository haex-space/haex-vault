//! Spec 066 T016: readiness and the shipped pause detection model.

use sha2::{Digest, Sha256};
use std::sync::{Arc, Barrier};
use std::thread;
use tempfile::tempdir;

use super::assets::{missing, voice_language, write_vad_model, Facts, Missing, SILERO_VAD};

const ALL_PRESENT: Facts = Facts {
    stt_model: true,
    vad_model: true,
    wake_word_model: true,
    voice: true,
    chat_model: true,
    microphone: true,
};

#[test]
fn nothing_is_missing_when_everything_is_there() {
    assert!(missing(&ALL_PRESENT).is_empty());
}

#[test]
fn readiness_names_exactly_what_is_missing() {
    let wire = |items: Vec<Missing>| serde_json::to_value(items).expect("serializes");
    assert_eq!(
        wire(missing(&Facts::default())),
        serde_json::json!([
            "sttModel",
            "vadModel",
            "wakeWordModel",
            "voice",
            "chatModel",
            "microphone"
        ])
    );
    type Unset = fn(&mut Facts);
    let cases: [(Unset, Missing); 6] = [
        (|f| f.stt_model = false, Missing::SttModel),
        (|f| f.vad_model = false, Missing::VadModel),
        (|f| f.wake_word_model = false, Missing::WakeWordModel),
        (|f| f.voice = false, Missing::Voice),
        (|f| f.chat_model = false, Missing::ChatModel),
        (|f| f.microphone = false, Missing::Microphone),
    ];
    for (unset, item) in cases {
        let mut facts = ALL_PRESENT;
        unset(&mut facts);
        assert_eq!(missing(&facts), [item]);
    }
}

#[test]
fn the_voice_follows_the_window_language() {
    assert_eq!(voice_language("de"), "de");
    assert_eq!(voice_language("de-DE"), "de");
    assert_eq!(voice_language("pt_BR"), "pt");
    assert_eq!(voice_language("FR"), "fr");
    assert_eq!(voice_language("nl"), "en");
    assert_eq!(voice_language(""), "en");
}

#[test]
fn the_shipped_model_is_the_documented_file() {
    let digest = Sha256::digest(SILERO_VAD);
    let hex: String = digest.iter().map(|b| format!("{b:02x}")).collect();
    assert_eq!(
        hex,
        "c36d490aff5ab924ca6c7aeec4d8f6bd3d22db6fa17611b9c5b17eae58ac3a20"
    );
}

#[test]
fn the_vad_model_is_written_once_and_repaired() {
    let dir = tempdir().expect("temp dir");
    let target = dir.path().join("silero-vad");
    let path = write_vad_model(&target).expect("writes");
    assert_eq!(std::fs::read(&path).expect("reads"), SILERO_VAD);
    let written = std::fs::metadata(&path).expect("meta").modified().ok();

    assert_eq!(write_vad_model(&target).expect("writes"), path);
    assert_eq!(
        std::fs::metadata(&path).expect("meta").modified().ok(),
        written,
        "an identical copy stays untouched"
    );

    std::fs::write(&path, b"broken").expect("corrupt");
    write_vad_model(&target).expect("repairs");
    assert_eq!(std::fs::read(&path).expect("reads"), SILERO_VAD);
}

#[test]
fn concurrent_vad_model_writes_publish_a_complete_model() {
    let dir = tempdir().expect("temp dir");
    let model_dir = dir.path().join("silero-vad");
    let target = model_dir.join("silero_vad.int8.onnx");
    let barrier = Arc::new(Barrier::new(8));
    let writers: Vec<_> = (0..8)
        .map(|_| {
            let (model_dir, barrier) = (model_dir.clone(), Arc::clone(&barrier));
            thread::spawn(move || {
                barrier.wait();
                write_vad_model(&model_dir).expect("writes model")
            })
        })
        .collect();
    for writer in writers {
        assert_eq!(writer.join().expect("writer completes"), target);
    }
    assert_eq!(std::fs::read(target).expect("reads model"), SILERO_VAD);
}
