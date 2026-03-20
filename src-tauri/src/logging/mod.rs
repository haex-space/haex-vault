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
pub fn query_logs(
    conn: &rusqlite::Connection,
    query: &LogQueryParams,
) -> Result<Vec<LogEntry>, crate::database::error::DatabaseError> {
    let mut conditions: Vec<String> = Vec::new();
    let mut param_values: Vec<Box<dyn rusqlite::types::ToSql>> = Vec::new();
    let mut idx = 1;

    if let Some(ref source) = query.source {
        conditions.push(format!("source = ?{idx}"));
        param_values.push(Box::new(source.clone()));
        idx += 1;
    }
    if let Some(ref ext_id) = query.extension_id {
        conditions.push(format!("extension_id = ?{idx}"));
        param_values.push(Box::new(ext_id.clone()));
        idx += 1;
    }
    if let Some(ref since) = query.since {
        conditions.push(format!("timestamp >= ?{idx}"));
        param_values.push(Box::new(since.clone()));
        idx += 1;
    }
    if let Some(ref until) = query.until {
        conditions.push(format!("timestamp <= ?{idx}"));
        param_values.push(Box::new(until.clone()));
        idx += 1;
    }
    if let Some(ref device_id) = query.device_id {
        conditions.push(format!("device_id = ?{idx}"));
        param_values.push(Box::new(device_id.clone()));
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
                param_values.push(Box::new(l.to_string()));
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
    param_values.push(Box::new(limit));
    param_values.push(Box::new(offset));

    let refs: Vec<&dyn rusqlite::types::ToSql> = param_values.iter().map(|p| p.as_ref()).collect();
    let mut stmt = conn.prepare(&sql).map_err(|e| crate::database::error::DatabaseError::QueryError { reason: e.to_string() })?;
    let rows = stmt.query_map(refs.as_slice(), |row| {
        Ok(LogEntry {
            id: row.get(0)?,
            timestamp: row.get(1)?,
            level: row.get(2)?,
            source: row.get(3)?,
            extension_id: row.get(4)?,
            message: row.get(5)?,
            metadata: row.get(6)?,
            device_id: row.get(7)?,
        })
    }).map_err(|e| crate::database::error::DatabaseError::QueryError { reason: e.to_string() })?;

    rows.collect::<Result<Vec<_>, _>>()
        .map_err(|e| crate::database::error::DatabaseError::QueryError { reason: e.to_string() })
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
pub fn cleanup_logs(conn: &rusqlite::Connection) -> Result<usize, crate::database::error::DatabaseError> {
    let global_retention = get_retention_days(conn, None);
    let global_cutoff = time::OffsetDateTime::now_utc() - time::Duration::days(global_retention);
    let global_cutoff_str = global_cutoff.format(&time::format_description::well_known::Rfc3339).unwrap_or_default();

    // Collect extensions with custom retention
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

    let mut total_deleted = 0;

    // Console interceptor logs: 1 day retention
    let console_cutoff = time::OffsetDateTime::now_utc() - time::Duration::days(1);
    let console_cutoff_str = console_cutoff.format(&time::format_description::well_known::Rfc3339).unwrap_or_default();
    total_deleted += conn.execute(
        &format!(
            "DELETE FROM {} WHERE source = 'console' AND extension_id IS NULL AND timestamp < ?1",
            crate::table_names::TABLE_LOGS
        ),
        rusqlite::params![console_cutoff_str],
    ).unwrap_or(0);

    // Extensions with custom retention
    for (ext_id, days) in &custom_extensions {
        let cutoff = time::OffsetDateTime::now_utc() - time::Duration::days(*days);
        let cutoff_str = cutoff.format(&time::format_description::well_known::Rfc3339).unwrap_or_default();
        total_deleted += conn.execute(
            &format!(
                "DELETE FROM {} WHERE extension_id = ?1 AND timestamp < ?2",
                crate::table_names::TABLE_LOGS
            ),
            rusqlite::params![ext_id, cutoff_str],
        ).unwrap_or(0);
    }

    // Everything else: global retention (excluding already-handled console + custom extensions)
    let custom_ids: Vec<&str> = custom_extensions.iter().map(|(id, _)| id.as_str()).collect();
    if custom_ids.is_empty() {
        total_deleted += conn.execute(
            &format!(
                "DELETE FROM {} WHERE source != 'console' AND timestamp < ?1",
                crate::table_names::TABLE_LOGS
            ),
            rusqlite::params![global_cutoff_str],
        ).unwrap_or(0);
    } else {
        let placeholders: Vec<String> = custom_ids.iter().enumerate()
            .map(|(i, _)| format!("?{}", i + 2))
            .collect();
        let sql = format!(
            "DELETE FROM {} WHERE source != 'console' AND timestamp < ?1 AND (extension_id IS NULL OR extension_id NOT IN ({}))",
            crate::table_names::TABLE_LOGS,
            placeholders.join(",")
        );
        let mut params: Vec<Box<dyn rusqlite::types::ToSql>> = Vec::new();
        params.push(Box::new(global_cutoff_str));
        for id in &custom_ids {
            params.push(Box::new(id.to_string()));
        }
        let refs: Vec<&dyn rusqlite::types::ToSql> = params.iter().map(|p| p.as_ref()).collect();
        total_deleted += conn.execute(&sql, refs.as_slice()).unwrap_or(0);
    }

    Ok(total_deleted)
}
