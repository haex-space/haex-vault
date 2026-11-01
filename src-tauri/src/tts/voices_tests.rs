//! Spec 066 T139: the built-in voices and how a preference resolves to one.

use std::collections::HashSet;

use super::commands::{voice_options, VoiceKind};
use super::voices::{
    audio, audio_files, builtin, decode_wav, default_voice, resolve, VoiceLicense, VoiceRef,
};
use super::LANGUAGES;
use crate::hardware::{Backend, HardwareInfo};

const README: &str = include_str!("voices/README.md");

fn hw() -> HardwareInfo {
    HardwareInfo {
        backend: Backend::Cpu,
        total_ram_bytes: 16 << 30,
        available_ram_bytes: 16 << 30,
        vram_bytes: None,
    }
}

#[test]
fn every_language_has_two_to_three_voices_and_one_default() {
    let all = builtin().expect("voices parse");
    for language in LANGUAGES {
        let voices: Vec<_> = all.iter().filter(|v| v.language == language).collect();
        assert!(
            (2..=3).contains(&voices.len()),
            "{language}: {} voices",
            voices.len()
        );
        assert_eq!(
            voices.iter().filter(|v| v.default).count(),
            1,
            "{language}: exactly one default"
        );
    }
    assert!(all.iter().all(|v| LANGUAGES.contains(&v.language.as_str())));
}

#[test]
fn every_voice_is_free_credited_and_documented() {
    let all = builtin().expect("voices parse");
    let mut ids = HashSet::new();
    for voice in all {
        assert!(ids.insert(&voice.id), "duplicate id {}", voice.id);
        assert!(voice.id.starts_with(&format!("{}-", voice.language)));
        assert_eq!(voice.file, format!("{}.wav", voice.id));
        assert!(!voice.name.trim().is_empty());
        assert!(matches!(
            voice.license,
            VoiceLicense::Cc0 | VoiceLicense::CcBy4
        ));
        assert!(!voice.attribution.trim().is_empty(), "{}", voice.id);
        assert!(voice.source_url.starts_with("https://"), "{}", voice.id);
        assert!(README.contains(&voice.id), "README misses {}", voice.id);
    }
    assert!(
        !serde_json::to_string(all)
            .expect("serializes")
            .contains("NC"),
        "no non-commercial source"
    );
}

#[test]
fn every_recording_ships_and_is_a_usable_reference() {
    let all = builtin().expect("voices parse");
    let listed: HashSet<_> = all.iter().map(|v| v.file.as_str()).collect();
    let shipped: HashSet<_> = audio_files().collect();
    assert_eq!(listed, shipped, "voices.json and the shipped files differ");
    for voice in all {
        let pcm = decode_wav(audio(voice).expect("recording ships")).expect("decodes");
        assert_eq!(pcm.channels, 1, "{} is mono", voice.id);
        let secs = pcm.samples.len() as f64 / f64::from(pcm.sample_rate);
        assert!((5.0..=20.0).contains(&secs), "{}: {secs:.1} s", voice.id);
        let peak = pcm.samples.iter().fold(0.0_f32, |m, s| m.max(s.abs()));
        assert!(peak > 0.1 && peak < 1.0, "{}: peak {peak}", voice.id);
    }
}

#[test]
fn broken_recordings_are_refused() {
    assert!(decode_wav(b"").is_err());
    assert!(decode_wav(b"RIFF\0\0\0\0WAVE").is_err());
    let mut float = Vec::new();
    float.extend_from_slice(b"RIFF\x24\0\0\0WAVEfmt \x10\0\0\0");
    float.extend_from_slice(&3u16.to_le_bytes()); // IEEE float
    float.extend_from_slice(&1u16.to_le_bytes());
    float.extend_from_slice(&24_000u32.to_le_bytes());
    float.extend_from_slice(&96_000u32.to_le_bytes());
    float.extend_from_slice(&4u16.to_le_bytes());
    float.extend_from_slice(&32u16.to_le_bytes());
    float.extend_from_slice(b"data\0\0\0\0");
    assert!(decode_wav(&float).is_err());
}

#[test]
fn stereo_recordings_are_mixed_down() {
    let mut wav = Vec::new();
    wav.extend_from_slice(b"RIFF\x2c\0\0\0WAVEfmt \x10\0\0\0");
    wav.extend_from_slice(&1u16.to_le_bytes());
    wav.extend_from_slice(&2u16.to_le_bytes());
    wav.extend_from_slice(&16_000u32.to_le_bytes());
    wav.extend_from_slice(&64_000u32.to_le_bytes());
    wav.extend_from_slice(&4u16.to_le_bytes());
    wav.extend_from_slice(&16u16.to_le_bytes());
    wav.extend_from_slice(b"data\x08\0\0\0");
    for s in [16_384i16, 0, -16_384, -16_384] {
        wav.extend_from_slice(&s.to_le_bytes());
    }
    let pcm = decode_wav(&wav).expect("decodes");
    assert_eq!((pcm.channels, pcm.sample_rate), (2, 16_000));
    assert_eq!(pcm.samples, [0.25, -0.5]);
}

