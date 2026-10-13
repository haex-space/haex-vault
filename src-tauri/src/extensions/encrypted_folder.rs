//! Encrypted folders for extensions (spec 048 US6, FR-035 to FR-039, contracts/bridge.md).
//!
//! An extension reaches a storage only with `remoteStorage` for it; without, every function here
//! answers 1002 and nobody is asked. With it, an encrypted folder still needs its own
//! `encryptedFolder` permission, which only the user gives when asked: in the chooser
//! ([`super::encrypted_folder_choose`]) or in the question a 1004 raises, which names the storage
//! and the decrypted folder name for the user and nothing for the extension. Paths are relative to
//! the encrypted folder and follow the key rules of 038 (FR-011). The work is done by
//! [`crate::files::encrypted`] (FR-041): the extension gets decrypted content, never object keys,
//! keys, sidecars or the provider's bytes.

use std::future::Future;
use std::sync::Arc;
use std::time::Duration;

use base64::Engine;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use tokio_util::sync::CancellationToken;

use super::bridge::blocking::block_on;
use super::bridge::dispatch::CallContext;
use super::error::{Asks, BridgeError, ExtensionErrorCode};
use super::permissions::store::candidates;
use super::permissions::{
    evaluate, Action, Decision, FolderRef, Permission, PermissionKind, PermissionRequest,
    RequestTarget,
};
use super::remote_storage::{self, provider, text};
use super::remote_storage_keys::valid;
use super::sql::exec::{limits_of, Limits};
use crate::files::encrypted::by_id::find;
use crate::files::encrypted::format::{decode_id, Id};
use crate::files::encrypted::route::Located;
use crate::files::encrypted::source::{EncryptedFile, WINDOW};
use crate::files::encrypted::tree::{facts, Item};
use crate::files::encrypted::{manage, ops, route, transfer, Scope};
use crate::files::storage_source::StorageFiles;
use crate::files::{EntryKind, FilesError, FilesErrorCode};

pub const MODULE: &str = module_path!();

const KIND: PermissionKind = PermissionKind::EncryptedFolder;

fn invalid(message: &str) -> BridgeError {
    BridgeError::new(ExtensionErrorCode::Validation, message)
}

fn limit(message: &str) -> BridgeError {
    BridgeError::new(ExtensionErrorCode::LimitExceeded, message)
}

fn unavailable() -> BridgeError {
    BridgeError::new(ExtensionErrorCode::Database, "database unavailable")
}

/// 1002 for `kind`, `action` and `target`, as the SDK reads a refusal.
pub(super) fn denied(kind: PermissionKind, action: &Action, target: &str) -> BridgeError {
    BridgeError::new(ExtensionErrorCode::PermissionDenied, "permission denied").with_details(
        json!({ "resourceType": kind.as_str(), "action": action.as_string(), "target": target }),
    )
}

/// The extension's error for a failure inside an encrypted folder; never the failure's text,
/// which could name an entry (FR-027).
pub(super) fn files_error(error: &FilesError) -> BridgeError {
    use FilesErrorCode as C;
    match error.code {
        C::Damaged => BridgeError::new(ExtensionErrorCode::Damaged, "the entry is damaged"),
        C::Credentials | C::NoAccess => provider("accessDenied"),
        // As the plain storage calls report them (`remote_storage.rs`).
        C::Unreachable | C::HostKey => provider("network"),
        C::TooLarge => limit("too large"),
        C::NoSpace => limit("quota exceeded"),
        C::Unsupported => unavailable(),
        C::Exists => invalid("the name is taken"),
        _ => invalid("invalid path"),
    }
}

/// Runs `work` within the extension's time limit (017): over it is 7000.
pub(super) fn within<T>(
    limits: &Limits,
    work: impl Future<Output = Result<T, FilesError>>,
) -> Result<T, BridgeError> {
    let time = Duration::from_millis(limits.timeout_ms);
    // The timer needs the runtime, so it starts inside it.
    match block_on(async { tokio::time::timeout(time, work).await }) {
        Ok(Ok(value)) => Ok(value),
        Ok(Err(error)) => Err(files_error(&error)),
        Err(_) => Err(limit("time limit exceeded")),
    }
}

pub(super) fn limits(ctx: &CallContext) -> Result<Limits, BridgeError> {
    let extension_id = ctx.session.extension_id;
    ctx.db
        .read_blocking(move |q| limits_of(q, extension_id))
        .map_err(|_| unavailable())
}

/// `remoteStorage` for `storage_id` covers `action`, or 1002 without a question (FR-035, FR-036).
pub(super) fn storage_gate(
    ctx: &CallContext,
    storage_id: &str,
    action: &Action,
) -> Result<(), BridgeError> {
    let request = PermissionRequest {
        kind: PermissionKind::RemoteStorage,
        action: action.clone(),
        target: RequestTarget::StorageId(storage_id.to_owned()),
    };
    match evaluate(&remote_storage::grants(ctx)?, &request, ctx.device) {
        Decision::Allow => Ok(()),
        Decision::Prompt | Decision::Deny => {
            Err(denied(PermissionKind::RemoteStorage, action, storage_id))
        }
    }
}

