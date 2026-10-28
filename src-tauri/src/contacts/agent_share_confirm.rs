//! The confirmation before an agent shares a contact (spec 060 FR-029 to FR-032, research R8): the
//! agent's proposal goes to the window as `contact-share-confirm-request`, shown in the share
//! dialog, and waits for `contact_share_confirm_answer`. Every time, without "remember" and
//! whatever the approval mode (except "Plan", which refuses the action before it runs); no window,
//! no answer in [`CONFIRM_TIMEOUT`], a "no" or a cancelled turn refuse it and give the agent nothing.

use std::sync::Arc;
use std::time::Duration;

use serde::Serialize;
use tauri::State;
use tokio_util::sync::CancellationToken;
use ts_rs::TS;

use super::share::model::{ContactShareOptions, ContactShareSelection};
use crate::prompt_park::PromptPark;

/// Event the confirmation goes out on (contracts/tauri-commands.md).
pub const EVENT_CONTACT_SHARE_CONFIRM: &str = "contact-share-confirm-request";

/// How long the confirmation waits for the user.
pub const CONFIRM_TIMEOUT: Duration = Duration::from_secs(120);

/// The payload of [`EVENT_CONTACT_SHARE_CONFIRM`]: which contact the agent wants to share, and
/// what of it. `options` is what the share dialog offers; `proposed` is the agent's proposal inside
/// it, which the user may change.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/types/bindings/")]
pub struct ContactShareConfirmRequest {
    pub request_id: String,
    pub agent_id: String,
    pub contact_id: String,
    pub contact_name: String,
    pub proposed: ContactShareSelection,
    pub options: ContactShareOptions,
}

/// Delivers a confirmation to the window; `false` when no window could hear it.
pub type ConfirmEmitter = Arc<dyn Fn(&ContactShareConfirmRequest) -> bool + Send + Sync>;

/// Shared by the executor and the answer command; a Tauri managed state. Cheap to clone.
#[derive(Clone)]
pub struct ContactShareConfirm {
    park: PromptPark<ContactShareConfirmRequest, Option<ContactShareSelection>>,
}

impl Default for ContactShareConfirm {
    fn default() -> Self {
        Self::with_timeout(CONFIRM_TIMEOUT)
    }
}

impl ContactShareConfirm {
    pub fn with_timeout(timeout: Duration) -> Self {
        Self {
            park: PromptPark::new(timeout),
        }
    }

    /// Set from `setup()`; the request goes out with the id it is parked under.
    pub fn set_emitter(&self, emitter: ConfirmEmitter) {
        self.park.set_emitter(Arc::new(
            move |id: &str, request: &ContactShareConfirmRequest| {
                emitter(&ContactShareConfirmRequest {
                    request_id: id.to_owned(),
                    ..request.clone()
                })
            },
        ));
    }

    /// Asks and waits. The selection the user left standing, or `None` for a "no", no window, a
    /// timeout or a cancelled turn.
    pub async fn ask(
        &self,
        request: &ContactShareConfirmRequest,
        cancel: &CancellationToken,
    ) -> Option<ContactShareSelection> {
        self.park.ask(request, cancel, |_| {}).await.ok().flatten()
    }

    /// The window's answer; `false` for an unknown or late id.
    pub fn answer(&self, request_id: &str, selection: Option<ContactShareSelection>) -> bool {
        self.park.answer(request_id, selection)
    }
}

/// The answer to `contact-share-confirm-request`: the selection to release, or none to decline. A
/// late or unknown id is not an error.
#[tauri::command]
pub fn contact_share_confirm_answer(
    confirm: State<'_, ContactShareConfirm>,
    request_id: String,
    selection: Option<ContactShareSelection>,
) {
    confirm.answer(&request_id, selection);
}
