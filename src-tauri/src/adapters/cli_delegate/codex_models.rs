//! The models a connected Codex account offers: `model/list` of `codex app-server`.
//!
//! The list is read from the program itself, with the account's stored `auth.json`, so it is what
//! the subscription offers today and no list in holzi falls out of date. The program is started
//! only for this question (initialize, `model/list`, end) in a run folder of the vault, contained
//! like every other run (spec 094).

use std::time::Duration;

use serde_json::{json, Value};
use tokio::io::{AsyncBufReadExt, BufReader};

use crate::adapters::types::AttachmentKind;
use crate::adapters::{AdapterError, ProviderModel};
use crate::cli_tools::runtime::DelegateRuntime;
use crate::model_capabilities::{ModelCapabilities, ReasoningControl, ReasoningOption};
use crate::vault_gate::ChildRegistry;

use super::codex::{build_command, build_request, read_line, write_message, Line};
use super::process::{configure_process_group, map_spawn_error, ChildLifecycle};
use super::DelegateVendor;

const CLIENT_NAME: &str = "holzi";
const CLIENT_VERSION: &str = env!("CARGO_PKG_VERSION");
/// The whole question, from start to answer: the program starts, the account is read, the list
/// comes back. A slow network must not hold a vault open or a refresh forever.
const LIST_TIMEOUT: Duration = Duration::from_secs(30);
/// The page size asked for, and the most pages read: a runaway cursor cannot loop for ever.
const PAGE_SIZE: u32 = 100;
const MAX_PAGES: usize = 10;

/// The id a chat started before the models were listed carries: the one placeholder model of
/// the vendor. It names no model, so Codex then runs its own default.
pub(super) const LEGACY_MODEL_ID: &str = "codex";

/// What one model of the answer says, as holzi's model row: the id Codex takes back at
/// `thread/start`, its name, the reasoning efforts it offers and whether it reads images. Hidden
/// models are the ones Codex itself keeps out of its picker. Pure and unit-testable.
pub(super) fn parse_models(result: &Value) -> Vec<ProviderModel> {
    let Some(items) = result.get("data").and_then(Value::as_array) else {
        return Vec::new();
    };
    items
        .iter()
        .filter(|item| item.get("hidden").and_then(Value::as_bool) != Some(true))
        .filter_map(parse_model)
        .collect()
}

fn parse_model(item: &Value) -> Option<ProviderModel> {
    let text = |key: &str| {
        item.get(key)
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
    };
    let remote_id = text("model").or_else(|| text("id"))?.to_string();
    let display_name = text("displayName").unwrap_or(&remote_id).to_string();

    let efforts: Vec<ReasoningOption> = item
        .get("supportedReasoningEfforts")
        .and_then(Value::as_array)
        .map(|options| {
            options
                .iter()
                .filter_map(|option| option.get("reasoningEffort").and_then(Value::as_str))
                .map(|effort| ReasoningOption {
                    id: effort.to_string(),
                    label: effort.to_string(),
                })
                .collect()
        })
        .unwrap_or_default();
    // A model that lists no effort says nothing about reasoning: not determined, not "none".
    let reasoning = if efforts.is_empty() {
        None
    } else {
        Some(ReasoningControl::presets(efforts))
    };

    let accepted_attachment_kinds = item
        .get("inputModalities")
        .and_then(Value::as_array)
        .and_then(|modalities| {
            let has_known_modality = modalities
                .iter()
                .any(|m| matches!(m.as_str(), Some("text" | "image")));
            has_known_modality.then(|| {
                let mut kinds = vec![AttachmentKind::Text];
                if modalities.iter().any(|m| m.as_str() == Some("image")) {
                    kinds.push(AttachmentKind::Image);
                }
                kinds
            })
        });

    Some(ProviderModel {
        remote_id,
        display_name,
        context_window: None,
        capabilities: ModelCapabilities {
            reasoning,
            accepted_attachment_kinds,
            ..ModelCapabilities::default()
        },
    })
}

