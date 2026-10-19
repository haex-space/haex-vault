//! Finding out in the background whether a local model can call tools (spec 032 FR-018a/b,
//! scenario 7). A model nobody knows anything about is asked once, after it loaded: first the
//! chat template (does it take tools at all?), then the five example sentences marked for the
//! self-test. The answer goes into the model's record, and from then on the chat treats the model
//! as it is. The chat is never blocked: a message the person sends, another model, or the end of
//! the vault ends the check, and a model asked too little is asked again at its next load.

use std::sync::Arc;

use tauri::{AppHandle, Emitter, Manager};
use tokio_util::sync::CancellationToken;

use crate::adapters::{ProviderAdapter, ToolTemplateProbe};
use crate::chat::eval::runner::{run_eval, EvalError, Which};
use crate::chat::eval::{embedded_set, embedded_tools};
use crate::chat::events::EVENT_MODEL_TOOL_USE_UPDATED;
use crate::model_capabilities::{ToolSupport, ToolUse, ToolUseBasis};
use crate::state::AppState;
use crate::storage::models as models_store;

/// The share of the self-test sentences a model has to get right to count as able to call tools.
///
/// ponytail: set from one measured model (spec 032 T046, research R17/R18). Qwen3-4B passes all 5
/// sentences with the tool instruction (1.0; 0.8 without it), a model that only talks passes the 2
/// smalltalk ones (0.4), so 0.6 separates the two.
/// Ceiling: no weaker model was measured (Qwen3 0.6B and 1.7B were not installed), so a model near
/// the line may flip between supported and unsupported across a re-install. Upgrade path: measure
/// the smaller catalog models and move the line to where they fall.
pub const SELF_TEST_PASS: f64 = 0.6;

/// What a self-test score means.
pub fn verdict(rate: f64) -> ToolSupport {
    if rate >= SELF_TEST_PASS {
        ToolSupport::Supported
    } else {
        ToolSupport::Unsupported
    }
}

/// Asks the model what it can do with tools: the template probe first, and only when that says
/// nothing, the self-test. Fails only when cancelled or when the model cannot be asked.
pub async fn find_tool_use(
    adapter: &dyn ProviderAdapter,
    cancel: &CancellationToken,
) -> Result<ToolUse, EvalError> {
    if adapter.probe_tool_template().await == ToolTemplateProbe::IgnoresTools {
        return Ok(ToolUse::new(
            ToolSupport::Unsupported,
            ToolUseBasis::Template,
        ));
    }
    let report = run_eval(
        adapter,
        "",
        true,
        &embedded_set(),
        &embedded_tools(),
        &[],
        Which::SelfTest,
        cancel,
    )
    .await?;
    Ok(ToolUse::new(
        verdict(report.total.rate),
        ToolUseBasis::SelfTest,
    ))
}

/// Starts the check of a freshly loaded local model in the background, as work of the vault
/// session, so closing the vault ends it. Does nothing for a model whose record already says
/// what it can do. `cancel` is the token `ChatState::begin_tool_check` handed out.
pub fn spawn_tool_use_check(
    app: &AppHandle,
    model_id: String,
    adapter: Arc<dyn ProviderAdapter>,
    cancel: CancellationToken,
) {
    let app = app.clone();
    let work = {
        let app = app.clone();
        async move {
            let Ok(db) = app.state::<AppState>().database() else {
                return;
            };
            let id = model_id.clone();
            let known = db
                .read(move |r| Ok(models_store::get_model(r, &id)?.and_then(|m| m.capabilities)))
                .await
                .ok()
                .flatten()
                .is_some_and(|capabilities| capabilities.tool_use.is_some());
            drop(db);
            if known {
                return;
            }
            let found = match find_tool_use(adapter.as_ref(), &cancel).await {
                Ok(found) => found,
                Err(EvalError::Cancelled) => return,
                Err(error) => {
                    log::warn!("tool-use check of {model_id} did not finish: {error}");
                    return;
                }
            };
            // The model may have changed while the last sentence ran.
            if cancel.is_cancelled() {
                return;
            }
            let Ok(db) = app.state::<AppState>().database() else {
                return;
            };
            let id = model_id.clone();
            match db
                .write(move |tx| models_store::set_tool_use(tx, &id, found).map(|_| ()))
                .await
            {
                Ok(()) => {
                    let _ = app.emit(
                        EVENT_MODEL_TOOL_USE_UPDATED,
                        serde_json::json!({ "modelId": model_id }),
                    );
                }
                Err(error) => log::warn!("tool-use result of {model_id} not stored: {error}"),
            }
        }
    };
    if let Err(error) = app.state::<AppState>().gate().spawn(work) {
        log::warn!("tool-use check not started: {error}");
    }
}

#[cfg(test)]
#[path = "selftest_tests.rs"]
mod selftest_tests;
