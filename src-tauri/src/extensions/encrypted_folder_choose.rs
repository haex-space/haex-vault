//! `extension_encrypted_folder_choose` (spec 048 FR-036, contracts/bridge.md, research R12): an
//! extension knows only its own area of a storage and cannot find an encrypted folder. It asks holzi
//! to choose one; holzi's window shows the readable encrypted folders of the storage by name and,
//! in the same view, the question (Lesen erlauben / Lesen und Schreiben erlauben / Ablehnen,
//! "Erlaubnis merken" off by default). The call waits like a confirmation dialog; the extension
//! learns the folder's id and name only after the user allowed it.

use std::collections::HashMap;
use std::sync::mpsc::{self, Sender};
use std::sync::{Arc, Mutex, MutexGuard, OnceLock, PoisonError};
use std::time::Duration;

use serde::Deserialize;
use serde_json::{json, Value};
use ts_rs::TS;

use super::bridge::dispatch::CallContext;
use super::encrypted_folder::{denied, files, limits, storage_gate, storage_name, within};
use super::error::{BridgeError, ExtensionErrorCode};
use super::permissions::prompts::Question;
use super::permissions::{Action, FolderRef, PermissionKind, PermissionStatus};
use super::protocol::token;
use super::remote_storage::text;
use crate::files::encrypted::by_id::readable_folders;
use crate::files::encrypted::format::encode_id;
use crate::files::encrypted::EncryptedFolders;
use crate::storage::query::Query;

/// A chooser nobody answers ends as a cancel, like a confirmation dialog.
pub const CHOOSE_WAIT: Duration = Duration::from_secs(300);

/// Event to holzi's window: show the chooser.
pub const CHOOSE_REQUEST: &str = "extension-encrypted-folder-choose";
/// Event to holzi's window: the chooser ended (answered, timed out or its frame closed).
pub const CHOOSE_ENDED: &str = "extension-encrypted-folder-choose-ended";

/// The user's answer in the chooser (`extension_encrypted_folder_choose_resolve`).
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, TS)]
#[serde(tag = "kind", rename_all = "camelCase")]
#[ts(export, export_to = "../../src/types/bindings/")]
pub enum ChooseAnswer {
    /// Closed without an answer: nothing is decided.
    Cancel,
    #[serde(rename_all = "camelCase")]
    Decide {
        folder_id: String,
        decision: ChooseDecision,
        /// "Erlaubnis merken"; off by default.
        remember: bool,
    },
}

/// Lesen erlauben / Lesen und Schreiben erlauben / Ablehnen.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/types/bindings/")]
pub enum ChooseDecision {
    Read,
    ReadWrite,
    Deny,
}

type Choosers = HashMap<String, (String, Sender<ChooseAnswer>)>;

/// The opened encrypted folders extensions reach and the open choosers.
#[derive(Default)]
pub struct EncryptedFolderState {
    /// The window's opened folders (`FilesState::encrypted`), set once at start; tests and a host
    /// without it use their own.
    folders: OnceLock<Arc<EncryptedFolders>>,
    choosers: Mutex<Choosers>,
}

impl EncryptedFolderState {
    /// Shares the window's opened folders; only the first call counts.
    pub fn set_folders(&self, folders: Arc<EncryptedFolders>) {
        let _ = self.folders.set(folders);
    }

    pub fn folders(&self) -> Arc<EncryptedFolders> {
        Arc::clone(self.folders.get_or_init(Arc::default))
    }

    fn choosers(&self) -> MutexGuard<'_, Choosers> {
        self.choosers.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// Registers a chooser of `frame`; `false` while that frame has one open.
    fn open(&self, request_id: &str, frame: &str, answer: Sender<ChooseAnswer>) -> bool {
        let mut choosers = self.choosers();
        if choosers.values().any(|(f, _)| f == frame) {
            return false;
        }
        choosers.insert(request_id.to_owned(), (frame.to_owned(), answer));
        true
    }

    /// Answers a chooser; an unknown or ended one is ignored.
    pub fn resolve(&self, request_id: &str, answer: ChooseAnswer) {
        if let Some((_, sender)) = self.choosers().remove(request_id) {
            // The waiting call may have given up already.
            let _ = sender.send(answer);
        }
    }

    /// Ends the choosers of a closed frame: their calls are cancelled.
    pub fn drop_of(&self, frame: &str) {
        self.choosers().retain(|_, (f, _)| f != frame);
    }

