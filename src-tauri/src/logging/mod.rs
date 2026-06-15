pub mod commands;
mod queries;

use queries::{
    SQL_DELETE_CONSOLE_LOGS_BEFORE, SQL_DELETE_EXTENSION_LOGS_BEFORE,
    SQL_DELETE_LOGS_EXCEPT_CONSOLE_BEFORE, SQL_GET_LOG_LEVEL_BY_EXTENSION,
    SQL_GET_LOG_LEVEL_GLOBAL, SQL_GET_RETENTION_DAYS_BY_EXTENSION, SQL_GET_RETENTION_DAYS_GLOBAL,
    SQL_INSERT_LOG_FULL, SQL_INSERT_LOG_MINIMAL, SQL_LIST_CUSTOM_RETENTION_EXTENSIONS,
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
/// uniform shape across `haex_logs`.
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
    debug_assert!(max > 0, "log_truncate called with max=0 — would erase identifier");
    s.chars().take(max).collect()
}

/// Read the configured log level for a source.
pub fn get_effective_log_level(
    conn: &rusqlite::Connection,
    extension_id: Option<&str>,
) -> LogLevel {
    if let Some(ext_id) = extension_id {
        if let Ok(level) = conn.query_row(
            &SQL_GET_LOG_LEVEL_BY_EXTENSION,
            [ext_id],
            |row| row.get::<_, String>(0),
        ) {
            if let Some(l) = LogLevel::from_str(&level) {
                return l;
            }
        }
    }

    if let Ok(level) = conn.query_row(
        &SQL_GET_LOG_LEVEL_GLOBAL,
        [],
        |row| row.get::<_, String>(0),
    ) {
        if let Some(l) = LogLevel::from_str(&level) {
            return l;
        }
    }

    LogLevel::from_str(DEFAULT_LOG_LEVEL)
        .expect("invariant: DEFAULT_LOG_LEVEL is a hardcoded string that must parse")
}

/// Log to both stderr and the CRDT-synced DB log table.
/// Use this from subsystems that have direct DB/HLC access but no AppState.
/// Locks HLC internally — safe to call from anywhere.
///
/// ## Structured metadata
///
/// `metadata` is an optional JSON object that lands in `haex_logs.metadata`.
/// By convention, set `{"subsystem": "AuthGate"}` (or whatever subsystem you
/// log from) so operators can filter the in-app log viewer by subsystem
/// independent of the per-op `source` tag. If `metadata.subsystem` is
/// present, the stderr line is also prefixed with `[<subsystem>]` so a
/// `grep "[AuthGate]"` against container logs still works.
///
/// `None` is the backward-compatible call shape — most existing callers pass
/// it and behave as before (no metadata column, no stderr prefix).
///
/// ## `device_id` is hardcoded to `"rust"`
///
/// Both the `None` (minimal) and `Some` (full) insert paths hardcode the
/// `haex_logs.device_id` column to the literal string `"rust"`, matching
/// `SQL_INSERT_LOG_MINIMAL`'s pre-existing behaviour. This means **all**
/// rows written by `log_to_db` collapse to a synthetic `"rust"` device on
/// CRDT-sync — operators filtering the in-app log viewer by device cannot
/// distinguish which physical Vault device emitted the row.
///
/// This is intentional for now: `log_to_db` is called from contexts that
/// don't carry `AppState`, so threading a real device_id through requires
/// either an additional parameter at every call site or a thread-local /
/// `OnceLock<String>` initialized at vault startup. Both are in scope for
/// the `Result<(), DatabaseError>` signature migration; see
/// `docs/plans/2026-06-13-critical-failure-pattern.md`. If you need
/// per-device attribution before that lands, use `insert_log` (which takes
/// a real `device_id: &str`) instead.
///
/// ## Failure modes
///
/// Two paths that previously failed silently now emit a `[CRITICAL]` stderr
/// marker so the audit-row loss is visible in CI / container logs:
///
/// 1. `hlc.lock()` returning `Err` (HLC mutex poisoned by an earlier panic).
/// 2. `execute_with_crdt` returning `Err` (e.g. schema drift on `haex_logs`,
///    poisoned DB mutex, transaction failure).
///
/// The function still returns `()` — silent best-effort semantics are
/// preserved. A follow-up PR will migrate the signature to
/// `Result<(), DatabaseError>` so individual callers can decide between
/// propagating, retrying, and emitting a critical notification. Tracked
/// in `docs/plans/2026-06-13-critical-failure-pattern.md`.
pub fn log_to_db(
    db: &crate::database::DbConnection,
    hlc: &std::sync::Arc<std::sync::Mutex<crate::crdt::hlc::HlcService>>,
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

    let hlc_guard = match hlc.lock() {
        Ok(g) => g,
        Err(_) => {
            eprintln!(
                "[CRITICAL] [log_to_db] HLC mutex poisoned — audit row LOST for source={source}, level={level}"
            );
            return;
        }
    };

    let id = uuid::Uuid::new_v4().to_string();
    let now = time::OffsetDateTime::now_utc();
    let timestamp = now.format(&time::format_description::well_known::Rfc3339).unwrap_or_default();

    // Choose minimal vs full insert based on whether metadata was supplied.
    // The minimal path keeps the historical behaviour for callers that
    // pass None; the full path populates the metadata column with the JSON
    // string serialization of the supplied value.
    let (sql, params) = match metadata {
        None => (
            SQL_INSERT_LOG_MINIMAL.clone(),
            vec![
                serde_json::Value::String(id),
                serde_json::Value::String(timestamp),
                serde_json::Value::String(level.to_string()),
                serde_json::Value::String(source.to_string()),
                serde_json::Value::String(message.to_string()),
            ],
        ),
        Some(meta) => (
            SQL_INSERT_LOG_FULL.clone(),
            vec![
                serde_json::Value::String(id),
                serde_json::Value::String(timestamp),
                serde_json::Value::String(level.to_string()),
                serde_json::Value::String(source.to_string()),
                serde_json::Value::Null, // extension_id
                serde_json::Value::String(message.to_string()),
                serde_json::Value::String(meta.to_string()),
                serde_json::Value::String("rust".to_string()), // device_id
            ],
        ),
    };

    if let Err(e) = crate::database::core::execute_with_crdt(sql, params, db, &hlc_guard) {
        eprintln!(
            "[CRITICAL] [log_to_db] DB write failed — audit row LOST for source={source}, level={level}, err={e}"
        );
    }
}

