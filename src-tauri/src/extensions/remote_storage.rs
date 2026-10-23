//! Remote storage of extensions (spec 038 US2, contracts/bridge.md, research R4–R7): the list of
//! storages by name only and the four object methods, each under a `remoteStorage` permission for
//! the storage and inside the extension's own area ([`super::remote_storage_keys`]). holzi speaks
//! S3, WebDAV and SFTP itself ([`crate::remote_storage`]); an extension gets the same answers on
//! each (spec 050 FR-017, [`super::remote_storage_tree`]). No answer and no error carries
//! credentials, the endpoint, the host or the region. Adding storages is in
//! [`super::remote_storage_propose`], the other management methods in
//! [`super::remote_storage_manage`].

use base64::Engine;
use serde_json::{json, Value};
use tokio::time::Instant;

use super::bridge::blocking::block_on;
use super::bridge::dispatch::CallContext;
use super::error::{BridgeError, ExtensionErrorCode};
use super::permissions::store::candidates;
use super::permissions::{
    evaluate, Action, Decision, Permission, PermissionKind, PermissionRequest, PermissionStatus,
    RequestTarget, Target,
};
use super::remote_storage_keys::Area;
use super::remote_storage_tree::{check_key, clashes, prune};
use super::sql::exec::{limits_of, Limits};
use crate::error::HolziError;
use crate::passwords::service::PasswordsService;
use crate::remote_storage::model::StorageOverview;
use crate::remote_storage::service::StorageService;
use crate::remote_storage::{Access, CredentialsState, Protocol, StorageError, TestOutcome};

pub const MODULE: &str = module_path!();

const KIND: PermissionKind = PermissionKind::RemoteStorage;

pub(super) fn invalid(message: &str) -> BridgeError {
    BridgeError::new(ExtensionErrorCode::Validation, message)
}

pub(super) fn not_found() -> BridgeError {
    BridgeError::new(ExtensionErrorCode::NotFound, "not found")
}

fn limit(message: &str) -> BridgeError {
    BridgeError::new(ExtensionErrorCode::LimitExceeded, message)
}

/// 2002 with the kind of the provider's refusal, never its text (research R7).
pub(super) fn provider(kind: &str) -> BridgeError {
    BridgeError::new(ExtensionErrorCode::Http, "storage provider error")
        .with_details(json!({ "kind": kind }))
}

fn unavailable() -> BridgeError {
    BridgeError::new(ExtensionErrorCode::Database, "database unavailable")
}

/// The storage service over the vault of the call, with the host's provider and resolver.
pub(super) fn service(ctx: &CallContext) -> StorageService {
    StorageService::new(
        ctx.db.clone(),
        PasswordsService::new(ctx.db.clone()),
        ctx.host.storage.store(),
        ctx.host.storage.resolver(),
    )
}

/// The `remoteStorage` permissions of the calling extension: remembered and held in memory.
pub(super) fn grants(ctx: &CallContext) -> Result<Vec<Permission>, BridgeError> {
    let (extension_id, device) = (ctx.session.extension_id, ctx.device);
    let mut grants = ctx
        .db
        .read_blocking(move |q| candidates(q, extension_id, KIND, device).map_err(Into::into))
        .map_err(|_| unavailable())?;
    grants.extend(ctx.host.permissions.temporary(extension_id, KIND));
    Ok(grants)
}

/// Allowed, or 1004 (state "ask") / 1002 with `{resourceType, action, target}`.
pub(super) fn check(
    ctx: &CallContext,
    grants: &[Permission],
    action: Action,
    target: RequestTarget,
    asked: &str,
) -> Result<(), BridgeError> {
    let request = PermissionRequest {
        kind: KIND,
        action: action.clone(),
        target,
    };
    let code = match evaluate(grants, &request, ctx.device) {
        Decision::Allow => return Ok(()),
        Decision::Prompt if Target::parse(KIND, asked).is_some() => {
            ExtensionErrorCode::PermissionPromptRequired
        }
        Decision::Prompt | Decision::Deny => ExtensionErrorCode::PermissionDenied,
    };
    Err(
        BridgeError::new(code, "permission required").with_details(json!({
            "resourceType": KIND.as_str(),
            "action": action.as_string(),
            "target": asked,
        })),
    )
}

/// The permission check for one storage. It comes before any lookup, so an extension without a
/// permission never learns whether the storage exists.
pub(super) fn check_storage(
    ctx: &CallContext,
    action: Action,
    storage_id: &str,
) -> Result<(), BridgeError> {
    let grants = grants(ctx)?;
    check(
        ctx,
        &grants,
        action,
        RequestTarget::StorageId(storage_id.to_owned()),
        storage_id,
    )
}

/// The text at `name` of `params.request` (or of `params` itself for test and remove).
pub(super) fn text<'a>(params: &'a Value, name: &str) -> Result<&'a str, BridgeError> {
    params
        .get("request")
        .unwrap_or(params)
        .get(name)
        .and_then(Value::as_str)
        .ok_or_else(|| invalid(&format!("{name} must be a string")))
}

