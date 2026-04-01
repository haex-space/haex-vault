pub mod commands;

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

/// Read the configured log level for a source.
pub fn get_effective_log_level(
    conn: &rusqlite::Connection,
    extension_id: Option<&str>,
) -> LogLevel {
    if let Some(ext_id) = extension_id {
        if let Ok(level) = conn.query_row(
            &format!(
                "SELECT value FROM {} WHERE key = 'log_level' AND extension_id = ?1",
                crate::table_names::TABLE_VAULT_SETTINGS
            ),
            [ext_id],
            |row| row.get::<_, String>(0),
        ) {
            if let Some(l) = LogLevel::from_str(&level) {
                return l;
            }
        }
    }

    if let Ok(level) = conn.query_row(
        &format!(
            "SELECT value FROM {} WHERE key = 'log_level' AND extension_id IS NULL",
            crate::table_names::TABLE_VAULT_SETTINGS
        ),
        [],
        |row| row.get::<_, String>(0),
    ) {
        if let Some(l) = LogLevel::from_str(&level) {
            return l;
        }
    }

    LogLevel::from_str(DEFAULT_LOG_LEVEL).unwrap()
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

    let sql = format!(
        "INSERT INTO {} (id, timestamp, level, source, extension_id, message, metadata, device_id) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
        crate::table_names::TABLE_LOGS
    );

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

    let hlc = state.hlc.lock().map_err(|_| crate::database::error::DatabaseError::ValidationError {
        reason: "Failed to lock HLC service".to_string(),
    })?;

    crate::database::core::execute_with_crdt(sql, params, &state.db, &hlc)?;
    Ok(())
}

/// Read logs with optional filters.
/// Uses select_with_crdt to automatically filter tombstoned (deleted) rows.
pub fn query_logs(
    connection: &crate::database::DbConnection,
    query: &LogQueryParams,
) -> Result<Vec<LogEntry>, crate::database::error::DatabaseError> {
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

    // select_with_crdt automatically filters tombstoned rows
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

const DEFAULT_RETENTION_DAYS: i64 = 14;

/// Get the retention days for a source (extension or global).
fn get_retention_days(conn: &rusqlite::Connection, extension_id: Option<&str>) -> i64 {
    if let Some(ext_id) = extension_id {
        if let Ok(days) = conn.query_row(
            &format!(
                "SELECT value FROM {} WHERE key = 'log_retention_days' AND extension_id = ?1",
                crate::table_names::TABLE_VAULT_SETTINGS
            ),
            [ext_id],
            |row| row.get::<_, String>(0),
        ) {
            if let Ok(d) = days.parse::<i64>() {
                return d;
            }
        }
    }

    if let Ok(days) = conn.query_row(
        &format!(
            "SELECT value FROM {} WHERE key = 'log_retention_days' AND extension_id IS NULL",
            crate::table_names::TABLE_VAULT_SETTINGS
        ),
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

    let hlc = state.hlc.lock().map_err(|_| crate::database::error::DatabaseError::ValidationError {
        reason: "Failed to lock HLC service".to_string(),
    })?;

    // Read retention config using raw connection (read-only, no CRDT needed)
    let (global_cutoff_str, console_cutoff_str, custom_extensions) = crate::database::core::with_connection(&state.db, |conn| {
        let global_retention = get_retention_days(conn, None);
        let global_cutoff = time::OffsetDateTime::now_utc() - time::Duration::days(global_retention);
        let global_cutoff_str = global_cutoff.format(&time::format_description::well_known::Rfc3339).unwrap_or_default();

        let console_cutoff = time::OffsetDateTime::now_utc() - time::Duration::days(1);
        let console_cutoff_str = console_cutoff.format(&time::format_description::well_known::Rfc3339).unwrap_or_default();

        let mut custom_extensions: Vec<(String, i64)> = Vec::new();
        if let Ok(mut stmt) = conn.prepare(&format!(
            "SELECT extension_id, value FROM {} WHERE key = 'log_retention_days' AND extension_id IS NOT NULL",
            crate::table_names::TABLE_VAULT_SETTINGS
        )) {
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
    let sql = format!(
        "DELETE FROM {} WHERE source = 'console' AND extension_id IS NULL AND timestamp < ?1",
        crate::table_names::TABLE_LOGS
    );
    crate::database::core::execute_with_crdt(
        sql, vec![JsonValue::String(console_cutoff_str)], &state.db, &hlc,
    )?;
    total_deleted += 1; // execute_with_crdt doesn't return affected count

    // Extensions with custom retention
    for (ext_id, days) in &custom_extensions {
        let cutoff = time::OffsetDateTime::now_utc() - time::Duration::days(*days);
        let cutoff_str = cutoff.format(&time::format_description::well_known::Rfc3339).unwrap_or_default();
        let sql = format!(
            "DELETE FROM {} WHERE extension_id = ?1 AND timestamp < ?2",
            crate::table_names::TABLE_LOGS
        );
        crate::database::core::execute_with_crdt(
            sql, vec![JsonValue::String(ext_id.clone()), JsonValue::String(cutoff_str)], &state.db, &hlc,
        )?;
        total_deleted += 1;
    }

    // Everything else: global retention (excluding already-handled console + custom extensions)
    let custom_ids: Vec<&str> = custom_extensions.iter().map(|(id, _)| id.as_str()).collect();
    if custom_ids.is_empty() {
        let sql = format!(
            "DELETE FROM {} WHERE source != 'console' AND timestamp < ?1",
            crate::table_names::TABLE_LOGS
        );
        crate::database::core::execute_with_crdt(
            sql, vec![JsonValue::String(global_cutoff_str)], &state.db, &hlc,
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
