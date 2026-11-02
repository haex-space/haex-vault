//! The password generator (spec 034, US3, FR-013, FR-014; spec 065 FR-036). Its one
//! implementation is the crate `haex-passgen` of the vault-sdk, shared with the browser extension
//! haex-pass-browser; holzi only chooses the configuration. The window generates through
//! `passwords_generate`, the agent browser with the default preset when it creates a login.

use haex_passgen::{generate_password, GeneratorConfig};
pub use haex_passgen::{GeneratorError, MAX_LENGTH};
use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::passwords::model::Preset;

/// What to generate, as the window sends it: the fields of a preset.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, TS)]
#[ts(export, export_to = "../../src/types/bindings/")]
#[serde(rename_all = "camelCase")]
pub struct GeneratorSettings {
    pub length: u32,
    pub uppercase: bool,
    pub lowercase: bool,
    pub numbers: bool,
    pub symbols: bool,
    pub exclude_chars: String,
    pub use_pattern: bool,
    pub pattern: String,
}

impl From<GeneratorSettings> for GeneratorConfig {
    fn from(settings: GeneratorSettings) -> Self {
        Self {
            length: settings.length,
            uppercase: settings.uppercase,
            lowercase: settings.lowercase,
            numbers: settings.numbers,
            symbols: settings.symbols,
            exclude_chars: settings.exclude_chars,
            use_pattern: settings.use_pattern,
            pattern: settings.pattern,
        }
    }
}

/// A generated password, or the code of what has to change (`length_range`, `no_class`,
/// `length_below_classes`, `class_empty`, `pattern_empty`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(export, export_to = "../../src/types/bindings/")]
#[serde(untagged)]
pub enum Generated {
    Value { value: String },
    Error { error: String },
}

/// A password for `settings`.
pub fn generate(settings: GeneratorSettings) -> Generated {
    match generate_password(&settings.into()) {
        Ok(value) => Generated::Value { value },
        Err(error) => Generated::Error {
            error: error.as_str().to_owned(),
        },
    }
}

/// The configuration of `preset`.
pub fn config_of(preset: &Preset) -> GeneratorConfig {
    GeneratorConfig {
        length: preset.length,
        uppercase: preset.uppercase,
        lowercase: preset.lowercase,
        numbers: preset.numbers,
        symbols: preset.symbols,
        exclude_chars: preset.exclude_chars.clone(),
        use_pattern: preset.use_pattern,
        pattern: preset.pattern.clone(),
    }
}

/// A password from the default preset of `presets` (the first flagged one, as the list shows
/// it); without one, or when it allows no password, from 20 characters of all four classes.
pub fn generate_default(presets: &[Preset]) -> String {
    presets
        .iter()
        .find(|preset| preset.is_default)
        .and_then(|preset| generate_password(&config_of(preset)).ok())
        .unwrap_or_else(|| {
            generate_password(&GeneratorConfig::default())
                .expect("the built-in configuration always allows a password")
        })
}