/// Asks the installed program for the models of the account in `credentials` (an `auth.json`).
pub(super) async fn list_models(
    runtime: DelegateRuntime,
    credentials: Vec<u8>,
) -> Result<Vec<ProviderModel>, AdapterError> {
    if credentials.is_empty() {
        return Err(AdapterError::InvalidCredentials);
    }
    let run = runtime.run_folder().map_err(|error| AdapterError::Http {
        reason: format!("failed to create the run folder for codex: {error}"),
    })?;
    tokio::fs::write(run.path().join("auth.json"), &credentials)
        .await
        .map_err(|error| AdapterError::Http {
            reason: format!("failed to write codex auth.json: {error}"),
        })?;

    let mut command = build_command(&runtime, &run);
    configure_process_group(&mut command);
    // The program lives for this question only (`kill_on_drop` and the group kill below end it),
    // so it needs no place in the registry of the vault's children.
    let mut child = ChildLifecycle::spawn(&mut command, &ChildRegistry::default())
        .map_err(|error| map_spawn_error(DelegateVendor::Codex, error))?;
    // Keep draining stderr while the protocol runs. The app-server's stderr is piped by
    // `build_command`; leaving it unread can fill the pipe and block model/list before its timeout.
    let stderr_task = child
        .child_mut()
        .stderr
        .take()
        .map(|stderr| tokio::spawn(super::codex::read_limited(stderr)));
    let outcome = match (
        child.child_mut().stdin.take(),
        child.child_mut().stdout.take(),
    ) {
        (Some(stdin), Some(stdout)) => {
            tokio::time::timeout(LIST_TIMEOUT, ask(stdin, BufReader::new(stdout).lines()))
                .await
                .unwrap_or_else(|_| {
                    Err(AdapterError::Http {
                        reason: format!("codex model/list timed out after {LIST_TIMEOUT:?}"),
                    })
                })
        }
        _ => Err(AdapterError::Http {
            reason: "failed to capture the codex app-server pipes".into(),
        }),
    };
    child.terminate_and_reap().await;
    if let Some(stderr_task) = stderr_task {
        let _stderr_bytes = stderr_task.await.unwrap_or_default();
    }

    let models = outcome?;
    // A run that answered has proven the version it ran (spec 094 FR-022).
    runtime.confirm_run().await;
    Ok(models)
}

async fn ask(
    mut stdin: tokio::process::ChildStdin,
    mut lines: tokio::io::Lines<BufReader<tokio::process::ChildStdout>>,
) -> Result<Vec<ProviderModel>, AdapterError> {
    let mut next_id = 1_i64;
    call(
        &mut stdin,
        &mut lines,
        &mut next_id,
        "initialize",
        json!({"clientInfo": {"name": CLIENT_NAME, "version": CLIENT_VERSION}}),
    )
    .await?;

    let mut models = Vec::new();
    let mut cursor: Option<String> = None;
    for _ in 0..MAX_PAGES {
        let result = call(
            &mut stdin,
            &mut lines,
            &mut next_id,
            "model/list",
            json!({"limit": PAGE_SIZE, "cursor": cursor, "includeHidden": false}),
        )
        .await?;
        if !result.get("data").is_some_and(Value::is_array) {
            return Err(AdapterError::Http {
                reason: "codex model/list response is missing its data array".into(),
            });
        }
        models.extend(parse_models(&result));
        cursor = match result.get("nextCursor") {
            None | Some(Value::Null) => None,
            Some(Value::String(cursor)) if !cursor.is_empty() => Some(cursor.clone()),
            Some(Value::String(_)) => None,
            Some(_) => {
                return Err(AdapterError::Http {
                    reason: "codex model/list response has an invalid nextCursor".into(),
                });
            }
        };
        if cursor.is_none() {
            return Ok(models);
        }
    }
    Err(AdapterError::Http {
        reason: format!("codex model/list exceeded the {MAX_PAGES}-page limit"),
    })
}

/// One request and its answer. Nothing else is expected from the program while it only lists
/// models; a request of its own (an approval) is not answered, because none can come.
async fn call(
    stdin: &mut tokio::process::ChildStdin,
    lines: &mut tokio::io::Lines<BufReader<tokio::process::ChildStdout>>,
    next_id: &mut i64,
    method: &str,
    params: Value,
) -> Result<Value, AdapterError> {
    let id = *next_id;
    *next_id += 1;
    write_message(stdin, &build_request(id, method, params))
        .await
        .map_err(|error| AdapterError::Http {
            reason: format!("failed to write {method} request: {error}"),
        })?;
    loop {
        match read_line(lines).await.map_err(|error| AdapterError::Http {
            reason: format!("failed to read the answer to {method}: {error}"),
        })? {
            Line::Response {
                id: response_id,
                value,
            } if response_id == id => {
                if let Some(error) = value.get("error") {
                    return Err(AdapterError::Http {
                        reason: format!("codex {method} failed: {error}"),
                    });
                }
                return Ok(value.get("result").cloned().unwrap_or(Value::Null));
            }
            Line::Eof => {
                return Err(AdapterError::Http {
                    reason: format!("codex app-server exited before answering {method}"),
                });
            }
            _ => {}
        }
    }
}

#[cfg(test)]
#[path = "codex_models_tests.rs"]
mod tests;
