use super::generator::{generate, generate_default, Generated, GeneratorSettings};
use super::model::Preset;

fn settings() -> GeneratorSettings {
    GeneratorSettings {
        length: 12,
        uppercase: false,
        lowercase: true,
        numbers: false,
        symbols: false,
        exclude_chars: String::new(),
        use_pattern: false,
        pattern: String::new(),
    }
}

fn preset(id: &str, length: u32, is_default: bool) -> Preset {
    Preset {
        id: id.to_owned(),
        name: id.to_owned(),
        length,
        uppercase: false,
        lowercase: false,
        numbers: true,
        symbols: false,
        exclude_chars: String::new(),
        use_pattern: false,
        pattern: String::new(),
        is_default,
    }
}

#[test]
fn the_window_gets_a_password_for_its_settings() {
    let Generated::Value { value } = generate(settings()) else {
        panic!("a value");
    };
    assert_eq!(value.len(), 12);
    assert!(value.chars().all(|c| c.is_ascii_lowercase()));
}

#[test]
fn settings_that_allow_no_password_answer_with_the_code() {
    let none = GeneratorSettings {
        lowercase: false,
        ..settings()
    };
    assert_eq!(
        generate(none),
        Generated::Error {
            error: "no_class".to_owned()
        }
    );
    let json = serde_json::to_value(generate(GeneratorSettings {
        length: 0,
        ..settings()
    }))
    .unwrap();
    assert_eq!(json, serde_json::json!({ "error": "length_range" }));
}

#[test]
fn a_new_login_uses_the_default_preset() {
    let presets = [preset("a", 30, false), preset("b", 9, true)];
    let password = generate_default(&presets);
    assert_eq!(password.len(), 9);
    assert!(password.chars().all(|c| c.is_ascii_digit()));
}

#[test]
fn without_a_usable_default_preset_a_new_login_gets_twenty_characters() {
    assert_eq!(generate_default(&[]).chars().count(), 20);
    let broken = [preset("b", 0, true)];
    assert_eq!(generate_default(&broken).chars().count(), 20);
}