/// What a call reaches: the storage's access, or the error the extension gets.
pub(super) fn access(
    ctx: &CallContext,
    service: &StorageService,
    storage_id: &str,
) -> Result<Access, BridgeError> {
    match block_on(service.access_of(storage_id)) {
        Ok(access) => Ok(access),
        Err(HolziError::StorageNotFound) => Err(not_found()),
        Err(HolziError::StorageCredentialsUnavailable { state }) => Err(match state {
            CredentialsState::Syncing => provider("network"),
            CredentialsState::Missing | CredentialsState::Present => {
                record(ctx, service, storage_id, TestOutcome::AccessDenied);
                provider("accessDenied")
            }
        }),
        Err(_) => Err(unavailable()),
    }
}

/// Remembers a refusal for the settings (data-model.md); a failure to remember changes nothing.
fn record(ctx: &CallContext, service: &StorageService, storage_id: &str, outcome: TestOutcome) {
    if let Err(error) = block_on(service.record(storage_id, outcome)) {
        log::warn!(
            "remote storage of extension {}: test result not kept: {error}",
            ctx.session.extension_id
        );
    }
}

/// The extension's error for a provider's error (research R7).
fn mapped(
    ctx: &CallContext,
    service: &StorageService,
    storage_id: &str,
    error: StorageError,
) -> BridgeError {
    match error {
        StorageError::NotFound => not_found(),
        StorageError::AccessDenied | StorageError::MissingRight => {
            let outcome = if error == StorageError::AccessDenied {
                TestOutcome::AccessDenied
            } else {
                TestOutcome::MissingRight
            };
            record(ctx, service, storage_id, outcome);
            provider("accessDenied")
        }
        // A host key nobody confirmed is the user's to settle in holzi (spec 050 contracts/bridge.md).
        StorageError::Network
        | StorageError::HostKeyChanged
        | StorageError::HostKeyUnknown
        | StorageError::Unsupported
        | StorageError::Protocol
        // Only another device's files report a conflict, and extensions never reach those.
        | StorageError::Conflict => provider("network"),
        StorageError::TooLarge => limit("answer too large"),
        StorageError::TimedOut => limit("time limit exceeded"),
        StorageError::QuotaExceeded => limit("quota exceeded"),
    }
}

fn limits(ctx: &CallContext) -> Result<Limits, BridgeError> {
    let extension_id = ctx.session.extension_id;
    ctx.db
        .read_blocking(move |q| limits_of(q, extension_id))
        .map_err(|_| unavailable())
}

fn deadline(limits: &Limits) -> Instant {
    Instant::now() + std::time::Duration::from_millis(limits.timeout_ms)
}

/// The largest object whose base64 form fits into `max_response_bytes` (as `web.rs`).
fn max_object(limits: &Limits) -> usize {
    usize::try_from(limits.max_response_bytes / 4 * 3).unwrap_or(usize::MAX)
}

/// The storages and their connections' provider names.
pub(super) fn overview(service: &StorageService) -> Result<StorageOverview, BridgeError> {
    block_on(service.overview()).map_err(|_| unavailable())
}

/// The name of a storage's place in a call: a bucket on S3, a folder on a server with real
/// folders (WebDAV, SFTP; a local folder of spec 048 is never offered to extensions).
pub(super) fn place_of(protocol: Protocol) -> &'static str {
    if protocol.is_tree() {
        "folder"
    } else {
        "bucket"
    }
}

/// One item of the list (FR-009a, spec 050 contracts/bridge.md): names only; a folder instead of
/// a bucket on WebDAV and SFTP.
pub(super) fn item(overview: &StorageOverview, storage_id: &str) -> Option<Value> {
    let storage = overview.storages.iter().find(|s| s.id == storage_id)?;
    let connection = overview
        .connections
        .iter()
        .find(|c| c.id == storage.connection_id)?;
    let place = place_of(connection.protocol);
    let mut item = json!({
        "id": storage.id,
        "type": connection.protocol.as_str(),
        "name": storage.name,
        "providerName": connection.provider_name,
    });
    item[place] = json!(storage.bucket);
    Some(item)
}

/// `{}` → the storages a read permission covers, by name (FR-009a). Without any read permission
/// at all the extension is asked for one (1004) or refused (1002).
pub fn list_backends(ctx: &CallContext, _params: &Value) -> Result<Value, BridgeError> {
    let grants = grants(ctx)?;
    let service = service(ctx);
    let overview = overview(&service)?;
    let readable: Vec<Value> = overview
        .storages
        .iter()
        .filter(|storage| {
            check(
                ctx,
                &grants,
                Action::Read,
                RequestTarget::StorageId(storage.id.clone()),
                &storage.id,
            )
            .is_ok()
        })
        .filter_map(|storage| item(&overview, &storage.id))
        .collect();
    let may_read = grants.iter().any(|grant| {
        grant.status == PermissionStatus::Granted && grant.action.covers(&Action::Read)
    });
    if readable.is_empty() && !may_read {
        check(
            ctx,
            &grants,
            Action::Read,
            RequestTarget::StorageId("*".into()),
            "*",
        )?;
    }
    Ok(Value::Array(readable))
}

