//! An extension changes, tests and removes storages (spec 038 US3, FR-013, FR-013a, FR-009b,
//! contracts/bridge.md, research R6, R8, R11; spec 050 US6); proposing a new one is in
//! [`super::remote_storage_propose`]. Every change goes through a dialog of holzi; credentials are
//! typed only in holzi's window over the whole app and never pass the bridge: a call with
//! credentials is refused before any dialog. A proposed endpoint is checked before the dialog too
//! ([`super::remote_storage_endpoint`]).

use std::time::SystemTime;

use serde_json::{json, Map, Value};

use super::bridge::blocking::block_on;
use super::bridge::dispatch::CallContext;
use super::commands::permissions::{set, PermissionSetArgs};
use super::error::{BridgeError, ExtensionErrorCode};
use super::permissions::Action;
pub(super) use super::remote_storage::place_of;
use super::remote_storage::{
    check_storage, invalid, item, not_found, overview, provider, service, text,
};
use super::remote_storage_dialog::{ask, open, Dialog, DialogKind, StorageAnswer};
use crate::error::HolziError;
use crate::passwords::clock::unix_millis;
use crate::remote_storage::model::{ConnectionInput, CredentialsInput, StorageInput};
use crate::remote_storage::service::StorageService;
use crate::remote_storage::TestOutcome;
use crate::storage::query::Query;

pub const MODULE: &str = module_path!();

/// Fields a call may never carry in `request` or `config` (FR-013a, spec 050 contracts/bridge.md).
const CREDENTIAL_FIELDS: [&str; 9] = [
    "accessKeyId",
    "secretAccessKey",
    "sessionToken",
    "username",
    "password",
    "privateKey",
    "passphrase",
    "hostKey",
    "fingerprint",
];

fn carries_credentials(map: &Map<String, Value>) -> bool {
    CREDENTIAL_FIELDS.iter().any(|f| map.contains_key(*f))
}

pub(super) fn cancelled() -> BridgeError {
    BridgeError::new(
        ExtensionErrorCode::PermissionDenied,
        "cancelled by the user",
    )
}

/// The extension's error for an error of the storage service; never its endpoint or region.
pub(super) fn from_service(error: HolziError) -> BridgeError {
    match error {
        HolziError::StorageTestFailed { outcome, .. } => provider(kind_of(outcome)),
        HolziError::StorageInvalid { field } => invalid(&format!("invalid {field}")),
        HolziError::StorageNotFound => not_found(),
        HolziError::StorageCredentialsUnavailable { .. } => provider("accessDenied"),
        _ => BridgeError::new(ExtensionErrorCode::Database, "database unavailable"),
    }
}

/// The `kind` of a failed test for the extension (research R7).
fn kind_of(outcome: TestOutcome) -> &'static str {
    match outcome {
        TestOutcome::Passed => "passed",
        TestOutcome::AccessDenied | TestOutcome::MissingRight => "accessDenied",
        TestOutcome::Unreachable
        | TestOutcome::NotWebDav
        | TestOutcome::HostKeyUnknown
        | TestOutcome::HostKeyChanged => "network",
        TestOutcome::BucketMissing => "bucketMissing",
        TestOutcome::FolderMissing => "folderMissing",
    }
}

/// What holzi's window sent with the credentials: whether to make a missing folder and the host
/// key the user confirmed (SFTP).
pub(super) struct Typed {
    pub credentials: CredentialsInput,
    pub create_folder: bool,
    pub host_key: Option<String>,
}

impl Typed {
    /// The credentials of a confirmed answer; none is a cancel.
    pub fn of(answer: Confirmed) -> Result<Self, BridgeError> {
        Ok(Self {
            credentials: answer.credentials.ok_or_else(cancelled)?,
            create_folder: answer.create_folder,
            host_key: answer.host_key,
        })
    }
}

/// Saves with what holzi's window sent. A failed test goes back to that window, which shows why
/// (for SFTP the host key to confirm) and waits for corrected credentials, the word to make the
/// folder, the confirmed key, or a cancel (1002); the extension never sees it.
pub(super) fn with_credentials<T>(
    dialog: &mut Dialog<'_>,
    first: Typed,
    save: impl Fn(Typed) -> crate::error::Result<T>,
) -> Result<T, BridgeError> {
    let mut typed = first;
    loop {
        match save(typed) {
            Err(HolziError::StorageTestFailed {
                outcome,
                leftover_key,
                host_key,
            }) => {
                dialog.failed(outcome, leftover_key, host_key);
                typed = Typed::of(confirmed(dialog.answer()?)?)?;
            }
            saved => return saved.map_err(from_service),
        }
    }
}