/// The storage with the scope that goes through encrypted folders, sharing the opened folders of
/// this session with holzi's window.
pub(super) fn files(ctx: &CallContext, storage_id: &str) -> Result<StorageFiles, BridgeError> {
    let service = remote_storage::service(ctx);
    let access = remote_storage::access(ctx, &service, storage_id)?;
    let db = ctx.db.clone();
    Ok(
        StorageFiles::new(ctx.host.storage.store(), access).with_scope(Scope {
            storage_id: storage_id.to_owned(),
            folders: ctx.host.encrypted.folders(),
            vault: Arc::new(move || Ok(db.clone())),
            // The window's loads clean up; an extension's call leaves nothing running.
            background: Arc::new(|_| {}),
        }),
    )
}

/// The storage's name, for the user only.
pub(super) fn storage_name(ctx: &CallContext, storage_id: &str) -> Result<String, BridgeError> {
    let id = storage_id.to_owned();
    ctx.db
        .read_blocking(move |q| {
            crate::remote_storage::store::storage(q, &id)
                .map_err(|e| haex_crdt::Error::consumer(e.to_string()))
        })
        .map_err(|_| unavailable())?
        .map(|row| row.name)
        .ok_or_else(remote_storage::not_found)
}

/// One call on a folder: the storage, the folder and the names below it.
struct Call {
    storage_id: String,
    folder: FolderRef,
    fid: Id,
    rest: Vec<String>,
}

impl Call {
    /// Reads `storageId`, `folderId` and `path`; `root_ok` lets `path` be empty (the folder).
    fn parse(params: &Value, root_ok: bool) -> Result<Self, BridgeError> {
        let storage_id = text(params, "storageId")?;
        let folder_id = text(params, "folderId")?;
        let path = text(params, "path")?;
        let fid = decode_id(folder_id).ok_or_else(|| invalid("invalid folderId"))?;
        let folder = FolderRef::parse(&format!("{storage_id}/{folder_id}"))
            .ok_or_else(|| invalid("invalid storageId or folderId"))?;
        if path.is_empty() && root_ok {
            return Ok(Self {
                storage_id: storage_id.to_owned(),
                folder,
                fid,
                rest: Vec::new(),
            });
        }
        if !valid(path, root_ok) {
            return Err(invalid("invalid path"));
        }
        let rest = path
            .trim_end_matches('/')
            .split('/')
            .map(str::to_owned)
            .collect();
        Ok(Self {
            storage_id: storage_id.to_owned(),
            folder,
            fid,
            rest,
        })
    }
}

/// The `encryptedFolder` permissions of the calling extension: remembered and held.
fn grants(ctx: &CallContext) -> Result<Vec<Permission>, BridgeError> {
    let (extension_id, device) = (ctx.session.extension_id, ctx.device);
    let mut grants = ctx
        .db
        .read_blocking(move |q| candidates(q, extension_id, KIND, device).map_err(Into::into))
        .map_err(|_| unavailable())?;
    grants.extend(ctx.host.permissions.temporary(extension_id, KIND));
    Ok(grants)
}

/// The folder of `call` when the extension may `action` in it. Without an answer the user is
/// asked (1004), naming the folder only to them; a refusal, or a folder that is not there for an
/// extension without a grant, is 1002, so it learns nothing else (FR-039).
fn reach(
    ctx: &CallContext,
    call: &Call,
    action: Action,
    files: &StorageFiles,
    limits: &Limits,
) -> Result<Located, BridgeError> {
    let target = call.folder.target();
    let request = PermissionRequest {
        kind: KIND,
        action: action.clone(),
        target: RequestTarget::EncryptedFolder(call.folder.clone()),
    };
    let decision = evaluate(&grants(ctx)?, &request, ctx.device);
    if decision == Decision::Deny {
        return Err(denied(KIND, &action, &target));
    }
    let found = within(limits, find(files, &call.fid));
    match (decision, found) {
        (Decision::Allow, Ok(at)) => Ok(Located {
            rest: call.rest.clone(),
            ..at
        }),
        (Decision::Allow, Err(error)) => Err(error),
        (Decision::Prompt, Ok(at)) => Err(BridgeError::new(
            ExtensionErrorCode::PermissionPromptRequired,
            "permission required",
        )
        .with_details(json!({
            "resourceType": KIND.as_str(),
            "action": action.as_string(),
            "target": target,
        }))
        .asking(Asks::Folder {
            storage_name: storage_name(ctx, &call.storage_id)?,
            folder_name: at.head.display.clone(),
        })),
        (_, _) => Err(denied(KIND, &action, &target)),
    }
}

