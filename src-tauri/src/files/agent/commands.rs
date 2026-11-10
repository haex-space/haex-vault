//! Tauri commands of the file actions of agents (contracts/tauri-commands.md): the answer to the
//! question for a storage, and the check behind `files.show`, which opens a window and so runs in
//! the frontend (ADR 0011).

use std::sync::Arc;

use serde::{Deserialize, Serialize};
use tauri::State;
use tokio_util::sync::CancellationToken;
use ts_rs::TS;

use super::env::{open_vault, vault_error, TauriEnv};
use super::exec::FilesAgent;
use super::prompt::{FilesAgentAnswer, FilesAgentChoice, PermissionPrompt};
use crate::files::encrypted::cache::heads_of_folder;

use crate::files::{FilesError, SourceRef};
use crate::state::AppState;

/// The executor of the built-in agent, managed as a state.
pub type BuiltinFilesAgent = Arc<FilesAgent<TauriEnv<tauri::Wry>>>;

/// The answer to `files-agent-permission-request`. A late or unknown id is not an error.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FilesAgentAnswerArgs {
    pub request_id: String,
    pub choice: FilesAgentChoice,
    /// "Erlaubnis merken" of a folder's question (spec 048 FR-031).
    #[serde(default)]
    pub remember: bool,
    /// "auch für Cloud-Modelle" of a folder's question (FR-030a).
    #[serde(default)]
    pub also_cloud: bool,
}

#[tauri::command]
pub fn files_agent_permission_answer(
    prompt: State<'_, PermissionPrompt>,
    args: FilesAgentAnswerArgs,
) {
    prompt.answer(
        &args.request_id,
        FilesAgentAnswer {
            choice: args.choice,
            remember: args.remember,
            also_cloud: args.also_cloud,
        },
    );
}

/// The decrypted names of encrypted folders, by their `<storage id>/<folder id>` target, for the
/// ones this device has read the head of: how the profile editor lists a remembered answer for a
/// folder (spec 048 FR-033, spec 083 FR-033).
#[tauri::command]
pub async fn files_folder_names(
    state: State<'_, AppState>,
    targets: Vec<String>,
) -> Result<std::collections::HashMap<String, String>, FilesError> {
    open_vault(&state)?
        .read(move |q| {
            let mut names = std::collections::HashMap::new();
            for target in targets {
                let Some((storage, folder)) = target.rsplit_once('/') else {
                    continue;
                };
                if let Some(name) = heads_of_folder(q, storage, folder)?
                    .into_iter()
                    .find_map(|(_, name)| name)
                {
                    names.insert(target, name);
                }
            }
            Ok(names)
        })
        .await
        .map_err(vault_error)
}

/// Whether the agent may show the entry, and where the file browser opens.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/types/bindings/")]
pub struct FilesAgentShow {
    pub allowed: bool,
    /// Why not: the code and message the agent gets.
    pub reason: Option<String>,
    pub source: Option<SourceRef>,
    /// The folder the browser opens.
    pub folder: Option<String>,
    /// The file opened in the viewer, by name in `folder`.
    pub open: Option<String>,
}

/// The checks of `files.stat` for the built-in agent, without content (FR-032a). A refusal is an
/// answer, not an error: the handler passes the reason on to the agent.
#[tauri::command]
pub async fn files_agent_check(
    agent: State<'_, BuiltinFilesAgent>,
    source: String,
    path: String,
    agent_call: bool,
    model_id: Option<String>,
    local_model: Option<bool>,
    external_agent_id: Option<String>,
) -> Result<FilesAgentShow, FilesError> {
    let cancel = CancellationToken::new();
    let show = if let Some(agent_id) = external_agent_id {
        super::exec::FilesAgent::external(agent.env.clone(), &agent_id)
            .show_target(&source, &path, &cancel)
            .await
    } else if agent_call {
        let locality = if local_model.unwrap_or(false) {
            crate::files::access::Locality::Local
        } else {
            crate::files::access::Locality::Cloud
        };
        crate::chat::tools::agent_task::with_origin(model_id, locality, async {
            agent.show_target(&source, &path, &cancel).await
        })
        .await
    } else {
        agent.show_target(&source, &path, &cancel).await
    };
    Ok(match show {
        Ok((source, folder, open)) => FilesAgentShow {
            allowed: true,
            reason: None,
            source: Some(source),
            folder: Some(folder),
            open,
        },
        Err(error) => FilesAgentShow {
            allowed: false,
            reason: Some(format!("{}: {}", error.code, error.message)),
            source: None,
            folder: None,
            open: None,
        },
    })
}