/// Insert a log entry via CRDT-aware execution (synced across devices).
/// NOTE: The console interceptor filters out sync-related messages (`[SYNC]` prefix)
/// to prevent a feedback loop: sync log → interceptor → insert → CRDT dirty → push → ∞
pub fn insert_log(
    state: &crate::AppState,
    level: &str,
    source: &str,
    extension_id: Option<&str>,
    message: &str,
    metadata: Option<serde_json::Value>,
    device_id: &str,
) -> Result<(), crate::database::error::DatabaseError> {
    let id = uuid::Uuid::new_v4().to_string();
    let now = time::OffsetDateTime::now_utc();
    let timestamp = now.format(&time::format_description::well_known::Rfc3339).unwrap_or_default();
    let metadata_str = metadata.map(|m| m.to_string());

    let params: Vec<serde_json::Value> = vec![
        serde_json::Value::String(id),
        serde_json::Value::String(timestamp),
        serde_json::Value::String(level.to_string()),
        serde_json::Value::String(source.to_string()),
        match extension_id {
            Some(eid) => serde_json::Value::String(eid.to_string()),
            None => serde_json::Value::Null,
        },
        serde_json::Value::String(message.to_string()),
        match metadata_str {
            Some(m) => serde_json::Value::String(m),
            None => serde_json::Value::Null,
        },
        serde_json::Value::String(device_id.to_string()),
    ];

    let hlc = state.lock_or_fail(
        &state.hlc,
        crate::critical::CriticalFailureCode::HlcMutexPoisoned,
        "logging::insert_log",
        serde_json::json!({}),
    )?;

    crate::database::core::execute_with_crdt(SQL_INSERT_LOG_FULL.clone(), params, &state.db, &hlc)?;
    Ok(())
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
            let levels: Vec<&str> = [LogLevel::Debug, LogLevel::Info, LogLevel::Warn, LogLevel::Error]
                .iter()
                .filter(|l| **l >= min_level)
                .map(|l| l.as_str())
                .collect();
            let placeholders: Vec<String> = levels.iter().enumerate()
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

    rows.iter().map(|row| {
        Ok(LogEntry {
            id: json_to_opt_string(row.get(0).unwrap_or(&JsonValue::Null)).unwrap_or_default(),
            timestamp: json_to_opt_string(row.get(1).unwrap_or(&JsonValue::Null)).unwrap_or_default(),
            level: json_to_opt_string(row.get(2).unwrap_or(&JsonValue::Null)).unwrap_or_default(),
            source: json_to_opt_string(row.get(3).unwrap_or(&JsonValue::Null)).unwrap_or_default(),
            extension_id: json_to_opt_string(row.get(4).unwrap_or(&JsonValue::Null)),
            message: json_to_opt_string(row.get(5).unwrap_or(&JsonValue::Null)).unwrap_or_default(),
            metadata: json_to_opt_string(row.get(6).unwrap_or(&JsonValue::Null)),
            device_id: json_to_opt_string(row.get(7).unwrap_or(&JsonValue::Null)).unwrap_or_default(),
        })
    }).collect()
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
        if let Ok(days) = conn.query_row(
            &SQL_GET_RETENTION_DAYS_BY_EXTENSION,
            [ext_id],
            |row| row.get::<_, String>(0),
        ) {
            if let Ok(d) = days.parse::<i64>() {
                return d;
            }
        }
    }

    if let Ok(days) = conn.query_row(
        &SQL_GET_RETENTION_DAYS_GLOBAL,
        [],
        |row| row.get::<_, String>(0),
    ) {
        if let Ok(d) = days.parse::<i64>() {
            return d;
        }
    }

    DEFAULT_RETENTION_DAYS
}

