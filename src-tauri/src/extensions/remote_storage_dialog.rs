//! What the host keeps for the remote storage of extensions (spec 038, research R6): the provider
//! and resolver (S3 and the system's resolver in holzi, fakes in tests) and the storage dialogs that
//! wait for the user.
//!
//! A management call of an extension (add, update, test, remove) waits like
//! `extension_dialog_confirm`: holzi sends `extension-storage-request` to its window, which shows a
//! confirmation over the tab (never with credential fields) and, when new credentials are needed,
//! holzi's own window over the whole app. The answer comes back through `storage_dialog_resolve`;
//! credentials in it travel from holzi's window to Rust only and never reach the extension.
//! Closing the frame or [`DIALOG_WAIT`] counts as cancel.
//!
//! holzi tests typed credentials while its window stays open: a failed test goes back to that
//! window ([`StorageTrial::Failed`]), which lets the user correct them or cancel, and the call waits
//! for the next answer. The extension learns only the end: the storage, or 1002.

use std::collections::HashMap;
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Arc, Mutex, MutexGuard, OnceLock, PoisonError};
use std::time::Duration;

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use tokio::sync::oneshot;
use ts_rs::TS;

use super::bridge::dispatch::CallContext;
use super::error::{BridgeError, ExtensionErrorCode};
use super::protocol::token;
use crate::remote_storage::address::{Resolver, SystemResolver};
use crate::remote_storage::model::CredentialsInput;
use crate::remote_storage::{RemoteStore, TestOutcome};

/// A storage dialog nobody answers ends as cancel, like a confirmation dialog.
pub const DIALOG_WAIT: Duration = Duration::from_secs(300);

/// The user's answer to a storage dialog (contracts/tauri-commands.md `storage_dialog_resolve`).
#[derive(Debug, Clone, Deserialize, TS)]
#[ts(export, export_to = "../../src/types/bindings/")]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum StorageAnswer {
    Cancel,
    #[serde(rename_all = "camelCase")]
    Confirm {
        /// An existing connection the user chose for the new storage.
        #[ts(optional)]
        #[serde(default)]
        connection_id: Option<String>,
        /// New credentials; only holzi's window over the whole app sends them.
        #[ts(optional)]
        #[serde(default)]
        credentials: Option<CredentialsInput>,
        #[ts(optional)]
        #[serde(default)]
        name: Option<String>,
        #[ts(optional)]
        #[serde(default)]
        bucket: Option<String>,
        /// The user's word to make a missing folder of a WebDAV or SFTP storage (spec 050 FR-006).
        #[ts(as = "Option<bool>", optional)]
        #[serde(default)]
        create_folder: bool,
    },
}

impl StorageAnswer {
    fn has_credentials(&self) -> bool {
        matches!(
            self,
            Self::Confirm {
                credentials: Some(_),
                ..
            }
        )
    }
}

/// What holzi's window learns of the credentials it sent (`storage_dialog_resolve`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(export, export_to = "../../src/types/bindings/")]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum StorageTrial {
    /// The request ended; its window closes.
    Ended,
    /// The test of the credentials failed; the window shows why and waits for another answer.
    Failed {
        outcome: TestOutcome,
        /// The test object holzi could not delete, if the failed probe left one behind.
        #[serde(rename = "leftoverKey")]
        leftover_key: Option<String>,
    },
}

/// An answer and, for credentials, where the result of their test goes.
struct Answered {
    answer: StorageAnswer,
    trial: Option<oneshot::Sender<StorageTrial>>,
}

type Dialogs = HashMap<String, (String, Sender<Answered>)>;

/// The provider, the resolver and the open storage dialogs of this process.
#[derive(Default)]
pub struct StorageState {
    store: OnceLock<Arc<dyn RemoteStore>>,
    resolver: OnceLock<Arc<dyn Resolver>>,
    /// Open dialogs by request id: the frame that asked and where the answer goes.
    dialogs: Mutex<Dialogs>,
}

impl StorageState {
    /// Sets provider and resolver; only the first call counts (tests set fakes).
    pub fn set(&self, store: Arc<dyn RemoteStore>, resolver: Arc<dyn Resolver>) {
        let _ = self.store.set(store);
        let _ = self.resolver.set(resolver);
    }

    /// The providers of every protocol unless a test set another.
    pub fn store(&self) -> Arc<dyn RemoteStore> {
        Arc::clone(
            self.store
                .get_or_init(crate::remote_storage::commands::stores),
        )
    }

    /// The resolver: the system's unless a test set another.
    pub fn resolver(&self) -> Arc<dyn Resolver> {
        Arc::clone(self.resolver.get_or_init(|| Arc::new(SystemResolver)))
    }

    fn dialogs(&self) -> MutexGuard<'_, Dialogs> {
        self.dialogs.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// Registers a dialog of `frame`; `false` while that frame already has one open.
    fn open(&self, request_id: &str, frame: &str, answer: Sender<Answered>) -> bool {
        let mut dialogs = self.dialogs();
        if dialogs.values().any(|(f, _)| f == frame) {
            return false;
        }
        dialogs.insert(request_id.to_owned(), (frame.to_owned(), answer));
        true
    }