/// The request object of add and update, refused when it carries credentials (FR-013a).
pub(super) fn request(params: &Value) -> Result<&Map<String, Value>, BridgeError> {
    let request = params
        .get("request")
        .and_then(Value::as_object)
        .ok_or_else(|| invalid("request must be an object"))?;
    if carries_credentials(request) {
        return Err(invalid("credentials are entered in holzi"));
    }
    Ok(request)
}

/// `config`, refused when it carries credentials (FR-013a) or a field outside `allowed`.
pub(super) fn config(
    request: &Map<String, Value>,
    allowed: &[&str],
) -> Result<Map<String, Value>, BridgeError> {
    let config = match request.get("config") {
        None | Some(Value::Null) => Map::new(),
        Some(Value::Object(config)) => config.clone(),
        Some(_) => return Err(invalid("config must be an object")),
    };
    if carries_credentials(&config) {
        return Err(invalid("credentials are entered in holzi"));
    }
    if let Some(field) = config.keys().find(|k| !allowed.contains(&k.as_str())) {
        return Err(invalid(&format!("config.{field} is not allowed here")));
    }
    Ok(config)
}

pub(super) fn optional_text<'a>(
    map: &'a Map<String, Value>,
    name: &str,
) -> Result<Option<&'a str>, BridgeError> {
    match map.get(name) {
        None | Some(Value::Null) => Ok(None),
        Some(Value::String(text)) => Ok(Some(text.as_str())),
        Some(_) => Err(invalid(&format!("{name} must be a string"))),
    }
}

/// The calling extension's name as the user knows it (for the dialog).
pub(super) fn extension_name(ctx: &CallContext) -> String {
    let id = ctx.session.extension_id.to_string();
    ctx.db
        .read_blocking(move |q| {
            q.query_row(
                "SELECT COALESCE(display_name, name) FROM extensions WHERE id = ?1 \
                 UNION ALL SELECT COALESCE(display_name, name) FROM dev_extensions_no_sync \
                 WHERE id = ?1",
                &[&id],
                |r| r.get::<_, String>(0),
            )
        })
        .ok()
        .flatten()
        .unwrap_or_default()
}

/// Gives the extension "read and write" on the storage it created, for every own device.
pub(super) fn grant(ctx: &CallContext, storage_id: &str) -> Result<(), BridgeError> {
    set(
        &ctx.db,
        ctx.device,
        PermissionSetArgs {
            extension_id: ctx.session.extension_id.to_string(),
            kind: "remoteStorage".into(),
            action: "readWrite".into(),
            target: storage_id.to_owned(),
            status: "granted".into(),
            replaces: None,
        },
        unix_millis(SystemTime::now()),
    )
    .map_err(|_| BridgeError::new(ExtensionErrorCode::Database, "database unavailable"))
}

/// The list item of `storage_id` after a change.
pub(super) fn answer_item(
    service: &StorageService,
    storage_id: &str,
) -> Result<Value, BridgeError> {
    item(&overview(service)?, storage_id).ok_or_else(not_found)
}

/// What the user typed or chose in the answer of a confirmed dialog.
pub(super) struct Confirmed {
    pub connection_id: Option<String>,
    pub credentials: Option<CredentialsInput>,
    pub name: Option<String>,
    pub bucket: Option<String>,
    /// Make a missing folder of a WebDAV or SFTP storage (spec 050 FR-006).
    pub create_folder: bool,
    /// The host key the user confirmed (SFTP, spec 050 research R5).
    pub host_key: Option<String>,
}

pub(super) fn confirmed(answer: StorageAnswer) -> Result<Confirmed, BridgeError> {
    match answer {
        StorageAnswer::Confirm {
            connection_id,
            credentials,
            name,
            bucket,
            create_folder,
            host_key,
        } => Ok(Confirmed {
            connection_id,
            credentials,
            name,
            bucket,
            create_folder,
            host_key,
        }),
        StorageAnswer::Cancel => Err(cancelled()),
    }
}

