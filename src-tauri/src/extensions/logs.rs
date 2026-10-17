//! The log of an extension (spec 017, US5, T086, contracts/bridge.md `extension_logging_*`, method
//! names as in haex-vault 2.x (Legacy)): an extension writes and reads only its own entries on this device;
//! holzi's settings read them with [`read`]. The entries stay on this device
//! (`extension_logs_no_sync`), at most [`MAX_ENTRIES`] per extension and device: the oldest go
//! first.

use haex_crdt::rusqlite::params;
use serde::Serialize;
use serde_json::{json, Value};
use ts_rs::TS;
use uuid::Uuid;

use crate::extensions::bridge::dispatch::CallContext;
use crate::extensions::error::{BridgeError, ExtensionErrorCode};
use crate::storage::query::Query;

pub const MODULE: &str = module_path!();

/// Entries kept per extension.
pub const MAX_ENTRIES: i64 = 5_000;
/// Bytes of one message.
pub const MAX_MESSAGE_BYTES: usize = 4 * 1024;
/// Bytes of the metadata of one entry, as JSON.
pub const MAX_METADATA_BYTES: usize = 16 * 1024;
/// Entries one read returns at most.
pub const MAX_READ: i64 = 500;

const LEVELS: [&str; 4] = ["debug", "info", "warn", "error"];

/// One entry as the settings and the extension read it.
#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/types/bindings/")]
pub struct LogEntry {
    #[ts(type = "number")]
    pub id: i64,
    /// `debug`, `info`, `warn` or `error`.
    pub level: String,
    pub message: String,
    /// The JSON the extension gave, as text.
    #[ts(optional)]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub metadata: Option<String>,
    /// Milliseconds.
    #[ts(type = "number")]
    pub created_at: i64,
}

/// Which entries a read wants.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct LogQuery {
    /// Only this level.
    pub level: Option<String>,
    pub limit: i64,
    /// Skips the newest `offset` entries (the SDK's paging).
    pub offset: i64,
    /// Only entries older than this id (the settings' paging).
    pub before: Option<i64>,
}

fn invalid(message: &str) -> BridgeError {
    BridgeError::new(ExtensionErrorCode::Validation, message)
}

fn unavailable() -> BridgeError {
    BridgeError::new(ExtensionErrorCode::Database, "database unavailable")
}

/// A known level, or a validation error.
fn level_of(value: &str) -> Result<&'static str, BridgeError> {
    LEVELS
        .into_iter()
        .find(|level| *level == value)
        .ok_or_else(|| invalid("unknown level"))
}

/// The newest entries of `extension_id` on `device` that match `query`, newest first.
pub fn read(
    q: &mut impl Query,
    extension_id: Uuid,
    device: Uuid,
    query: &LogQuery,
) -> haex_crdt::Result<Vec<LogEntry>> {
    q.query_map(
        "SELECT id, level, message, metadata, created_at FROM extension_logs_no_sync \
         WHERE extension_id = ?1 AND vault_device_uuid = ?2 \
         AND (?3 IS NULL OR level = ?3) AND (?4 IS NULL OR id < ?4) \
         ORDER BY id DESC LIMIT ?5 OFFSET ?6",
        params![
            extension_id.to_string(),
            device.to_string(),
            query.level,
            query.before,
            query.limit.clamp(0, MAX_READ),
            query.offset.max(0)
        ],
        |r| {
            Ok(LogEntry {
                id: r.get(0)?,
                level: r.get(1)?,
                message: r.get(2)?,
                metadata: r.get(3)?,
                created_at: r.get(4)?,
            })
        },
    )
}

/// `{level, message, metadata?}`: adds an entry for the caller on this device.
pub fn write(ctx: &CallContext, params: &Value) -> Result<Value, BridgeError> {
    let level = level_of(
        params
            .get("level")
            .and_then(Value::as_str)
            .ok_or_else(|| invalid("level must be a string"))?,
    )?;
    let message = params
        .get("message")
        .and_then(Value::as_str)
        .ok_or_else(|| invalid("message must be a string"))?
        .to_owned();
    if message.len() > MAX_MESSAGE_BYTES {
        return Err(BridgeError::new(
            ExtensionErrorCode::LimitExceeded,
            "message too long",
        ));
    }
    let metadata = match params.get("metadata") {
        None | Some(Value::Null) => None,
        Some(value) => Some(value.to_string()),
    };
    if metadata
        .as_ref()
        .is_some_and(|m| m.len() > MAX_METADATA_BYTES)
    {
        return Err(BridgeError::new(
            ExtensionErrorCode::LimitExceeded,
            "metadata too large",
        ));
    }
    let ext = ctx.session.extension_id.to_string();
    let device = ctx.device.to_string();
    let now = crate::passwords::clock::unix_millis(std::time::SystemTime::now());
    ctx.db
        .write_blocking(move |tx| {
            tx.execute(
                "INSERT INTO extension_logs_no_sync \
                 (extension_id, vault_device_uuid, level, message, metadata, created_at) \
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                params![ext, device, level, message, metadata, now],
            )?;
            // The ring buffer of this extension on this device: everything behind its newest
            // MAX_ENTRIES goes. Rows of another device id (a copied vault file) stay.
            tx.execute(
                "DELETE FROM extension_logs_no_sync \
                 WHERE extension_id = ?1 AND vault_device_uuid = ?2 AND id <= \
                 (SELECT id FROM extension_logs_no_sync \
                  WHERE extension_id = ?1 AND vault_device_uuid = ?2 \
                  ORDER BY id DESC LIMIT 1 OFFSET ?3)",
                params![ext, device, MAX_ENTRIES],
            )?;
            Ok(())
        })
        .map_err(|_| unavailable())?;
    Ok(Value::Null)
}

/// `{level?, limit?, offset?}`: the caller's own entries on this device, newest first.
pub fn read_own(ctx: &CallContext, params: &Value) -> Result<Value, BridgeError> {
    let level = match params.get("level") {
        None | Some(Value::Null) => None,
        Some(value) => Some(
            level_of(
                value
                    .as_str()
                    .ok_or_else(|| invalid("level must be a string"))?,
            )?
            .to_owned(),
        ),
    };
    let number = |name: &str, default: i64| match params.get(name) {
        None | Some(Value::Null) => Ok(default),
        Some(value) => value
            .as_i64()
            .filter(|n| *n >= 0)
            .ok_or_else(|| invalid("limit and offset must be whole numbers")),
    };
    let query = LogQuery {
        level,
        limit: number("limit", 100)?,
        offset: number("offset", 0)?,
        before: None,
    };
    let (ext, device) = (ctx.session.extension_id, ctx.device);
    let entries = ctx
        .db
        .read_blocking(move |q| read(q, ext, device, &query))
        .map_err(|_| unavailable())?;
    Ok(json!(entries))
}

#[cfg(test)]
#[path = "logs_tests.rs"]
mod tests;
