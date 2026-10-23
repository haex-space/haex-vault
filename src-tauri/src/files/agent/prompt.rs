//! The question to the user when an agent reaches for a storage it has no permission for (spec 044
//! FR-031a, research R14), for an encrypted folder in it (spec 048 FR-030a, FR-031), or for the
//! contacts (spec 051 FR-030, research R15), parked on `prompt_park.rs`: Rust emits
//! `files-agent-permission-request` and waits for `files_agent_permission_answer`. Nobody to ask (no window), no answer in 60 s or a cancelled turn
//! refuse this one call; the caller stores only real answers.

use std::sync::Arc;
use std::time::Duration;

use serde::{Deserialize, Serialize};
use tokio_util::sync::CancellationToken;
use ts_rs::TS;

use crate::prompt_park::PromptPark;

/// Event the question goes out on (contracts/tauri-commands.md).
pub const EVENT_PERMISSION_REQUEST: &str = "files-agent-permission-request";

/// How long the question waits for the user.
pub const PROMPT_TIMEOUT: Duration = Duration::from_secs(60);

/// What the agent wants to do with the storage.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/types/bindings/")]
pub enum FilesAgentWant {
    Read,
    ReadWrite,
}

/// The user's answer.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/types/bindings/")]
pub enum FilesAgentChoice {
    Read,
    ReadWrite,
    Deny,
}

/// The user's answer. A storage's answer is always kept; a folder's only with `remember`, and for
/// cloud models only with `also_cloud` too.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FilesAgentAnswer {
    pub choice: FilesAgentChoice,
    pub remember: bool,
    pub also_cloud: bool,
}

impl FilesAgentAnswer {
    /// `choice` without the checkboxes.
    pub fn plain(choice: FilesAgentChoice) -> Self {
        Self {
            choice,
            remember: false,
            also_cloud: false,
        }
    }
}

/// What the question names for an encrypted folder (FR-030a); only the user sees it, never the
/// agent.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/types/bindings/")]
pub struct FilesAgentFolderQuestion {
    /// The decrypted name.
    pub name: String,
    /// The model the read contents go to.
    pub model: String,
    /// Whether it runs in the cloud rather than on this device.
    pub cloud: bool,
}

/// What a question is about.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/types/bindings/")]
pub enum FilesAgentQuestionKind {
    /// A storage, or an encrypted folder in it.
    Storage,
    /// The address book; `storage_id` and `storage_name` are empty.
    Contacts,
}

/// The payload of [`EVENT_PERMISSION_REQUEST`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/types/bindings/")]
pub struct FilesAgentPermissionRequest {
    pub request_id: String,
    pub agent_id: String,
    pub kind: FilesAgentQuestionKind,
    pub storage_id: String,
    pub storage_name: String,
    pub wants: FilesAgentWant,
    /// Set when the question is for an encrypted folder of the storage.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub folder: Option<FilesAgentFolderQuestion>,
}

/// Delivers a question to the window; `false` when no window could hear it.
pub type PromptEmitter = Arc<dyn Fn(&FilesAgentPermissionRequest) -> bool + Send + Sync>;

/// Shared by the executors and the answer command; a Tauri managed state. Cheap to clone.
#[derive(Clone)]
pub struct PermissionPrompt {
    park: PromptPark<FilesAgentPermissionRequest, FilesAgentAnswer>,
    /// One question at a time: two calls for one storage or folder must not ask twice.
    asking: Arc<tokio::sync::Mutex<()>>,
}

impl Default for PermissionPrompt {
    fn default() -> Self {
        Self::with_timeout(PROMPT_TIMEOUT)
    }
}

impl PermissionPrompt {
    pub fn with_timeout(timeout: Duration) -> Self {
        Self {
            park: PromptPark::new(timeout),
            asking: Arc::new(tokio::sync::Mutex::new(())),
        }
    }

    /// Set from `setup()`, where the `AppHandle` exists. The question goes out with the id it is
    /// parked under as its `request_id`.
    pub fn set_emitter(&self, emitter: PromptEmitter) {
        self.park.set_emitter(Arc::new(
            move |id: &str, request: &FilesAgentPermissionRequest| {
                emitter(&FilesAgentPermissionRequest {
                    request_id: id.to_owned(),
                    ..request.clone()
                })
            },
        ));
    }

    /// Holds the turn of the asking caller; it reads the grants again before it asks, since the
    /// caller before it may have asked the same.
    pub async fn turn(&self) -> tokio::sync::MutexGuard<'_, ()> {
        self.asking.lock().await
    }

    /// Asks for a storage or a folder in it and waits; `None` when nobody answered.
    pub async fn ask(
        &self,
        agent_id: &str,
        storage_id: &str,
        storage_name: &str,
        wants: FilesAgentWant,
        folder: Option<FilesAgentFolderQuestion>,
        cancel: &CancellationToken,
    ) -> Option<FilesAgentAnswer> {
        self.question(
            FilesAgentPermissionRequest {
                request_id: String::new(),
                agent_id: agent_id.to_owned(),
                kind: FilesAgentQuestionKind::Storage,
                storage_id: storage_id.to_owned(),
                storage_name: storage_name.to_owned(),
                wants,
                folder,
            },
            cancel,
        )
        .await
    }

    /// Asks for the contacts and waits; `None` when nobody answered. The answer is kept only with
    /// `remember` ("Immer erlauben") or as a denial.
    pub async fn ask_contacts(
        &self,
        agent_id: &str,
        wants: FilesAgentWant,
        cancel: &CancellationToken,
    ) -> Option<FilesAgentAnswer> {
        self.question(
            FilesAgentPermissionRequest {
                request_id: String::new(),
                agent_id: agent_id.to_owned(),
                kind: FilesAgentQuestionKind::Contacts,
                storage_id: String::new(),
                storage_name: String::new(),
                wants,
                folder: None,
            },
            cancel,
        )
        .await
    }

    /// No window, no answer in time and a cancelled turn all end as `None`.
    async fn question(
        &self,
        request: FilesAgentPermissionRequest,
        cancel: &CancellationToken,
    ) -> Option<FilesAgentAnswer> {
        self.park.ask(&request, cancel, |_| {}).await.ok()
    }

    /// The window's answer; `false` for an unknown or late id.
    pub fn answer(&self, request_id: &str, answer: FilesAgentAnswer) -> bool {
        self.park.answer(request_id, answer)
    }
}

#[cfg(test)]
#[path = "prompt_tests.rs"]
mod tests;