/// `{request: {backendId, key, data}}`: stores the object under the extension's key.
pub fn upload(ctx: &CallContext, params: &Value) -> Result<Value, BridgeError> {
    let storage_id = text(params, "backendId")?;
    check_storage(ctx, Action::ReadWrite, storage_id)?;
    let data = text(params, "data")?;
    let limits = limits(ctx)?;
    if data.len() as u64 > limits.max_response_bytes {
        return Err(limit("data too large"));
    }
    let body = base64::engine::general_purpose::STANDARD
        .decode(data)
        .map_err(|_| invalid("data must be base64"))?;
    let own = text(params, "key")?;
    let key = Area::of(ctx)?.key(own)?;
    let service = service(ctx);
    let access = access(ctx, &service, storage_id)?;
    check_key(&access, own, "key")?;
    let store = ctx.host.storage.store();
    let deadline = deadline(&limits);
    if access.location.is_tree()
        && block_on(clashes(store.as_ref(), &access, &key, deadline))
            .map_err(|e| mapped(ctx, &service, storage_id, e))?
    {
        return Err(invalid("invalid key"));
    }
    block_on(store.put(&access, &key, body, deadline))
        .map_err(|e| mapped(ctx, &service, storage_id, e))?;
    Ok(Value::Null)
}

/// `{request: {backendId, key}}` → the object as base64.
pub fn download(ctx: &CallContext, params: &Value) -> Result<Value, BridgeError> {
    let storage_id = text(params, "backendId")?;
    check_storage(ctx, Action::Read, storage_id)?;
    let own = text(params, "key")?;
    let key = Area::of(ctx)?.key(own)?;
    let limits = limits(ctx)?;
    let service = service(ctx);
    let access = access(ctx, &service, storage_id)?;
    check_key(&access, own, "key")?;
    let store = ctx.host.storage.store();
    let body = block_on(store.get(&access, &key, max_object(&limits), deadline(&limits)))
        .map_err(|e| mapped(ctx, &service, storage_id, e))?;
    Ok(Value::String(
        base64::engine::general_purpose::STANDARD.encode(body),
    ))
}

/// `{request: {backendId, prefix?}}` → `[{key, size, lastModified}]`, keys without the area.
pub fn list(ctx: &CallContext, params: &Value) -> Result<Value, BridgeError> {
    let storage_id = text(params, "backendId")?;
    check_storage(ctx, Action::Read, storage_id)?;
    let prefix = match params.get("request").unwrap_or(params).get("prefix") {
        None | Some(Value::Null) => "",
        Some(Value::String(prefix)) => prefix.as_str(),
        Some(_) => return Err(invalid("prefix must be a string")),
    };
    let area = Area::of(ctx)?;
    let full = area.list_prefix(prefix)?;
    let limits = limits(ctx)?;
    let max = usize::try_from(limits.max_rows).unwrap_or(usize::MAX);
    let service = service(ctx);
    let access = access(ctx, &service, storage_id)?;
    check_key(&access, prefix, "prefix")?;
    let store = ctx.host.storage.store();
    let objects = block_on(store.list(&access, &full, max, deadline(&limits))).map_err(|e| {
        if e == StorageError::TooLarge {
            limit("too many objects; list a narrower prefix")
        } else {
            mapped(ctx, &service, storage_id, e)
        }
    })?;
    Ok(Value::Array(
        objects
            .iter()
            // A server with real folders lists them as `ordner/`; S3 shows an extension none.
            .filter(|object| !(access.location.is_tree() && object.key.ends_with('/')))
            .filter_map(|object| {
                Some(json!({
                    "key": area.strip(&object.key)?,
                    "size": object.size,
                    "lastModified": object.last_modified,
                }))
            })
            .collect(),
    ))
}

/// `{request: {backendId, key}}`: deletes the object under the extension's key.
pub fn delete(ctx: &CallContext, params: &Value) -> Result<Value, BridgeError> {
    let storage_id = text(params, "backendId")?;
    check_storage(ctx, Action::ReadWrite, storage_id)?;
    let own = text(params, "key")?;
    let area = Area::of(ctx)?;
    let key = area.key(own)?;
    let limits = limits(ctx)?;
    let service = service(ctx);
    let access = access(ctx, &service, storage_id)?;
    check_key(&access, own, "key")?;
    let store = ctx.host.storage.store();
    let deadline = deadline(&limits);
    block_on(store.delete(&access, &key, deadline))
        .map_err(|e| mapped(ctx, &service, storage_id, e))?;
    if access.location.is_tree() {
        block_on(prune(
            store.as_ref(),
            &access,
            area.prefix(),
            &key,
            deadline,
        ));
    }
    Ok(Value::Null)
}

#[cfg(test)]
#[path = "remote_storage_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "remote_storage_parity_tests.rs"]
mod parity_tests;
