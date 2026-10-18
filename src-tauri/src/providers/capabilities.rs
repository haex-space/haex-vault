//! What the user states about a model (spec 055 FR-019, research R7).
//!
//! An OpenAI-compatible server's `/models` answer says neither whether a model takes images nor
//! which effort levels it knows. The user states it per model; a refresh keeps the statement
//! (`storage::models::replace_provider_models`).

use serde::Deserialize;
use tauri::State;

use super::OPENAI_COMPATIBLE;
use crate::error::{HolziError, Result};
use crate::model_capabilities::{ModelCapabilities, UserCapabilities};
use crate::state::AppState;
use crate::state_utils::active_database;
use crate::storage::models as models_store;
use crate::storage::providers as providers_store;
use crate::vault_gate::VaultDb;

/// Most effort levels a model can be given by hand, and the longest id.
const MAX_LEVELS: usize = 8;
const MAX_LEVEL_LEN: usize = 32;

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SetUserCapabilitiesArgs {
    pub model_id: String,
    /// `Some(true)`: takes images; `Some(false)`: does not; `None`: not stated.
    #[serde(default)]
    pub accepts_images: Option<bool>,
    /// The effort levels the model takes (provider-native ids, lowest first); `Some(vec![])` for
    /// none; `None`: not stated. Both fields `None` withdraws an earlier statement.
    #[serde(default)]
    pub effort_levels: Option<Vec<String>>,
}

/// Checks the effort ids: short, non-empty, free of blanks and control characters, no repeats.
fn checked_levels(levels: Vec<String>) -> Result<Vec<String>> {
    let invalid = |reason: String| HolziError::InvalidInput { reason };
    if levels.len() > MAX_LEVELS {
        return Err(invalid(format!("at most {MAX_LEVELS} effort levels")));
    }
    let mut seen = std::collections::HashSet::new();
    for level in &levels {
        let usable = !level.is_empty()
            && level.len() <= MAX_LEVEL_LEN
            && level.chars().all(|c| !c.is_whitespace() && !c.is_control());
        if !usable {
            return Err(invalid(
                "an effort level is a short word without blanks".into(),
            ));
        }
        if !seen.insert(level.as_str()) {
            return Err(invalid(format!("effort level {level:?} given twice")));
        }
    }
    Ok(levels)
}

/// Why a statement was not stored.
enum Refused {
    NoModel,
    /// The provider says itself what the model can do; a statement would replace that and
    /// withdrawing it would not bring it back.
    NotOpenAiCompatible,
}

/// Stores the statement and returns the model's record with it laid over. Only a model of an
/// OpenAI-compatible provider takes a statement: its server reports nothing about it.
pub(crate) async fn set_user_capabilities(
    db: &VaultDb,
    args: SetUserCapabilitiesArgs,
) -> Result<ModelCapabilities> {
    let user = UserCapabilities {
        accepts_images: args.accepts_images,
        effort_levels: args.effort_levels.map(checked_levels).transpose()?,
    };
    let id = args.model_id;
    let lookup = id.clone();
    let stored = db
        .write(move |tx| {
            let Some(model) = models_store::get_model(tx, &lookup)? else {
                return Ok(Err(Refused::NoModel));
            };
            let adapter = providers_store::get_provider(tx, model.provider_id)?
                .and_then(|provider| provider.adapter);
            if adapter.as_deref() != Some(OPENAI_COMPATIBLE) {
                return Ok(Err(Refused::NotOpenAiCompatible));
            }
            models_store::set_user_capabilities(tx, &lookup, user)
                .map(|stored| stored.ok_or(Refused::NoModel))
        })
        .await?;
    stored.map_err(|refused| HolziError::InvalidInput {
        reason: match refused {
            Refused::NoModel => format!("model {id} not found"),
            Refused::NotOpenAiCompatible => {
                format!("model {id} is not served by an OpenAI-compatible provider")
            }
        },
    })
}

/// Sets what the user says a model can do. The answer is the record every reader now sees.
#[tauri::command]
pub async fn set_model_user_capabilities(
    state: State<'_, AppState>,
    args: SetUserCapabilitiesArgs,
) -> Result<ModelCapabilities> {
    let db = active_database(&state)?;
    state.gate().run(set_user_capabilities(&db, args)).await?
}
