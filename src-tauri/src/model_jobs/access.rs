//! The way from an extension to its grant (spec 055 US9, R17, `contracts/bridge.md`): the user
//! chooses models and effort in holzi, never in the extension. The extension may open holzi's
//! page of its own models, and ask for a grant before its first job: holzi asks as it would for
//! that job, without a destination and without sending anything.

use serde::Deserialize;
use serde_json::{json, Value};

use super::bridge::entry;
use super::candidates::fits;
use super::effort::Level;
use super::errors::refusal;
use super::job::{JobError, Need};
use super::preflight::{ask_for_model, read_facts};
use crate::extensions::bridge::dispatch::CallContext;
use crate::extensions::error::BridgeError;

/// Tells holzi's window to show the model settings of an extension.
pub(crate) const MODEL_SETTINGS_EVENT: &str = "extension-open-model-settings";

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawRequest {
    needs: Vec<String>,
    #[serde(default)]
    effort: Option<String>,
}

fn parse(params: &Value) -> Result<(Vec<Need>, Option<Level>), JobError> {
    let raw = RawRequest::deserialize(params)
        .map_err(|error| JobError::Invalid(format!("the request: {error}")))?;
    let mut needs = Vec::new();
    for text in &raw.needs {
        let need =
            Need::parse(text).ok_or_else(|| JobError::Invalid(format!("unknown need {text:?}")))?;
        if needs.contains(&need) {
            return Err(JobError::Invalid(format!("`needs` names {text:?} twice")));
        }
        needs.push(need);
    }
    let effort = match raw.effort.as_deref() {
        None => None,
        Some(text) => Some(
            Level::parse(text)
                .ok_or_else(|| JobError::Invalid(format!("unknown effort {text:?}")))?,
        ),
    };
    Ok((needs, effort))
}

/// `{needs, effort?}`: the granted models that fit, first choice first, or the question for a
/// grant (1004), `no_capable_model` (8001) or `consent_denied` (1002) as for a step.
pub(crate) fn request(ctx: &CallContext, params: &Value) -> Result<Value, BridgeError> {
    let (needs, effort) = parse(params).map_err(|error| refusal(&error))?;
    let facts = read_facts(ctx)?;
    let models: Vec<Value> = facts
        .granted
        .iter()
        .filter(|c| fits(&needs, c.model.capabilities.as_ref()))
        .map(entry)
        .collect();
    if models.is_empty() {
        return Err(ask_for_model(&needs, effort, &facts, ctx.device));
    }
    Ok(json!({ "models": models }))
}

/// `{}`: shows the model settings of the calling extension; which one is never the caller's to say.
pub(crate) fn open_settings(ctx: &CallContext, _params: &Value) -> Result<Value, BridgeError> {
    ctx.emitter.emit(
        MODEL_SETTINGS_EVENT,
        json!({ "extensionId": ctx.session.extension_id.to_string() }),
    );
    Ok(json!({}))
}
