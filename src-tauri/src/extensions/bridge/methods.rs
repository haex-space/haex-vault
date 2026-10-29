//! The bridge methods of L1 that are not about data: context, own info, tab attention.

use std::sync::mpsc;
use std::time::Duration;

use serde_json::{json, Value};
use uuid::Uuid;

use super::dispatch::CallContext;
use crate::error::HolziError;
use crate::extensions::bundle::Manifest;
use crate::extensions::error::{BridgeError, ExtensionErrorCode};
use crate::extensions::host::ExtensionHost;
use crate::extensions::protocol::token;
use crate::storage::query::Query;

pub const MODULE: &str = module_path!();

/// The SDK's `ApplicationContext` on `device`: `{theme, locale, platform, deviceId}`.
pub fn context_of(host: &ExtensionHost, device: Uuid) -> Value {
    let context = host.context();
    json!({
        "theme": context.theme,
        "locale": context.locale,
        "platform": crate::platform::os_name(),
        "deviceId": device.to_string(),
    })
}

/// `{theme, locale, platform, deviceId}`.
pub fn context_get(ctx: &CallContext, _params: &Value) -> Result<Value, BridgeError> {
    Ok(context_of(&ctx.host, ctx.device))
}

/// The manifest the calling frame runs: of its bundle, or for a development version the one in its
/// project folder now, as long as it still names the registered prefix (the SQL policy's).
pub(super) fn manifest_of(ctx: &CallContext) -> Result<Manifest, BridgeError> {
    let unavailable = || BridgeError::new(ExtensionErrorCode::Database, "database unavailable");
    let unreadable = || BridgeError::new(ExtensionErrorCode::Manifest, "manifest unreadable");
    let Some(bundle_id) = ctx.session.source.bundle() else {
        let id = ctx.session.extension_id;
        let registration = ctx
            .db
            .read_blocking(move |q| crate::extensions::dev::registration(q, id).map_err(Into::into))
            .map_err(|_| unavailable())?
            .ok_or_else(|| BridgeError::new(ExtensionErrorCode::NotFound, "not found"))?;
        return match crate::extensions::dev::current_project(&registration) {
            Ok(project) => Ok(project.manifest),
            Err(HolziError::ExtensionInstall { reason }) if reason == "dev_project_changed" => {
                Err(BridgeError::new(
                    ExtensionErrorCode::Manifest,
                    "the project names another key or name now; load it again",
                ))
            }
            Err(_) => Err(unreadable()),
        };
    };
    let bundle_id = bundle_id.to_string();
    let manifest_json = ctx
        .db
        .read_blocking(move |q| {
            q.query_row(
                "SELECT manifest_json FROM extension_bundles WHERE id = ?1",
                &[&bundle_id],
                |r| r.get::<_, Vec<u8>>(0),
            )
        })
        .map_err(|_| unavailable())?
        .ok_or_else(|| BridgeError::new(ExtensionErrorCode::NotFound, "not found"))?;
    Manifest::from_stored(&manifest_json).map_err(|_| unreadable())
}

/// `{publicKey, name, version, displayName}` of the calling extension only.
pub fn get_info(ctx: &CallContext, _params: &Value) -> Result<Value, BridgeError> {
    let manifest = manifest_of(ctx)?;
    Ok(json!({
        "publicKey": manifest.public_key.as_str(),
        "name": manifest.name.as_str(),
        "version": manifest.version.to_string(),
        "displayName": manifest.title(),
    }))
}

/// `{active}`: marks or unmarks the calling frame's own tab (event `extension-tab-attention`).
pub fn tab_attention(ctx: &CallContext, params: &Value) -> Result<Value, BridgeError> {
    let active = params
        .get("active")
        .and_then(Value::as_bool)
        .ok_or_else(|| {
            BridgeError::new(ExtensionErrorCode::Validation, "active must be a boolean")
        })?;
    ctx.emitter.emit(
        "extension-tab-attention",
        json!({ "frame": ctx.session.frame, "active": active }),
    );
    Ok(Value::Null)
}

/// Longest dialog text and longest title or button label (characters).
const MAX_DIALOG_MESSAGE: usize = 2000;
const MAX_DIALOG_LABEL: usize = 200;
/// A dialog nobody answers ends as "cancel", so the call never waits forever.
const DIALOG_WAIT: Duration = Duration::from_secs(300);

fn optional_text(params: &Value, key: &str) -> Result<Option<String>, BridgeError> {
    match params.get(key) {
        None | Some(Value::Null) => Ok(None),
        Some(Value::String(text)) if text.chars().count() <= MAX_DIALOG_LABEL => {
            Ok(Some(text.clone()))
        }
        Some(_) => Err(BridgeError::new(
            ExtensionErrorCode::Validation,
            format!("{key} must be a string of at most {MAX_DIALOG_LABEL} characters"),
        )),
    }
}

/// `{message, title?, confirmLabel?, cancelLabel?, destructive?}` → `true` or `false`: holzi asks
/// the user over the calling frame's own tab (replaces `confirm()`, which the sandbox blocks;
/// research R12, T045). Blocks this call until the answer, the frame's end or [`DIALOG_WAIT`].
pub fn dialog_confirm(ctx: &CallContext, params: &Value) -> Result<Value, BridgeError> {
    let message = params
        .get("message")
        .and_then(Value::as_str)
        .filter(|m| !m.is_empty() && m.chars().count() <= MAX_DIALOG_MESSAGE)
        .ok_or_else(|| {
            BridgeError::new(
                ExtensionErrorCode::Validation,
                format!("message must be a string of 1 to {MAX_DIALOG_MESSAGE} characters"),
            )
        })?;
    let title = optional_text(params, "title")?;
    let confirm_label = optional_text(params, "confirmLabel")?;
    let cancel_label = optional_text(params, "cancelLabel")?;
    let destructive = params.get("destructive").and_then(Value::as_bool) == Some(true);

    let request_id = token::mint();
    let frame = &ctx.session.frame;
    let (answer, waiting) = mpsc::channel();
    if !ctx.host.open_dialog(&request_id, frame, answer) {
        return Err(BridgeError::new(
            ExtensionErrorCode::LimitExceeded,
            "a dialog of this frame is already open",
        ));
    }
    ctx.emitter.emit(
        "extension-dialog-request",
        json!({
            "requestId": request_id,
            "frame": frame,
            "message": message,
            "title": title,
            "confirmLabel": confirm_label,
            "cancelLabel": cancel_label,
            "destructive": destructive,
        }),
    );
    let confirmed = waiting.recv_timeout(DIALOG_WAIT).unwrap_or(false);
    // After a timeout the dialog is still registered; answering it later must not reach anyone.
    ctx.host.resolve_dialog(&request_id, false);
    Ok(Value::Bool(confirmed))
}