/// `{request: {backendId, name?, config?: {bucket?}}}` (`folder?` on WebDAV and SFTP): a
/// confirmed change; new credentials only from holzi's window.
pub fn update_backend(ctx: &CallContext, params: &Value) -> Result<Value, BridgeError> {
    let request = request(params)?;
    let storage_id = text(params, "backendId")?;
    let config = config(request, &["bucket", "folder"])?;
    let name = optional_text(request, "name")?;
    check_storage(ctx, Action::ReadWrite, storage_id)?;
    let service = service(ctx);
    let storage = block_on(service.storage(storage_id)).map_err(from_service)?;
    let connection = block_on(service.connection(&storage.connection_id)).map_err(from_service)?;
    let place = place_of(connection.protocol);
    if let Some(other) = config.keys().find(|key| key.as_str() != place) {
        return Err(invalid(&format!("config.{other} is not allowed here")));
    }
    let bucket = optional_text(&config, place)?;
    let mut dialog = open(
        ctx,
        DialogKind::Update,
        &extension_name(ctx),
        json!({
            "storageId": storage.id,
            "currentName": storage.name,
            "currentBucket": storage.bucket,
            "name": name.unwrap_or(&storage.name),
            "bucket": bucket.unwrap_or(&storage.bucket),
            "providerName": connection.provider_name,
            "endpoint": connection.endpoint,
            "region": connection.region,
            "scope": connection.endpoint_scope.as_str(),
            "protocol": connection.protocol.as_str(),
        }),
        &[],
    )?;
    let mut answer = confirmed(dialog.answer()?)?;
    let new_bucket = answer
        .bucket
        .clone()
        .unwrap_or_else(|| bucket.unwrap_or(&storage.bucket).to_owned());
    let new_name = answer.name.take();
    let mut create_folder = answer.create_folder;
    if answer.credentials.is_some() {
        let first = Typed::of(answer)?;
        create_folder = with_credentials(&mut dialog, first, |typed| {
            let create_folder = typed.create_folder;
            block_on(service.save_connection(ConnectionInput {
                id: Some(connection.id.clone()),
                protocol: connection.protocol,
                provider_name: connection.provider_name.clone(),
                provider_kind: connection.provider_kind,
                endpoint: Some(connection.endpoint.clone()),
                region: connection.region.clone(),
                addressing: connection.addressing,
                credentials: Some(typed.credentials),
                bucket_for_test: new_bucket.clone(),
                create_folder,
                host_key: typed.host_key,
            }))
            .map(|_| create_folder)
        })?;
    }
    drop(dialog);
    block_on(service.save_storage(StorageInput {
        id: Some(storage.id.clone()),
        connection_id: storage.connection_id.clone(),
        name: new_name.unwrap_or_else(|| name.unwrap_or(&storage.name).to_owned()),
        bucket: new_bucket,
        create_folder,
    }))
    .map_err(from_service)?;
    answer_item(&service, &storage.id)
}

/// What a test or removal dialog shows of `storage`: its name and its bucket or folder.
fn shown(
    service: &StorageService,
    storage: &crate::remote_storage::StorageRow,
) -> Result<Value, BridgeError> {
    let connection = block_on(service.connection(&storage.connection_id)).map_err(from_service)?;
    Ok(json!({
        "storageId": storage.id,
        "name": storage.name,
        "bucket": storage.bucket,
        "protocol": connection.protocol.as_str(),
    }))
}

/// `{backendId}`: a confirmed test; `null` when it passed, else 2002 with its `kind`.
pub fn test_backend(ctx: &CallContext, params: &Value) -> Result<Value, BridgeError> {
    let storage_id = text(params, "backendId")?;
    check_storage(ctx, Action::ReadWrite, storage_id)?;
    let service = service(ctx);
    let storage = block_on(service.storage(storage_id)).map_err(from_service)?;
    confirmed(ask(
        ctx,
        DialogKind::Test,
        &extension_name(ctx),
        shown(&service, &storage)?,
        &[],
    )?)?;
    let result = block_on(service.test_storage(storage_id)).map_err(from_service)?;
    match result.outcome {
        TestOutcome::Passed => Ok(Value::Null),
        outcome => Err(provider(kind_of(outcome))),
    }
}

/// `{backendId}`: a confirmed removal; the dialog names other extensions that lose access.
pub fn remove_backend(ctx: &CallContext, params: &Value) -> Result<Value, BridgeError> {
    let storage_id = text(params, "backendId")?;
    check_storage(ctx, Action::ReadWrite, storage_id)?;
    let service = service(ctx);
    let storage = block_on(service.storage(storage_id)).map_err(from_service)?;
    let own = extension_name(ctx);
    let id = storage.id.clone();
    let others: Vec<String> = ctx
        .db
        .read_blocking(move |q| {
            crate::remote_storage::store::extensions_of(q, &id).map_err(Into::into)
        })
        .unwrap_or_default()
        .into_iter()
        .filter(|name| *name != own)
        .collect();
    confirmed(ask(
        ctx,
        DialogKind::Remove,
        &own,
        shown(&service, &storage)?,
        &others,
    )?)?;
    block_on(service.remove_storage(storage_id)).map_err(from_service)?;
    Ok(Value::Null)
}

#[cfg(test)]
#[path = "remote_storage_manage_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "remote_storage_credentials_tests.rs"]
mod credentials_tests;
