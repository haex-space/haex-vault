//! Permission questions from bridge calls and `extension_permissions_check_database` (US3, T066,
//! T068). The extension can ask what it may do; it can never grant, resolve or change anything
//! (FR-021).

use serde_json::{json, Value};

use super::dispatch::CallContext;
use crate::extensions::error::{Asks, BridgeError, ExtensionErrorCode};
use crate::extensions::ids::ExtensionTable;
use crate::extensions::permissions::prompts::{FolderNames, PermissionRequestEvent, Question};
use crate::extensions::permissions::store::rows_of;
use crate::extensions::permissions::{
    evaluate, Action, Decision, PermissionKind, PermissionRequest, RequestTarget,
};
use crate::files::access::is_device_storage;
use crate::storage::query::Query;

/// Event to holzi's window: a question for the user.
pub const PERMISSION_REQUEST: &str = "extension-permission-request";
/// Event to holzi's window: a question nobody waits for any more.
pub const PERMISSION_REQUEST_CANCELLED: &str = "extension-permission-request-cancelled";

/// Puts the question of a 1004 answer before the user, once per identical open question. It names
/// `details.target` unless the answer [`Asks`] about something else.
pub fn ask(ctx: &CallContext, error: &BridgeError) {
    let Some(details) = &error.details else {
        return;
    };
    let text = |key: &str| details.get(key).and_then(Value::as_str).map(str::to_owned);
    let (Some(kind), Some(action), Some(told)) =
        (text("resourceType"), text("action"), text("target"))
    else {
        return;
    };
    let (target, folder) = match error.asks.as_deref() {
        None => (told.clone(), None),
        Some(Asks::Target(target)) => (target.clone(), None),
        Some(Asks::Nothing) => return,
        Some(Asks::Folder {
            storage_name,
            folder_name,
        }) => (
            told.clone(),
            Some(FolderNames {
                storage_name: storage_name.clone(),
                folder_name: folder_name.clone(),
                on_device: crate::extensions::permissions::FolderRef::parse(&told)
                    .is_some_and(|folder| is_device_storage(&folder.storage_id)),
            }),
        ),
    };
    let Some(kind) = PermissionKind::parse(&kind) else {
        return;
    };
    let extension_id = ctx.session.extension_id;
    let question = Question {
        extension_id,
        kind,
        action: action.clone(),
        target: target.clone(),
    };
    let (request_id, new) = ctx
        .host
        .permissions
        .ask(question, &ctx.session.frame, &told);
    if !new {
        return;
    }
    let ext = extension_id.to_string();
    let (lookup_action, lookup_target) = (action.clone(), target.clone());
    let facts = ctx.db.read_blocking(move |q| {
        let display = q.query_row(
            "SELECT coalesce(display_name, name) FROM extensions WHERE id = ?1 \
             UNION ALL SELECT coalesce(display_name, name) FROM dev_extensions_no_sync \
             WHERE id = ?1",
            &[&ext],
            |r| r.get::<_, String>(0),
        )?;
        let declared = rows_of(q, extension_id)
            .map_err(|e| haex_crdt::Error::consumer(e.to_string()))?
            .iter()
            .any(|r| r.declared && r.is_about(kind.as_str(), &lookup_action, &lookup_target));
        let missing = kind == PermissionKind::Database && !target_installed(q, &lookup_target)?;
        Ok((display.unwrap_or_default(), declared, missing))
    });
    let Ok((display_name, declared, target_missing)) = facts else {
        // Nobody was told: the question must not stay open, or every retry would be merged into it.
        ctx.host.permissions.take(&request_id);
        return;
    };
    let event = PermissionRequestEvent {
        request_id,
        extension_id: extension_id.to_string(),
        display_name,
        kind: kind.as_str().to_owned(),
        action,
        target,
        declared,
        device_scoped: kind.is_device_scoped(),
        target_missing,
        by_agent: ctx.session.mcp.calls_in_flight() > 0,
        encrypted_folder: folder,
    };
    if let Ok(payload) = serde_json::to_value(event) {
        ctx.emitter.emit(PERMISSION_REQUEST, payload);
    }
}

/// Whether the extension a `database` target names is installed (FR-062: a missing one gets the
/// same 1004, the question then offers only "Verweigern").
fn target_installed(q: &mut impl Query, target: &str) -> haex_crdt::Result<bool> {
    let Some(prefix) = target
        .strip_suffix("__*")
        .map(|base| format!("{base}__x"))
        .or_else(|| Some(target.to_owned()))
        .and_then(|name| ExtensionTable::parse(&name).ok())
        .map(|t| t.prefix)
    else {
        return Ok(false);
    };
    Ok(q.query_row(
        "SELECT count(*) FROM extensions \
             WHERE public_key = ?1 AND name = ?2 AND state = 'installed'",
        &[&prefix.public_key.as_str(), &prefix.name.as_str()],
        |r| r.get::<_, i64>(0),
    )?
    .unwrap_or(0)
        > 0)
}

/// `{resource, operation: read | write}` → `{status: granted | denied | ask}`; grants nothing.
pub fn check_database(ctx: &CallContext, params: &Value) -> Result<Value, BridgeError> {
    let invalid = || BridgeError::new(ExtensionErrorCode::Validation, "resource and operation");
    let resource = params
        .get("resource")
        .and_then(Value::as_str)
        .ok_or_else(invalid)?;
    let write = match params.get("operation").and_then(Value::as_str) {
        Some("read") => false,
        Some("write") => true,
        _ => return Err(invalid()),
    };
    let Ok(table) = ExtensionTable::parse(resource) else {
        return Ok(json!({ "status": "denied" }));
    };
    let policy = super::database::policy(ctx)?;
    let request = PermissionRequest {
        kind: PermissionKind::Database,
        action: if write {
            Action::ReadWrite
        } else {
            Action::Read
        },
        target: RequestTarget::Table(table.clone()),
    };
    let decision = if table.prefix == policy.own {
        Decision::Allow
    } else {
        evaluate(&policy.grants, &request, policy.device)
    };
    Ok(json!({
        "status": match decision {
            Decision::Allow => "granted",
            Decision::Deny => "denied",
            Decision::Prompt => "ask",
        }
    }))
}

pub const MODULE: &str = module_path!();
