//! The confirmation before an agent gives files away (spec 056 FR-045, research R17): a new share, a
//! contact added or a level raised goes to the window as `file-share-confirm-request` and waits for
//! `file_share_confirm_answer`. Every time, without "remember" and whatever the approval mode; no
//! window, no answer in [`CONFIRM_TIMEOUT`] or a cancelled turn refuse it.

use std::sync::Arc;
use std::time::Duration;

use serde::Serialize;
use tauri::State;
use tokio_util::sync::CancellationToken;
use ts_rs::TS;

use crate::files::share::FileShareLevel;
use crate::prompt_park::PromptPark;

/// Event the confirmation goes out on (contracts/tauri-commands.md).
pub const EVENT_SHARE_CONFIRM: &str = "file-share-confirm-request";

/// How long the confirmation waits for the user.
pub const CONFIRM_TIMEOUT: Duration = Duration::from_secs(120);

/// A contact the share would reach, or reach further, with its level.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/types/bindings/")]
pub struct FileShareConfirmGrant {
    pub contact_name: String,
    pub level: FileShareLevel,
}

/// The payload of [`EVENT_SHARE_CONFIRM`]: what the agent wants to give whom.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/types/bindings/")]
pub struct FileShareConfirmRequest {
    pub request_id: String,
    pub agent_id: String,
    /// The share is new, not one that gets another contact or a higher level.
    pub new_share: bool,
    pub share_name: String,
    /// Window path of the shared folder or file.
    pub path: String,
    /// The device that serves it; `None` for this device.
    pub device_name: Option<String>,
    /// Only the contacts that are new or get a higher level.
    pub grants: Vec<FileShareConfirmGrant>,
}

/// Delivers a confirmation to the window; `false` when no window could hear it.
pub type ConfirmEmitter = Arc<dyn Fn(&FileShareConfirmRequest) -> bool + Send + Sync>;

/// Shared by the executor and the answer command; a Tauri managed state. Cheap to clone.
#[derive(Clone)]
pub struct ShareConfirm {
    park: PromptPark<FileShareConfirmRequest, bool>,
}

impl Default for ShareConfirm {
    fn default() -> Self {
        Self::with_timeout(CONFIRM_TIMEOUT)
    }
}

impl ShareConfirm {
    pub fn with_timeout(timeout: Duration) -> Self {
        Self {
            park: PromptPark::new(timeout),
        }
    }

    /// Set from `setup()`; the request goes out with the id it is parked under.
    pub fn set_emitter(&self, emitter: ConfirmEmitter) {
        self.park.set_emitter(Arc::new(
            move |id: &str, request: &FileShareConfirmRequest| {
                emitter(&FileShareConfirmRequest {
                    request_id: id.to_owned(),
                    ..request.clone()
                })
            },
        ));
    }

    /// Asks and waits; only an explicit "Freigeben" confirms.
    pub async fn ask(&self, request: &FileShareConfirmRequest, cancel: &CancellationToken) -> bool {
        self.park
            .ask(request, cancel, |_| {})
            .await
            .unwrap_or(false)
    }

    /// The window's answer; `false` for an unknown or late id.
    pub fn answer(&self, request_id: &str, confirmed: bool) -> bool {
        self.park.answer(request_id, confirmed)
    }
}

/// The answer to `file-share-confirm-request`. A late or unknown id is not an error.
#[tauri::command]
pub fn file_share_confirm_answer(
    confirm: State<'_, ShareConfirm>,
    request_id: String,
    confirmed: bool,
) {
    confirm.answer(&request_id, confirmed);
}