/// Delete log entries older than the configured retention period.
/// Handles per-extension retention: extensions with custom retention
/// are cleaned separately, remaining logs use the global retention.
/// Uses execute_with_crdt to properly create tombstones instead of hard-deleting.
pub fn cleanup_logs(state: &crate::AppState) -> Result<usize, crate::database::error::DatabaseError> {
    use serde_json::Value as JsonValue;

    let hlc = state.lock_or_fail(
        &state.hlc,
        crate::critical::CriticalFailureCode::HlcMutexPoisoned,
        "logging::cleanup_logs",
        serde_json::json!({}),
    )?;

    // Read retention config using raw connection (read-only, no CRDT needed)
    let (global_cutoff_str, console_cutoff_str, custom_extensions) = crate::database::core::with_connection(&state.db, |conn| {
        let global_retention = get_retention_days(conn, None);
        let global_cutoff = time::OffsetDateTime::now_utc() - time::Duration::days(global_retention);
        let global_cutoff_str = global_cutoff.format(&time::format_description::well_known::Rfc3339).unwrap_or_default();

        let console_cutoff = time::OffsetDateTime::now_utc() - time::Duration::days(1);
        let console_cutoff_str = console_cutoff.format(&time::format_description::well_known::Rfc3339).unwrap_or_default();

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

    let mut total_deleted = 0;

    // Console interceptor logs: 1 day retention (via CRDT soft-delete)
    crate::database::core::execute_with_crdt(
        SQL_DELETE_CONSOLE_LOGS_BEFORE.clone(),
        vec![JsonValue::String(console_cutoff_str)],
        &state.db,
        &hlc,
    )?;
    total_deleted += 1; // execute_with_crdt doesn't return affected count

    // Extensions with custom retention
    for (ext_id, days) in &custom_extensions {
        let cutoff = time::OffsetDateTime::now_utc() - time::Duration::days(*days);
        let cutoff_str = cutoff.format(&time::format_description::well_known::Rfc3339).unwrap_or_default();
        crate::database::core::execute_with_crdt(
            SQL_DELETE_EXTENSION_LOGS_BEFORE.clone(),
            vec![JsonValue::String(ext_id.clone()), JsonValue::String(cutoff_str)],
            &state.db,
            &hlc,
        )?;
        total_deleted += 1;
    }

    // Everything else: global retention (excluding already-handled console + custom extensions)
    let custom_ids: Vec<&str> = custom_extensions.iter().map(|(id, _)| id.as_str()).collect();
    if custom_ids.is_empty() {
        crate::database::core::execute_with_crdt(
            SQL_DELETE_LOGS_EXCEPT_CONSOLE_BEFORE.clone(),
            vec![JsonValue::String(global_cutoff_str)],
            &state.db,
            &hlc,
        )?;
    } else {
        let mut params: Vec<JsonValue> = vec![JsonValue::String(global_cutoff_str)];
        let placeholders: Vec<String> = custom_ids.iter().enumerate()
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
        crate::database::core::execute_with_crdt(sql, params, &state.db, &hlc)?;
    }
    total_deleted += 1;

    Ok(total_deleted)
}
