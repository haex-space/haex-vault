//! What a person chose for the context of a local model on this device (spec 086 FR-005): the length
//! (Auto or a fixed number of tokens) and the type of the KV cache. One row of `preferences` per model, in
//! the scope of this device, so the choice follows the vault but belongs to the computer that made it.
//!
//! A row that cannot be read counts as absent, which means Auto and the standard cache, as the choices of
//! the agent browser do (`agent_browser::choices`).

use haex_crdt::CrdtTransaction;
use serde::de::{self, Deserializer};
use serde::{Deserialize, Serialize, Serializer};
use uuid::Uuid;

use crate::llm::llamacpp::choice::{check_fixed, CacheType, ContextChoice, PlanError};
use crate::llm::llamacpp::memory::ModelShape;
use crate::storage::preferences::{self, PrefScope};
use crate::storage::query::Query;

/// The key of a model's row; the rows of all models share the prefix.
const KEY_PREFIX: &str = "models.context.";
/// The least a fixed context may be: the planner's floor.
pub const MIN_FIXED_CONTEXT: u32 = 2048;

/// The key of the row of `model_id`.
pub fn key(model_id: &str) -> String {
    format!("{KEY_PREFIX}{model_id}")
}

/// The context length of a setting: left to the planner, or this many tokens.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ContextLength {
    #[default]
    Auto,
    Tokens(u32),
}

impl Serialize for ContextLength {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        match self {
            Self::Auto => serializer.serialize_str("auto"),
            Self::Tokens(tokens) => serializer.serialize_u32(*tokens),
        }
    }
}

impl<'de> Deserialize<'de> for ContextLength {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        #[serde(untagged)]
        enum Raw {
            Text(String),
            Number(u64),
        }
        match Raw::deserialize(deserializer)? {
            Raw::Text(text) if text == "auto" => Ok(Self::Auto),
            Raw::Text(text) => Err(de::Error::custom(format!("unknown context length {text}"))),
            Raw::Number(tokens) => u32::try_from(tokens)
                .map(Self::Tokens)
                .map_err(|_| de::Error::custom("context length out of range")),
        }
    }
}

/// The choice for one model on one device.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct ContextSetting {
    pub context: ContextLength,
    pub cache: CacheType,
}

impl ContextSetting {
    /// Auto with the standard cache: what no row means.
    pub fn is_default(&self) -> bool {
        *self == Self::default()
    }

    /// The engine's form of the length.
    pub fn choice(&self) -> ContextChoice {
        match self.context {
            ContextLength::Auto => ContextChoice::Auto,
            ContextLength::Tokens(tokens) => ContextChoice::Fixed(tokens),
        }
    }

    /// Whether the setting can be kept: a fixed length is at least the floor.
    pub fn usable(&self) -> bool {
        match self.context {
            ContextLength::Auto => true,
            ContextLength::Tokens(tokens) => tokens >= MIN_FIXED_CONTEXT,
        }
    }
}

/// Whether `setting` may be kept for the model of `shape` (`None` when its header cannot be sized): a fixed
/// length obeys the rules of the choice (spec 086 FR-007, FR-008). Memory is not a rule here: it changes
/// between loads, and the load refuses a choice that does not fit then.
pub fn validate(shape: Option<&ModelShape>, setting: &ContextSetting) -> Result<(), PlanError> {
    match (setting.context, shape) {
        (ContextLength::Auto, _) => Ok(()),
        (ContextLength::Tokens(tokens), Some(shape)) => check_fixed(shape, tokens),
        (ContextLength::Tokens(tokens), None) if tokens < MIN_FIXED_CONTEXT => {
            Err(PlanError::BelowFloor {
                asked: tokens,
                floor: MIN_FIXED_CONTEXT,
            })
        }
        (ContextLength::Tokens(_), None) => Ok(()),
    }
}

/// The setting of `model_id` on `device`; the default while there is none or it cannot be used.
pub fn read(q: &mut impl Query, device: Uuid, model_id: &str) -> haex_crdt::Result<ContextSetting> {
    Ok(
        preferences::get(q, PrefScope::Device(device), &key(model_id))?
            .and_then(|text| serde_json::from_str::<ContextSetting>(&text).ok())
            .filter(ContextSetting::usable)
            .unwrap_or_default(),
    )
}

/// Keeps the setting of `model_id` on `device`; the default removes the row. The caller has checked it
/// against the model.
pub fn write(
    tx: &mut CrdtTransaction<'_>,
    device: Uuid,
    model_id: &str,
    setting: &ContextSetting,
) -> haex_crdt::Result<()> {
    let scope = PrefScope::Device(device);
    if setting.is_default() {
        preferences::delete(tx, scope, &key(model_id))?;
        return Ok(());
    }
    let text = serde_json::to_string(setting)
        .map_err(|error| haex_crdt::Error::consumer(error.to_string()))?;
    preferences::insert_or_update(tx, scope, &key(model_id), &text)?;
    Ok(())
}

#[cfg(test)]
#[path = "context_setting_tests.rs"]
mod tests;
