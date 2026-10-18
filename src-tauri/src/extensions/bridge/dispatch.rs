//! The one checkpoint for calls from extension frames (ADR-0004, research R14, FR-060): the frame
//! session says who calls, the allowlist [`METHODS`] says what can be called. A method that is not
//! listed answers "not supported" (8000), one of a later delivery "not available" (8001), every
//! call of a disabled extension "disabled" (8002).

use std::sync::Arc;

use serde_json::{json, Value};
use uuid::Uuid;

use super::frames::{FrameSession, FrameSource};
use crate::extensions::error::{BridgeError, ExtensionErrorCode};
use crate::extensions::host::ExtensionHost;
use crate::storage::query::Query;
use crate::vault_gate::VaultDb;

/// Sends an event to holzi's own window.
pub trait Emit: Send + Sync {
    fn emit(&self, event: &str, payload: Value);
}

/// Everything a bridge method may use: the session it was called from, never anything the
/// extension says about itself.
pub struct CallContext {
    pub db: VaultDb,
    pub host: Arc<ExtensionHost>,
    pub session: Arc<FrameSession>,
    /// This device's `vault_device_uuid`.
    pub device: Uuid,
    pub emitter: Arc<dyn Emit>,
}

pub use super::allowlist::METHODS;

pub type Handler = fn(&CallContext, &Value) -> Result<Value, BridgeError>;

/// One method of the allowlist; `module` is where its handler lives (contract test, FR-009).
pub struct Method {
    pub name: &'static str,
    pub handler: Handler,
    pub module: &'static str,
}

/// Methods of later deliveries (research R1): they answer 8001 until they land. Remote storage
/// landed with spec 038.
const LATER: &[&str] = &[];

/// Whether the caller may run host functions: an installed, enabled extension, or a development
/// version while developer mode is on for this device (US12).
pub(crate) fn is_enabled(ctx: &CallContext) -> Result<bool, BridgeError> {
    let id = ctx.session.extension_id;
    let device = ctx.device;
    let dev = ctx.session.source == FrameSource::DevServer;
    ctx.db
        .read_blocking(move |q| {
            if dev {
                return Ok(crate::extensions::dev::start(q, id, device).is_ok());
            }
            Ok(q.query_row(
                "SELECT enabled FROM extensions WHERE id = ?1 AND state = 'installed'",
                &[&id.to_string()],
                |r| r.get::<_, i64>(0),
            )? == Some(1))
        })
        .map_err(|_| BridgeError::new(ExtensionErrorCode::Database, "database unavailable"))
}

/// Runs one call. Blocking (methods read the vault). A call that needs a permission (1004) puts
/// its question before the user (`permissions::ask`).
pub fn call(ctx: &CallContext, method: &str, params: &Value) -> Result<Value, BridgeError> {
    if !is_enabled(ctx)? {
        return Err(BridgeError::disabled());
    }
    if let Some(found) = METHODS.iter().find(|m| m.name == method) {
        return (found.handler)(ctx, params).inspect_err(|error| {
            if error.code == ExtensionErrorCode::PermissionPromptRequired {
                super::permissions::ask(ctx, error);
            }
        });
    }
    if LATER.iter().any(|prefix| method.starts_with(prefix)) {
        return Err(BridgeError::not_available());
    }
    Err(BridgeError::not_supported())
}

/// The answer in the form the SDK reads: `{id, result}` or `{id, error}`.
pub fn answer(id: Value, outcome: Result<Value, BridgeError>) -> Value {
    match outcome {
        Ok(result) => json!({ "id": id, "result": result }),
        Err(error) => json!({ "id": id, "error": error }),
    }
}

#[cfg(test)]
#[path = "dispatch_tests.rs"]
pub(in crate::extensions::bridge) mod tests;
