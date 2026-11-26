//! The models a connected Abacus account offers: the `get_model_catalog` control request of
//! `abacusai` in its JSON-lines mode (spec 096 plan D7).
//!
//! The list is read from the program itself, with the account's device code from the password
//! manager, so it is what the account offers today. Measured (research §14): the request spends no
//! credits. The program is started only for this question (the request, its answer, end) in a run
//! folder of the vault, contained like every other run (spec 094); no `user` line is written, so no
//! model runs.

use std::time::Duration;

use serde_json::{json, Value};
use tokio::io::{AsyncBufReadExt, BufReader};

use crate::adapters::{AdapterError, ProviderModel};
use crate::cli_tools::runtime::DelegateRuntime;
use crate::model_capabilities::ModelCapabilities;
use crate::vault_gate::{ChildRegistry, VaultDb};

use super::abacus::{
    approval_settings, build_command, fatal_reason, read_credential, read_stderr_tail,
    startup_error, write_line, APPROVAL_SETTINGS_FILE, DEVICE_CODE_ENV,
};
use super::process::{configure_process_group, map_spawn_error, ChildLifecycle};
use super::DelegateVendor;

/// The whole question, from start to answer. A slow network must not hold a refresh forever.
const LIST_TIMEOUT: Duration = Duration::from_secs(30);

/// The id of the one control request this run sends.
const REQUEST_ID: &str = "haex-model-catalog";

/// The origin of the account's own models. The catalog can also name models of local providers
/// on the host, which a run contained to the vault does not reach.
const ACCOUNT_ORIGIN: &str = "abacus";

/// The `control_request` that asks for the catalog.
pub(super) fn catalog_request() -> Value {
    json!({
        "type": "control_request",
        "request_id": REQUEST_ID,
        "request": { "subtype": "get_model_catalog" },
    })
}

/// What a line of the program's output is for this question.
#[derive(Debug, PartialEq)]
pub(super) enum CatalogLine {
    /// The answer to [`catalog_request`]: the account's models, in the program's order.
    Models(Vec<ProviderModel>),
    /// The answer, but not a usable list.
    Refused(String),
    /// Any other line.
    Other,
}

/// Reads one line of the program's output. Pure and unit-testable.
pub(super) fn parse_catalog_line(line: &str) -> CatalogLine {
    let Ok(message) = serde_json::from_str::<Value>(line) else {
        return CatalogLine::Other;
    };
    let response = message.get("response");
    if message.get("type").and_then(Value::as_str) != Some("control_response")
        || response
            .and_then(|r| r.get("request_id"))
            .and_then(Value::as_str)
            != Some(REQUEST_ID)
    {
        return CatalogLine::Other;
    }
    if response
        .and_then(|r| r.get("subtype"))
        .and_then(Value::as_str)
        != Some("success")
    {
        let error = response
            .and_then(|r| r.get("error"))
            .and_then(Value::as_str)
            .unwrap_or("no reason given");
        return CatalogLine::Refused(format!("abacusai get_model_catalog failed: {error}"));
    }
    let Some(groups) = response
        .and_then(|r| r.get("response"))
        .and_then(|r| r.get("groups"))
        .and_then(Value::as_array)
    else {
        return CatalogLine::Refused(
            "abacusai get_model_catalog answered without a list of groups".into(),
        );
    };
    let models: Vec<ProviderModel> = groups
        .iter()
        .filter_map(|group| group.get("models").and_then(Value::as_array))
        .flatten()
        .filter(|model| model.get("origin").and_then(Value::as_str) == Some(ACCOUNT_ORIGIN))
        .filter_map(parse_model)
        .collect();
    if models.is_empty() {
        return CatalogLine::Refused("abacusai get_model_catalog listed no model".into());
    }
    CatalogLine::Models(models)
}

fn parse_model(model: &Value) -> Option<ProviderModel> {
    let text = |key: &str| {
        model
            .get(key)
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
    };
    let remote_id = text("id")?.to_owned();
    let display_name = text("name").unwrap_or(&remote_id).to_owned();
    Some(ProviderModel {
        remote_id,
        display_name,
        context_window: None,
        capabilities: ModelCapabilities::default(),
    })
}