    /// Answers a dialog; an unknown or ended one is ignored. For credentials, the receiver of
    /// their test's result: [`StorageTrial::Ended`] once the request ends.
    pub fn resolve(
        &self,
        request_id: &str,
        answer: StorageAnswer,
    ) -> Option<oneshot::Receiver<StorageTrial>> {
        let dialogs = self.dialogs();
        let (_, sender) = dialogs.get(request_id)?;
        let (trial, result) = match answer.has_credentials() {
            true => {
                let (trial, result) = oneshot::channel();
                (Some(trial), Some(result))
            }
            false => (None, None),
        };
        // The waiting call may have given up already.
        sender.send(Answered { answer, trial }).ok()?;
        result
    }

    /// Ends a dialog: later answers reach nobody.
    fn end(&self, request_id: &str) {
        self.dialogs().remove(request_id);
    }

    /// Ends the dialogs of a closed frame: their calls are cancelled.
    pub fn drop_of(&self, frame: &str) {
        self.dialogs().retain(|_, (f, _)| f != frame);
    }

    /// Whether a dialog with this id waits (tests).
    pub fn is_open(&self, request_id: &str) -> bool {
        self.dialogs().contains_key(request_id)
    }
}

/// What a dialog is about.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DialogKind {
    Add,
    Update,
    Test,
    Remove,
}

impl DialogKind {
    fn as_str(self) -> &'static str {
        match self {
            Self::Add => "add",
            Self::Update => "update",
            Self::Test => "test",
            Self::Remove => "remove",
        }
    }
}

fn cancelled() -> BridgeError {
    BridgeError::new(
        ExtensionErrorCode::PermissionDenied,
        "cancelled by the user",
    )
}

/// A dialog over the calling frame, open until dropped: then holzi's windows for it close.
pub struct Dialog<'a> {
    ctx: &'a CallContext,
    request_id: String,
    waiting: Receiver<Answered>,
    /// Where the result of the credentials of the last answer goes.
    trial: Option<oneshot::Sender<StorageTrial>>,
}

impl Dialog<'_> {
    /// The next answer; cancel, the frame's end or [`DIALOG_WAIT`] is 1002.
    pub fn answer(&mut self) -> Result<StorageAnswer, BridgeError> {
        self.end_trial(StorageTrial::Ended);
        match self.waiting.recv_timeout(DIALOG_WAIT) {
            Ok(Answered {
                answer: answer @ StorageAnswer::Confirm { .. },
                trial,
            }) => {
                self.trial = trial;
                Ok(answer)
            }
            Ok(Answered {
                answer: StorageAnswer::Cancel,
                ..
            })
            | Err(_) => Err(cancelled()),
        }
    }

    /// The test of the credentials of the last answer failed: holzi's window shows `outcome` and
    /// the dialog waits for the next answer.
    pub fn failed(&mut self, outcome: TestOutcome, leftover_key: Option<String>) {
        self.end_trial(StorageTrial::Failed {
            outcome,
            leftover_key,
        });
    }

    fn end_trial(&mut self, trial: StorageTrial) {
        if let Some(sender) = self.trial.take() {
            // holzi's window may be closed already.
            let _ = sender.send(trial);
        }
    }
}

impl Drop for Dialog<'_> {
    fn drop(&mut self) {
        self.ctx.host.storage.end(&self.request_id);
        self.end_trial(StorageTrial::Ended);
        self.ctx.emitter.emit(
            "extension-storage-request-ended",
            json!({ "requestId": self.request_id }),
        );
    }
}

/// Opens a dialog over the calling frame: `proposal` and `other_extensions` are shown, never
/// credentials. One dialog per frame at a time (7000).
pub fn open<'a>(
    ctx: &'a CallContext,
    kind: DialogKind,
    extension_name: &str,
    proposal: Value,
    other_extensions: &[String],
) -> Result<Dialog<'a>, BridgeError> {
    let request_id = token::mint();
    let frame = &ctx.session.frame;
    let (answer, waiting) = mpsc::channel();
    if !ctx.host.storage.open(&request_id, frame, answer) {
        return Err(BridgeError::new(
            ExtensionErrorCode::LimitExceeded,
            "a storage dialog of this frame is already open",
        ));
    }
    ctx.emitter.emit(
        "extension-storage-request",
        json!({
            "requestId": request_id,
            "frame": frame,
            "kind": kind.as_str(),
            "proposal": proposal,
            "extensionName": extension_name,
            "otherExtensions": other_extensions,
        }),
    );
    Ok(Dialog {
        ctx,
        request_id,
        waiting,
        trial: None,
    })
}

/// Asks once and ends the dialog with the answer; cancel, the frame's end or [`DIALOG_WAIT`] is
/// 1002.
pub fn ask(
    ctx: &CallContext,
    kind: DialogKind,
    extension_name: &str,
    proposal: Value,
    other_extensions: &[String],
) -> Result<StorageAnswer, BridgeError> {
    open(ctx, kind, extension_name, proposal, other_extensions)?.answer()
}
