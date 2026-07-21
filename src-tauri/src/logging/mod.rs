pub mod commands;
mod queries;
mod sink;

pub use sink::LogSink;

use queries::{
    SQL_DELETE_CONSOLE_LOGS_BEFORE, SQL_DELETE_EXTENSION_LOGS_BEFORE,
    SQL_DELETE_LOGS_EXCEPT_CONSOLE_BEFORE, SQL_GET_LOG_LEVEL_BY_EXTENSION,
    SQL_GET_LOG_LEVEL_GLOBAL, SQL_GET_RETENTION_DAYS_BY_EXTENSION, SQL_GET_RETENTION_DAYS_GLOBAL,
    SQL_LIST_CUSTOM_RETENTION_EXTENSIONS,
};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum LogLevel {
    Debug,
    Info,
    Warn,
    Error,
}

impl LogLevel {
    pub fn as_str(&self) -> &'static str {
        match self {
            LogLevel::Debug => "debug",
            LogLevel::Info => "info",
            LogLevel::Warn => "warn",
            LogLevel::Error => "error",
        }
    }

    pub fn from_str(s: &str) -> Option<Self> {
        match s {
            "debug" => Some(LogLevel::Debug),
            "info" => Some(LogLevel::Info),
            "warn" => Some(LogLevel::Warn),
            "error" => Some(LogLevel::Error),
            _ => None,
        }
    }
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LogEntry {
    pub id: String,
    pub timestamp: String,
    pub level: String,
    pub source: String,
    pub extension_id: Option<String>,
    pub message: String,
    pub metadata: Option<String>,
    pub device_id: String,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LogQueryParams {
    pub source: Option<String>,
    pub extension_id: Option<String>,
    pub level: Option<String>,
    pub since: Option<String>,
    pub until: Option<String>,
    pub device_id: Option<String>,
    pub limit: Option<i64>,
    pub offset: Option<i64>,
}

pub const DEFAULT_LOG_LEVEL: &str = "warn";

/// Established convention across the codebase for how many characters of
/// a DID, space ID, or endpoint ID to keep in log messages. Long enough
/// to be unambiguous to an operator triaging by eye, short enough to
/// keep one log line on one terminal row. Used by every `log_truncate`
/// caller in the codebase — DO NOT pass a different `max` to
/// [`log_truncate`] without updating this constant; the goal is a
/// uniform shape across `haex_logs_no_sync`.
pub const LOG_TRUNCATE_DEFAULT: usize = 24;

/// UTF-8-safe truncation for log message interpolation.
///
/// DIDs and space IDs are long opaque strings; logs need a short enough
/// fragment to fit in a single line yet enough to identify the principal.
/// Slicing by byte (`&s[..max]`) would panic on a multi-byte UTF-8
/// boundary; `.chars().take(max).collect()` is the safe pattern, but
/// repeating it inline 12+ times across the codebase invited drift in
/// the truncation length and in whether to use `.chars()` or
/// `.bytes()`. This helper enforces both.
///
/// `max == 0` is a programmer error (would strip every identifier from
/// every log line) and is caught in debug builds by `debug_assert!`. In
/// release builds it returns an empty string — the call has no panic
/// surface even on a bad caller.
///
/// Used wherever a log message embeds an attacker- or peer-controlled
/// identifier: AuthGate reject paths, peer_storage handlers,
/// multi_leader.rs, endpoint.rs — always at [`LOG_TRUNCATE_DEFAULT`].
pub fn log_truncate(s: &str, max: usize) -> String {
    debug_assert!(
        max > 0,
        "log_truncate called with max=0 — would erase identifier"
    );
    s.chars().take(max).collect()
}

/// Read the configured log level for a source.
pub fn get_effective_log_level(
    conn: &rusqlite::Connection,
    extension_id: Option<&str>,
) -> LogLevel {
    if let Some(ext_id) = extension_id {
        if let Ok(level) = conn.query_row(&SQL_GET_LOG_LEVEL_BY_EXTENSION, [ext_id], |row| {
            row.get::<_, String>(0)
        }) {
            if let Some(l) = LogLevel::from_str(&level) {
                return l;
            }
        }
    }

    if let Ok(level) = conn.query_row(&SQL_GET_LOG_LEVEL_GLOBAL, [], |row| row.get::<_, String>(0))
    {
        if let Some(l) = LogLevel::from_str(&level) {
            return l;
        }
    }

    LogLevel::from_str(DEFAULT_LOG_LEVEL)
        .expect("invariant: DEFAULT_LOG_LEVEL is a hardcoded string that must parse")
}

/// Log to both stderr and the local (`_no_sync`) log table.
///
/// Post-2026-07-21 refactor: writes go through the dedicated
/// [`LogSink`] connection, so a blocked / poisoned main DB mutex no
/// longer silences logging. Removes the per-log HLC lock and the
/// owner-device replication that previously turned every log into a
/// CRDT transaction — see `docs/plans/2026-07-21-haex-logs-no-sync.md`.
///
/// ## Sink availability
///
/// `sink` is `Option` so callers on the pre-vault path (or tests
/// without a mounted vault) can still log — passing `None` degrades to
/// stderr-only, no panic, no audit row (best-effort). Callers with an
/// `AppState`/`AppHandle` in scope should snapshot the sink via
/// [`crate::AppState::log_sink_snapshot`] once at the top of their
/// scope and pass `sink.as_ref()` down.
///
/// ## Structured metadata
///
/// `metadata` is an optional JSON object that lands in
/// `haex_logs_no_sync.metadata`. By convention, set
/// `{"subsystem": "AuthGate"}` so operators can filter the in-app log
/// viewer by subsystem independent of the per-op `source` tag. If
/// `metadata.subsystem` is present, the stderr line is also prefixed
/// with `[<subsystem>]` so a `grep "[AuthGate]"` against container
/// logs still works.
///
/// ## `device_id` is hardcoded to `"rust"`
///
/// The insert hardcodes `haex_logs_no_sync.device_id` to `"rust"`.
/// Same intentional trade-off as before the refactor: threading a real
/// device_id through every callsite is deferred to
/// `docs/plans/2026-06-13-critical-failure-pattern.md`. Callers that
/// need per-device attribution should use [`insert_log`] (takes a
/// real `device_id: &str`) instead.
///
/// ## Failure modes
///
/// Best-effort — the function returns `()`. Two paths emit a
/// `[CRITICAL]` stderr marker so audit-row loss is visible in CI:
///
/// 1. `sink` is `None` (no vault mounted).
/// 2. `LogSink::write` returning `Err` (schema drift, sink mutex
///    poisoned, or the second connection failed to reach the file).
///
/// A follow-up PR will migrate the signature to
/// `Result<(), DatabaseError>` so callers can decide between propagating,
/// retrying, and emitting a critical notification; tracked in
/// `docs/plans/2026-06-13-critical-failure-pattern.md`.
pub fn log_to_db(
    sink: Option<&LogSink>,
    level: &str,
    source: &str,
    message: &str,
    metadata: Option<serde_json::Value>,
) {
    // Subsystem prefix for stderr legibility — restores the `[AuthGate]`-style
    // marker that pre-T6 reject paths used to emit. If metadata is None or
    // has no `subsystem` field, no prefix is added (backward-compatible).
    let subsystem_prefix = metadata
        .as_ref()
        .and_then(|m| m.get("subsystem"))
        .and_then(|s| s.as_str())
        .map(|s| format!("[{s}] "))
        .unwrap_or_default();
    eprintln!("{subsystem_prefix}[{source}] [{level}] {message}");

    let Some(sink) = sink else {
        // Pre-vault / test contexts without a mounted vault. Silent
        // best-effort — same semantics as the pre-refactor "audit row
        // LOST" branches, but now unified into a single branch.
        return;
    };

    let id = uuid::Uuid::new_v4().to_string();
    let now = time::OffsetDateTime::now_utc();
    let timestamp = now
        .format(&time::format_description::well_known::Rfc3339)
        .unwrap_or_default();
    let metadata_str = metadata.as_ref().map(|m| m.to_string());
    let extension_id: Option<&str> = None;

    if let Err(e) = sink.write(
        &id,
        &timestamp,
        level,
        source,
        extension_id,
        message,
        metadata_str.as_deref(),
        "rust",
    ) {
        eprintln!(
            "[CRITICAL] [log_to_db] sink write failed — audit row LOST for source={source}, level={level}, err={e}"
        );
    }
}

/// Insert a log entry via the dedicated [`LogSink`] connection.
///
/// Unlike [`log_to_db`], this variant takes a real `device_id` and
/// returns `Result`, so callers with `AppState` in scope can propagate
/// write failures. Post-2026-07-21 refactor: no HLC lock, no CRDT
/// transaction, no owner-device replication.
pub fn insert_log(
    state: &crate::AppState,
    level: &str,
    source: &str,
    extension_id: Option<&str>,
    message: &str,
    metadata: Option<serde_json::Value>,
    device_id: &str,
) -> Result<(), crate::database::error::DatabaseError> {
    let sink_guard = state.log_sink.lock().map_err(|e| {
        crate::database::error::DatabaseError::LockError {
            reason: e.to_string(),
        }
    })?;
    let Some(sink) = sink_guard.as_ref() else {
        // No vault mounted — same best-effort semantics as log_to_db.
        eprintln!("[{source}] [{level}] {message}");
        return Ok(());
    };

    let id = uuid::Uuid::new_v4().to_string();
    let now = time::OffsetDateTime::now_utc();
    let timestamp = now
        .format(&time::format_description::well_known::Rfc3339)
        .unwrap_or_default();
    let metadata_str = metadata.map(|m| m.to_string());

    sink.write(
        &id,
        &timestamp,
        level,
        source,
        extension_id,
        message,
        metadata_str.as_deref(),
        device_id,
    )
    .map_err(|e| crate::database::error::DatabaseError::DatabaseError {
        reason: format!("log sink write failed: {e}"),
    })
}

/// Build the WHERE clause + bound parameters shared by `query_logs` and `count_logs`.
fn build_log_filter(query: &LogQueryParams) -> (String, Vec<serde_json::Value>, usize) {
    use serde_json::Value as JsonValue;

    let mut conditions: Vec<String> = Vec::new();
    let mut params: Vec<JsonValue> = Vec::new();
    let mut idx = 1;

    if let Some(ref source) = query.source {
        conditions.push(format!("source = ?{idx}"));
        params.push(JsonValue::String(source.clone()));
        idx += 1;
    }
    if let Some(ref ext_id) = query.extension_id {
        conditions.push(format!("extension_id = ?{idx}"));
        params.push(JsonValue::String(ext_id.clone()));
        idx += 1;
    }
    if let Some(ref since) = query.since {
        conditions.push(format!("timestamp >= ?{idx}"));
        params.push(JsonValue::String(since.clone()));
        idx += 1;
    }
    if let Some(ref until) = query.until {
        conditions.push(format!("timestamp <= ?{idx}"));
        params.push(JsonValue::String(until.clone()));
        idx += 1;
    }
    if let Some(ref device_id) = query.device_id {
        conditions.push(format!("device_id = ?{idx}"));
        params.push(JsonValue::String(device_id.clone()));
        idx += 1;
    }
    if let Some(ref level) = query.level {
        if let Some(min_level) = LogLevel::from_str(level) {
            let levels: Vec<&str> = [
                LogLevel::Debug,
                LogLevel::Info,
                LogLevel::Warn,
                LogLevel::Error,
            ]
            .iter()
            .filter(|l| **l >= min_level)
            .map(|l| l.as_str())
            .collect();
            let placeholders: Vec<String> = levels
                .iter()
                .enumerate()
                .map(|(i, _)| format!("?{}", idx + i))
                .collect();
            conditions.push(format!("level IN ({})", placeholders.join(",")));
            for l in &levels {
                params.push(JsonValue::String(l.to_string()));
                idx += 1;
            }
        }
    }

    let where_clause = if conditions.is_empty() {
        String::new()
    } else {
        format!("WHERE {}", conditions.join(" AND "))
    };

    (where_clause, params, idx)
}

/// Read logs with optional filters.
///
/// Routed through `select_with_crdt` so any future SELECT-side CRDT
/// transformation (e.g. once the delete-log gains a `WHERE NOT IN
/// (deleted)` projection) is automatically applied. Today
/// `transform_query` is a no-op for plain SELECTs — tombstone
/// filtering happens at INSERT/UPDATE time via the delete-log, not at
/// read time — so the routing buys nothing observable on its own; it
/// just keeps this query on the same code path the rest of the
/// codebase uses.
pub fn query_logs(
    connection: &crate::database::DbConnection,
    query: &LogQueryParams,
) -> Result<Vec<LogEntry>, crate::database::error::DatabaseError> {
    use serde_json::Value as JsonValue;

    let (where_clause, mut params, idx) = build_log_filter(query);

    let limit = query.limit.unwrap_or(500);
    let offset = query.offset.unwrap_or(0);

    let sql = format!(
        "SELECT id, timestamp, level, source, extension_id, message, metadata, device_id FROM {} {} ORDER BY timestamp DESC LIMIT ?{} OFFSET ?{}",
        crate::table_names::TABLE_LOGS,
        where_clause,
        idx,
        idx + 1,
    );
    params.push(JsonValue::Number(limit.into()));
    params.push(JsonValue::Number(offset.into()));

    // Routed through select_with_crdt for SELECT-side codepath parity —
    // see the module-level note above; no tombstone filter today.
    let rows = crate::database::core::select_with_crdt(sql, params, connection)?;

    fn json_to_opt_string(val: &JsonValue) -> Option<String> {
        match val {
            JsonValue::String(s) => Some(s.clone()),
            JsonValue::Null => None,
            other => Some(other.to_string()),
        }
    }

    rows.iter()
        .map(|row| {
            Ok(LogEntry {
                id: json_to_opt_string(row.get(0).unwrap_or(&JsonValue::Null)).unwrap_or_default(),
                timestamp: json_to_opt_string(row.get(1).unwrap_or(&JsonValue::Null))
                    .unwrap_or_default(),
                level: json_to_opt_string(row.get(2).unwrap_or(&JsonValue::Null))
                    .unwrap_or_default(),
                source: json_to_opt_string(row.get(3).unwrap_or(&JsonValue::Null))
                    .unwrap_or_default(),
                extension_id: json_to_opt_string(row.get(4).unwrap_or(&JsonValue::Null)),
                message: json_to_opt_string(row.get(5).unwrap_or(&JsonValue::Null))
                    .unwrap_or_default(),
                metadata: json_to_opt_string(row.get(6).unwrap_or(&JsonValue::Null)),
                device_id: json_to_opt_string(row.get(7).unwrap_or(&JsonValue::Null))
                    .unwrap_or_default(),
            })
        })
        .collect()
}

/// Count logs matching the same filters used by `query_logs` (limit/offset are ignored).
/// Uses select_with_crdt so tombstoned rows are excluded.
pub fn count_logs(
    connection: &crate::database::DbConnection,
    query: &LogQueryParams,
) -> Result<i64, crate::database::error::DatabaseError> {
    use serde_json::Value as JsonValue;

    let (where_clause, params, _) = build_log_filter(query);

    let sql = format!(
        "SELECT COUNT(*) FROM {} {}",
        crate::table_names::TABLE_LOGS,
        where_clause,
    );

    let rows = crate::database::core::select_with_crdt(sql, params, connection)?;

    Ok(rows
        .first()
        .and_then(|row| row.first())
        .and_then(|val| match val {
            JsonValue::Number(n) => n.as_i64(),
            _ => None,
        })
        .unwrap_or(0))
}

const DEFAULT_RETENTION_DAYS: i64 = 14;

/// Get the retention days for a source (extension or global).
fn get_retention_days(conn: &rusqlite::Connection, extension_id: Option<&str>) -> i64 {
    if let Some(ext_id) = extension_id {
        if let Ok(days) = conn.query_row(&SQL_GET_RETENTION_DAYS_BY_EXTENSION, [ext_id], |row| {
            row.get::<_, String>(0)
        }) {
            if let Ok(d) = days.parse::<i64>() {
                return d;
            }
        }
    }

    if let Ok(days) = conn.query_row(&SQL_GET_RETENTION_DAYS_GLOBAL, [], |row| {
        row.get::<_, String>(0)
    }) {
        if let Ok(d) = days.parse::<i64>() {
            return d;
        }
    }

    DEFAULT_RETENTION_DAYS
}

/// Delete log entries older than the configured retention period.
/// Handles per-extension retention: extensions with custom retention
/// are cleaned separately, remaining logs use the global retention.
///
/// Post-2026-07-21 refactor: DELETEs run on the [`LogSink`]'s dedicated
/// connection (plain SQL — the `_no_sync` table has no CRDT triggers to
/// bypass and no delete-log to write). Retention config is still read
/// from `haex_vault_settings` via the main connection.
pub fn cleanup_logs(
    state: &crate::AppState,
) -> Result<usize, crate::database::error::DatabaseError> {
    use serde_json::Value as JsonValue;

    // Read retention config from vault_settings (main connection).
    let (global_cutoff_str, console_cutoff_str, custom_extensions) =
        crate::database::core::with_connection(&state.db, |conn| {
            let global_retention = get_retention_days(conn, None);
            let global_cutoff =
                time::OffsetDateTime::now_utc() - time::Duration::days(global_retention);
            let global_cutoff_str = global_cutoff
                .format(&time::format_description::well_known::Rfc3339)
                .unwrap_or_default();

            let console_cutoff = time::OffsetDateTime::now_utc() - time::Duration::days(1);
            let console_cutoff_str = console_cutoff
                .format(&time::format_description::well_known::Rfc3339)
                .unwrap_or_default();

            let mut custom_extensions: Vec<(String, i64)> = Vec::new();
            if let Ok(mut stmt) = conn.prepare(&SQL_LIST_CUSTOM_RETENTION_EXTENSIONS) {
                if let Ok(rows) = stmt.query_map([], |row| {
                    Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
                }) {
                    for row in rows.flatten() {
                        if let Ok(days) = row.1.parse::<i64>() {
                            custom_extensions.push((row.0, days));
                        }
                    }
                }
            }

            Ok((global_cutoff_str, console_cutoff_str, custom_extensions))
        })?;

    let sink_guard = state.log_sink.lock().map_err(|e| {
        crate::database::error::DatabaseError::LockError {
            reason: e.to_string(),
        }
    })?;
    let Some(sink) = sink_guard.as_ref() else {
        // No vault mounted — nothing to clean.
        return Ok(0);
    };

    let mut total_deleted = 0usize;

    // Console interceptor logs: 1 day retention.
    total_deleted += sink
        .execute(
            &SQL_DELETE_CONSOLE_LOGS_BEFORE,
            &[JsonValue::String(console_cutoff_str)],
        )
        .map_err(|e| crate::database::error::DatabaseError::DatabaseError {
            reason: format!("log sink cleanup failed (console): {e}"),
        })?;

    // Extensions with custom retention.
    for (ext_id, days) in &custom_extensions {
        let cutoff = time::OffsetDateTime::now_utc() - time::Duration::days(*days);
        let cutoff_str = cutoff
            .format(&time::format_description::well_known::Rfc3339)
            .unwrap_or_default();
        total_deleted += sink
            .execute(
                &SQL_DELETE_EXTENSION_LOGS_BEFORE,
                &[
                    JsonValue::String(ext_id.clone()),
                    JsonValue::String(cutoff_str),
                ],
            )
            .map_err(|e| crate::database::error::DatabaseError::DatabaseError {
                reason: format!("log sink cleanup failed (ext {ext_id}): {e}"),
            })?;
    }

    // Everything else: global retention (excluding already-handled
    // console + custom extensions).
    let custom_ids: Vec<&str> = custom_extensions
        .iter()
        .map(|(id, _)| id.as_str())
        .collect();
    total_deleted += if custom_ids.is_empty() {
        sink.execute(
            &SQL_DELETE_LOGS_EXCEPT_CONSOLE_BEFORE,
            &[JsonValue::String(global_cutoff_str)],
        )
    } else {
        let mut params: Vec<JsonValue> = vec![JsonValue::String(global_cutoff_str)];
        let placeholders: Vec<String> = custom_ids
            .iter()
            .enumerate()
            .map(|(i, _)| format!("?{}", i + 2))
            .collect();
        for id in &custom_ids {
            params.push(JsonValue::String(id.to_string()));
        }
        let sql = format!(
            "DELETE FROM {} WHERE source != 'console' AND timestamp < ?1 AND (extension_id IS NULL OR extension_id NOT IN ({}))",
            crate::table_names::TABLE_LOGS,
            placeholders.join(",")
        );
        sink.execute(&sql, &params)
    }
    .map_err(|e| crate::database::error::DatabaseError::DatabaseError {
        reason: format!("log sink cleanup failed (global): {e}"),
    })?;

    Ok(total_deleted)
}