/// Asks the installed program for the models of the account whose device code is in `db`.
pub(super) async fn list_models(
    runtime: DelegateRuntime,
    db: Option<&VaultDb>,
    children: ChildRegistry,
) -> Result<Vec<ProviderModel>, AdapterError> {
    let credential = read_credential(db).await?;
    let run = runtime.run_folder().map_err(|error| AdapterError::Http {
        reason: format!("failed to create the run folder for abacusai: {error}"),
    })?;
    // The same start as a request, so the program is in the same posture; no model runs here.
    let settings = run.path().join(APPROVAL_SETTINGS_FILE);
    tokio::fs::write(&settings, approval_settings().to_string())
        .await
        .map_err(|error| AdapterError::Http {
            reason: format!("failed to write the approval settings for abacusai: {error}"),
        })?;
    let mut command = build_command(&runtime, "", None, Some(&settings), &run);
    command.env(DEVICE_CODE_ENV, credential.secret());
    drop(credential);
    configure_process_group(&mut command);
    // The program lives for this question only (`kill_on_drop` and the group kill below end it),
    // so it needs no place in the registry of the vault's children.
    let mut child = ChildLifecycle::spawn(&mut command, &children)
        .map_err(|error| map_spawn_error(DelegateVendor::Abacus, error))?;
    let stderr_task = child
        .child_mut()
        .stderr
        .take()
        .map(|stderr| tokio::spawn(read_stderr_tail(stderr)));
    let mut initial_write_error = None;
    let outcome = match (
        child.child_mut().stdin.take(),
        child.child_mut().stdout.take(),
    ) {
        (Some(mut stdin), Some(stdout)) => {
            let ask = async {
                if let Err(error) = write_line(&mut stdin, &catalog_request()).await {
                    initial_write_error = Some(error.to_string());
                    return Err(AdapterError::Http {
                        reason: format!("failed to write the abacusai catalog request: {error}"),
                    });
                }
                let mut lines = BufReader::new(stdout).lines();
                loop {
                    match lines.next_line().await {
                        Ok(Some(line)) => match parse_catalog_line(&line) {
                            CatalogLine::Models(models) => return Ok(Some(models)),
                            CatalogLine::Refused(reason) => {
                                return Err(AdapterError::Http { reason })
                            }
                            CatalogLine::Other => {}
                        },
                        // The program ended before it answered: its stderr says why.
                        Ok(None) => return Ok(None),
                        Err(error) => {
                            return Err(AdapterError::Http {
                                reason: format!("failed to read abacusai output: {error}"),
                            })
                        }
                    }
                }
            };
            tokio::time::timeout(LIST_TIMEOUT, ask)
                .await
                .unwrap_or_else(|_| {
                    Err(AdapterError::Http {
                        reason: format!(
                            "abacusai get_model_catalog timed out after {LIST_TIMEOUT:?}"
                        ),
                    })
                })
        }
        _ => Err(AdapterError::Http {
            reason: "failed to capture the abacusai pipes".into(),
        }),
    };
    child.terminate_and_reap().await;
    let stderr = match stderr_task {
        Some(task) => task.await.unwrap_or_default(),
        None => Vec::new(),
    };
    if let Some(error) = initial_write_error {
        return Err(initial_request_write_error(&stderr, &error));
    }

    let models = match outcome? {
        Some(models) => models,
        None => return Err(startup_error(&stderr)),
    };
    // A run that answered has proven the version it ran (spec 094 FR-022).
    runtime.confirm_run().await;
    Ok(models)
}

fn initial_request_write_error(stderr: &[u8], error: &str) -> AdapterError {
    if fatal_reason(&String::from_utf8_lossy(stderr)).is_some() {
        startup_error(stderr)
    } else {
        AdapterError::Http {
            reason: format!("failed to write the abacusai catalog request: {error}"),
        }
    }
}

#[cfg(test)]
#[path = "abacus_models_tests.rs"]
mod tests;