    /// Whether a chooser with this id waits (tests).
    pub fn is_open(&self, request_id: &str) -> bool {
        self.choosers().contains_key(request_id)
    }
}

/// The name holzi shows for the calling extension.
fn display_name(ctx: &CallContext) -> String {
    let ext = ctx.session.extension_id.to_string();
    ctx.db
        .read_blocking(move |q| {
            q.query_row(
                "SELECT coalesce(display_name, name) FROM extensions WHERE id = ?1 \
                 UNION ALL SELECT coalesce(display_name, name) FROM dev_extensions_no_sync \
                 WHERE id = ?1",
                &[&ext],
                |r| r.get::<_, String>(0),
            )
        })
        .ok()
        .flatten()
        .unwrap_or_default()
}

/// `{request: {storageId, access: read | readWrite}}` → `{folderId, name}`, `null` when the user
/// closed the chooser, 1002 when they refused or the extension has no `remoteStorage` for the
/// storage (then nobody is asked).
pub fn choose(ctx: &CallContext, params: &Value) -> Result<Value, BridgeError> {
    let storage_id = text(params, "storageId")?;
    let (access, wanted) = match text(params, "access")? {
        "read" => (Action::Read, "read"),
        "readWrite" => (Action::ReadWrite, "readWrite"),
        _ => {
            return Err(BridgeError::new(
                ExtensionErrorCode::Validation,
                "access must be read or readWrite",
            ))
        }
    };
    storage_gate(ctx, storage_id, &access)?;
    let limits = limits(ctx)?;
    let files = files(ctx, storage_id)?;
    let offered: Vec<(String, String)> = within(&limits, readable_folders(&files))?
        .iter()
        .filter_map(|at| Some((encode_id(&at.head.fid?), at.head.display.clone())))
        .collect();
    let (extension_name, storage) = (display_name(ctx), storage_name(ctx, storage_id)?);
    let request_id = token::mint();
    let frame = &ctx.session.frame;
    let (sender, waiting) = mpsc::channel();
    if !ctx.host.encrypted.open(&request_id, frame, sender) {
        return Err(BridgeError::new(
            ExtensionErrorCode::LimitExceeded,
            "a chooser of this frame is already open",
        ));
    }
    ctx.emitter.emit(
        CHOOSE_REQUEST,
        json!({
            "requestId": request_id,
            "frame": frame,
            "extensionName": extension_name,
            "storageName": storage,
            "access": wanted,
            "folders": offered
                .iter()
                .map(|(folder_id, name)| json!({ "folderId": folder_id, "name": name }))
                .collect::<Vec<_>>(),
        }),
    );
    let answer = waiting
        .recv_timeout(CHOOSE_WAIT)
        .unwrap_or(ChooseAnswer::Cancel);
    ctx.host
        .encrypted
        .resolve(&request_id, ChooseAnswer::Cancel);
    ctx.emitter
        .emit(CHOOSE_ENDED, json!({ "requestId": request_id }));
    let ChooseAnswer::Decide {
        folder_id,
        decision,
        remember,
    } = answer
    else {
        return Ok(Value::Null);
    };
    let Some((_, name)) = offered.iter().find(|(id, _)| *id == folder_id) else {
        return Ok(Value::Null);
    };
    let target = FolderRef {
        storage_id: storage_id.to_owned(),
        folder_id: folder_id.clone(),
    }
    .target();
    // A refusal covers reading and writing; an allowance is what the user chose.
    let (status, action) = match decision {
        ChooseDecision::Read => (PermissionStatus::Granted, Action::Read),
        ChooseDecision::ReadWrite => (PermissionStatus::Granted, Action::ReadWrite),
        ChooseDecision::Deny => (PermissionStatus::Denied, Action::ReadWrite),
    };
    let question = Question {
        extension_id: ctx.session.extension_id,
        kind: PermissionKind::EncryptedFolder,
        action: action.as_string(),
        target: target.clone(),
    };
    let now = crate::passwords::clock::unix_millis(std::time::SystemTime::now());
    super::commands::permissions::record(
        &ctx.db, &ctx.host, ctx.device, &question, status, remember, now,
    )
    .map_err(|_| BridgeError::new(ExtensionErrorCode::Database, "database unavailable"))?;
    if status == PermissionStatus::Denied {
        return Err(denied(PermissionKind::EncryptedFolder, &access, &target));
    }
    Ok(json!({ "folderId": folder_id, "name": name }))
}
