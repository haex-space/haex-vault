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
use crate::error::HolziError;
use crate::files::encrypted::cache::heads_of_folder;
use crate::files::permissions::{
    self, AgentFileKind, AgentFilePermission, AgentFileReach, AgentFileStatus,
};
use crate::files::{FilesError, FilesErrorCode, SourceRef};
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

/// A stored permission as the settings show it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/types/bindings/")]
pub struct AgentFilePermissionView {
    #[serde(flatten)]
    #[ts(flatten)]
    pub permission: AgentFilePermission,
    /// The decrypted name of an encrypted folder, when this device has read its head.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub folder_name: Option<String>,
}

/// The stored file permissions of every agent, for the settings (044 FR-031b, 048 FR-033).
#[tauri::command]
pub async fn files_agent_permissions(
    state: State<'_, AppState>,
) -> Result<Vec<AgentFilePermissionView>, FilesError> {
    open_vault(&state)?
        .read(|q| {
            let mut views = Vec::new();
            for permission in permissions::list(q)? {
                let folder_name = match (permission.kind, permission.target.rsplit_once('/')) {
                    (AgentFileKind::EncryptedFolder, Some((storage, folder))) => {
                        heads_of_folder(q, storage, folder)?
                            .into_iter()
                            .find_map(|(_, name)| name)
                    }
                    _ => None,
                };
                views.push(AgentFilePermissionView {
                    permission,
                    folder_name,
                });
            }
            Ok(views)
        })
        .await
        .map_err(vault_error)
}

/// Sets one file permission of an agent from the settings (FR-031b): a change or a revoke.
#[tauri::command]
pub async fn files_agent_permission_set(
    state: State<'_, AppState>,
    agent_id: String,
    kind: AgentFileKind,
    target: String,
    status: AgentFileStatus,
    reach: Option<AgentFileReach>,
) -> Result<(), FilesError> {
    let row = AgentFilePermission {
        agent_id,
        kind,
        target,
        status,
        updated_at: crate::passwords::clock::unix_millis(std::time::SystemTime::now()),
        reach,
    };
    open_vault(&state)?
        .write(move |tx| Ok(permissions::set(tx, &row)?))
        .await
        .map_err(|error| match error {
            HolziError::InvalidInput { reason } => {
                FilesError::new(FilesErrorCode::Unsupported, reason)
            }
            other => vault_error(other),
        })
}

/// Removes one file permission of an agent from the settings: its default applies again, and an
/// encrypted folder asks on the next access (spec 048 FR-033).
#[tauri::command]
pub async fn files_agent_permission_remove(
    state: State<'_, AppState>,
    agent_id: String,
    kind: AgentFileKind,
    target: String,
) -> Result<(), FilesError> {
    open_vault(&state)?
        .write(move |tx| Ok(permissions::remove(tx, &agent_id, kind, &target)?))
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
) -> Result<FilesAgentShow, FilesError> {
    Ok(
        match agent
            .show_target(&source, &path, &CancellationToken::new())
            .await
        {
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
        },
    )
}