#[test]
fn voice_refs_parse_and_print_back() {
    for (text, parsed) in [
        ("builtin:de-lena", VoiceRef::Builtin("de-lena".into())),
        ("clone:7f3a", VoiceRef::Clone("7f3a".into())),
        (
            "model:thorsten",
            VoiceRef::Model {
                id: "thorsten".into(),
                speaker: None,
            },
        ),
        (
            "model:vctk#p225",
            VoiceRef::Model {
                id: "vctk".into(),
                speaker: Some("p225".into()),
            },
        ),
    ] {
        assert_eq!(VoiceRef::parse(text).as_ref(), Some(&parsed), "{text}");
        assert_eq!(parsed.to_string(), text);
    }
    for bad in [
        "",
        "de-lena",
        "builtin:",
        "builtin:de lena",
        "builtin:a#b",
        "clone:",
        "model:",
        "model:#p225",
        "model:vctk#",
        "voice:de-lena",
    ] {
        assert_eq!(VoiceRef::parse(bad), None, "{bad:?}");
    }
}

#[test]
fn a_preference_resolves_to_its_voice_or_the_language_default() {
    let default_de = default_voice("de").expect("parses").expect("de has one");
    let other_de = builtin()
        .expect("parses")
        .iter()
        .find(|v| v.language == "de" && !v.default)
        .expect("a second German voice");

    let pick = |pref: Option<&str>| resolve(pref, "de").expect("resolves").id.as_str();
    assert_eq!(pick(Some(&format!("builtin:{}", other_de.id))), other_de.id);
    for fallback in [
        None,
        Some("builtin:gone"),
        Some("builtin:en-caro"),
        Some("clone:deleted"),
        Some("model:never-imported#x"),
        Some("garbage"),
    ] {
        assert_eq!(pick(fallback), default_de.id, "{fallback:?}");
    }
    assert!(resolve(None, "nl").is_err());
}

#[test]
fn every_voice_is_offered_with_the_model_it_needs() {
    let all = builtin().expect("parses");
    let installed = vec!["pocket-de".to_owned()];
    let options = voice_options(all, &installed, &hw()).expect("options");
    assert_eq!(options.len(), all.len());
    for (option, voice) in options.iter().zip(all) {
        assert_eq!(option.voice_ref, format!("builtin:{}", voice.id));
        assert_eq!(option.kind, VoiceKind::Builtin);
        assert_eq!(option.language, voice.language);
        assert_eq!(option.is_default, voice.default);
        assert_eq!(option.installed, voice.language == "de");
        assert!(option.size_bytes > 0);
        let expected_model = if voice.language == "fr" {
            "pocket-fr-24l".to_owned()
        } else {
            format!("pocket-{}", voice.language)
        };
        assert_eq!(option.model_id, expected_model);
    }
    let wire = serde_json::to_value(&options[0]).expect("serializes");
    for key in [
        "ref",
        "kind",
        "name",
        "language",
        "modelId",
        "installed",
        "sizeBytes",
        "fit",
        "isDefault",
    ] {
        assert!(wire.get(key).is_some(), "{key}");
    }
    assert_eq!(wire["kind"], "builtin");
}

/// Listening check after changing the voices: every built-in voice says two assistant sentences
/// with the model of its language. `HOLZI_TTS_MODELS` holds one model folder per language
/// (`de`, `en`, `fr`, …, files as `tts::files` names them); the WAVs go to `HOLZI_TTS_SAVE_DIR`.
#[cfg(feature = "voice")]
#[test]
#[ignore = "needs the six speech models; listening check"]
fn builtin_voices_speak() {
    use super::local::{Engine, PocketModel};
    use super::voices::reference;
    use std::path::PathBuf;

    let var = |key: &str| PathBuf::from(std::env::var(key).expect(key));
    let (models, save) = (var("HOLZI_TTS_MODELS"), var("HOLZI_TTS_SAVE_DIR"));
    let sentences = |language: &str| match language {
        "de" => [
            "Du hast morgen um halb zehn einen Termin beim Zahnarzt.",
            "Soll ich dich eine Stunde vorher erinnern?",
        ],
        "en" => [
            "You have a dentist appointment tomorrow at half past nine.",
            "Should I remind you an hour before?",
        ],
        "fr" => [
            "Tu as rendez-vous chez le dentiste demain à neuf heures et demie.",
            "Veux-tu que je te le rappelle une heure avant ?",
        ],
        "es" => [
            "Mañana a las nueve y media tienes cita con el dentista.",
            "¿Quieres que te lo recuerde una hora antes?",
        ],
        "it" => [
            "Domani alle nove e mezza hai un appuntamento dal dentista.",
            "Vuoi che te lo ricordi un'ora prima?",
        ],
        _ => [
            "Amanhã às nove e meia você tem consulta no dentista.",
            "Quer que eu te lembre uma hora antes?",
        ],
    };
    for language in LANGUAGES {
        let model = PocketModel::load(&models.join(language)).expect("model loads");
        for voice in builtin()
            .expect("parses")
            .iter()
            .filter(|v| v.language == language)
        {
            let reference = reference(voice).expect("reference");
            let mut samples = Vec::new();
            for sentence in sentences(language) {
                samples.extend(model.generate(sentence, &reference, 1.0).expect("speaks"));
                samples.extend(std::iter::repeat_n(
                    0.0,
                    model.sample_rate() as usize * 3 / 10,
                ));
            }
            let seconds = samples.len() as f64 / f64::from(model.sample_rate());
            assert!(seconds > 2.0, "{}: {seconds:.1} s", voice.id);
            let path = save.join(&voice.file);
            assert!(sherpa_onnx::write(
                path.to_str().expect("utf-8"),
                &samples,
                model.sample_rate() as i32
            ));
            println!("{}: {seconds:.1} s", voice.id);
        }
    }
}
