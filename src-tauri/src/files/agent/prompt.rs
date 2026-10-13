//! The question to the user when an agent reaches for a storage it has no permission for (spec 044
//! FR-031a, research R14), or for an encrypted folder in it (spec 048 FR-030a, FR-031), after the
//! pattern of `chat/tools/action_bridge.rs`: Rust emits
//! `files-agent-permission-request`, parks on a oneshot and waits for
//! `files_agent_permission_answer`. Nobody to ask (no window), no answer in 60 s or a cancelled turn
//! refuse this one call; the caller stores only real answers.

use std::collections::HashMap;
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::time::Duration;

use serde::{Deserialize, Serialize};
use tokio::sync::oneshot;
use tokio_util::sync::CancellationToken;
use ts_rs::TS;
use uuid::Uuid;

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

/// The payload of [`EVENT_PERMISSION_REQUEST`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/types/bindings/")]
pub struct FilesAgentPermissionRequest {
    pub request_id: String,
    pub agent_id: String,
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

struct Inner {
    emitter: Mutex<Option<PromptEmitter>>,
    pending: Mutex<HashMap<String, oneshot::Sender<FilesAgentAnswer>>>,
    /// One question at a time: two calls for one storage or folder must not ask twice.
    asking: tokio::sync::Mutex<()>,
    timeout: Duration,
}

/// Shared by the executors and the answer command; a Tauri managed state. Cheap to clone.
#[derive(Clone)]
pub struct PermissionPrompt {
    inner: Arc<Inner>,
}

impl Default for PermissionPrompt {
    fn default() -> Self {
        Self::with_timeout(PROMPT_TIMEOUT)
    }
}

impl PermissionPrompt {
    pub fn with_timeout(timeout: Duration) -> Self {
        Self {
            inner: Arc::new(Inner {
                emitter: Mutex::new(None),
                pending: Mutex::new(HashMap::new()),
                asking: tokio::sync::Mutex::new(()),
                timeout,
            }),
        }
    }

    /// Set from `setup()`, where the `AppHandle` exists.
    pub fn set_emitter(&self, emitter: PromptEmitter) {
        *self
            .inner
            .emitter
            .lock()
            .unwrap_or_else(PoisonError::into_inner) = Some(emitter);
    }

    /// Holds the turn of the asking caller; it reads the grants again before it asks, since the
    /// caller before it may have asked the same.
    pub async fn turn(&self) -> tokio::sync::MutexGuard<'_, ()> {
        self.inner.asking.lock().await
    }

    /// Asks and waits; `None` when nobody answered.
    pub async fn ask(
        &self,
        agent_id: &str,
        storage_id: &str,
        storage_name: &str,
        wants: FilesAgentWant,
        folder: Option<FilesAgentFolderQuestion>,
        cancel: &CancellationToken,
    ) -> Option<FilesAgentAnswer> {
        let emitter = self
            .inner
            .emitter
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone()?;
        let request = FilesAgentPermissionRequest {
            request_id: Uuid::new_v4().to_string(),
            agent_id: agent_id.to_owned(),
            storage_id: storage_id.to_owned(),
            storage_name: storage_name.to_owned(),
            wants,
            folder,
        };
        let (sender, receiver) = oneshot::channel();
        self.pending().insert(request.request_id.clone(), sender);
        let choice = if emitter(&request) {
            tokio::select! {
                biased;
                _ = cancel.cancelled() => None,
                answer = receiver => answer.ok(),
                _ = tokio::time::sleep(self.inner.timeout) => None,
            }
        } else {
            None
        };
        self.pending().remove(&request.request_id);
        choice
    }

    /// The window's answer; `false` for an unknown or late id.
    pub fn answer(&self, request_id: &str, answer: FilesAgentAnswer) -> bool {
        match self.pending().remove(request_id) {
            Some(sender) => sender.send(answer).is_ok(),
            None => false,
        }
    }

    fn pending(&self) -> MutexGuard<'_, HashMap<String, oneshot::Sender<FilesAgentAnswer>>> {
        self.inner
            .pending
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
    }
}

#[cfg(test)]
#[path = "prompt_tests.rs"]
mod tests;