/// Everything up to the folder for `action`: checks, limits, storage and folder.
fn prepare(
    ctx: &CallContext,
    params: &Value,
    root_ok: bool,
    action: Action,
) -> Result<(Limits, StorageFiles, Located), BridgeError> {
    let call = Call::parse(params, root_ok)?;
    storage_gate(ctx, &call.storage_id, &action)?;
    let limits = limits(ctx)?;
    let files = files(ctx, &call.storage_id)?;
    let at = reach(ctx, &call, action, &files, &limits)?;
    Ok((limits, files, at))
}

/// `{request: {storageId, folderId, path}}` → `{entries: [{name, kind, size?, modified?}]}` of the
/// folder at `path` (`""`: the encrypted folder). A sidecar that does not open has no name, so it
/// is left out.
pub fn list(ctx: &CallContext, params: &Value) -> Result<Value, BridgeError> {
    let (limits, files, at) = prepare(ctx, params, true, Action::Read)?;
    let max = usize::try_from(limits.max_rows).unwrap_or(usize::MAX);
    let entries = within(&limits, async {
        let folder = route::open(&files, &at).await?;
        let view = route::view(&files, &folder).await?;
        let index = view
            .tree
            .find(&at.rest)
            .ok_or_else(|| FilesError::new(FilesErrorCode::NotFound, "not found"))?;
        if !view.tree.is_dir(index) {
            return Err(FilesError::invalid_path("not a folder"));
        }
        Ok(view
            .tree
            .children(index)
            .iter()
            .filter(|&&child| !matches!(view.tree.node(child).item, Item::Broken { .. }))
            .map(|&child| view.tree.entry(child, String::new()))
            .collect::<Vec<_>>())
    })?;
    if entries.len() > max {
        return Err(limit("too many entries"));
    }
    let entries: Vec<Value> = entries
        .into_iter()
        .map(|entry| {
            let mut item = json!({
                "name": entry.name,
                "kind": if entry.kind == EntryKind::Dir { "folder" } else { "file" },
            });
            if let Some(size) = entry.size {
                item["size"] = json!(size);
            }
            if let Some(modified) = entry.modified_ms {
                item["modified"] = json!(modified);
            }
            item
        })
        .collect();
    Ok(json!({ "entries": entries }))
}

/// `{request: {storageId, folderId, path}}` → the decrypted file as base64, checked whole against
/// its SHA-256; a damaged one is 3010.
pub fn download(ctx: &CallContext, params: &Value) -> Result<Value, BridgeError> {
    let (limits, files, at) = prepare(ctx, params, false, Action::Read)?;
    // The largest file whose base64 form fits into `max_response_bytes` (as 038).
    let max = limits.max_response_bytes / 4 * 3;
    let bytes = within(&limits, async {
        let (folder, version) = ops::file_at(&files, &at).await?;
        let expected = facts(&version)
            .map(|f| f.sha256)
            .ok_or_else(crate::files::encrypted::damaged)?;
        let file = EncryptedFile::open(&files, folder, &version).await?;
        if file.size() > max {
            return Err(FilesError::new(FilesErrorCode::TooLarge, "too large"));
        }
        let mut bytes = Vec::with_capacity(usize::try_from(file.size()).unwrap_or(0));
        let mut at = 0;
        while at < file.size() {
            let end = file.size().min(at + WINDOW);
            bytes.extend(file.read(at, end).await?);
            at = end;
        }
        if <[u8; 32]>::from(Sha256::digest(&bytes)) != expected {
            return Err(crate::files::encrypted::damaged());
        }
        Ok(bytes)
    })?;
    Ok(Value::String(
        base64::engine::general_purpose::STANDARD.encode(bytes),
    ))
}

/// `{request: {storageId, folderId, path, data}}`: writes the file (base64 `data`); missing folders
/// are made, a file there gets a new version.
pub fn upload(ctx: &CallContext, params: &Value) -> Result<Value, BridgeError> {
    let data = text(params, "data")?;
    let call = Call::parse(params, false)?;
    storage_gate(ctx, &call.storage_id, &Action::ReadWrite)?;
    let limits = limits(ctx)?;
    if data.len() as u64 > limits.max_response_bytes {
        return Err(limit("data too large"));
    }
    let body = base64::engine::general_purpose::STANDARD
        .decode(data)
        .map_err(|_| invalid("data must be base64"))?;
    let files = files(ctx, &call.storage_id)?;
    let at = reach(ctx, &call, Action::ReadWrite, &files, &limits)?;
    within(&limits, transfer::put_file(&files, &at, &body))?;
    Ok(Value::Null)
}

/// `{request: {storageId, folderId, path}}`: deletes the file or folder at `path` with everything
/// below it; the encrypted folder itself stays.
pub fn delete(ctx: &CallContext, params: &Value) -> Result<Value, BridgeError> {
    let (limits, files, at) = prepare(ctx, params, false, Action::ReadWrite)?;
    within(
        &limits,
        manage::delete(&files, &at, &CancellationToken::new(), &mut |_| {}),
    )?;
    Ok(Value::Null)
}

pub use super::encrypted_folder_choose::choose;

#[cfg(test)]
#[path = "encrypted_folder_tests.rs"]
mod tests;
