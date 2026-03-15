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
    pub source_type: String,
    pub message: String,
    pub metadata: Option<String>,
    pub device_id: String,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LogQueryParams {
    pub source: Option<String>,
    pub source_type: Option<String>,
    pub level: Option<String>,
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

/// Insert a log entry.
pub fn insert_log(
    conn: &rusqlite::Connection,
    level: &str,
    source: &str,
    source_type: &str,
    message: &str,
    metadata: Option<serde_json::Value>,
    device_id: &str,
) -> Result<(), crate::database::error::DatabaseError> {
    let id = uuid::Uuid::new_v4().to_string();
    let now = time::OffsetDateTime::now_utc();
    let timestamp = now.format(&time::format_description::well_known::Rfc3339).unwrap_or_default();
    let metadata_str = metadata.map(|m| m.to_string());

    conn.execute(
        &format!(
            "INSERT INTO {} (id, timestamp, level, source, source_type, message, metadata, device_id) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
            crate::table_names::TABLE_LOGS
        ),
        rusqlite::params![id, timestamp, level, source, source_type, message, metadata_str, device_id],
    ).map_err(|e| crate::database::error::DatabaseError::QueryError { reason: e.to_string() })?;

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
    if let Some(ref source_type) = query.source_type {
        conditions.push(format!("source_type = ?{idx}"));
        param_values.push(Box::new(source_type.clone()));
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
        "SELECT id, timestamp, level, source, source_type, message, metadata, device_id FROM {} {} ORDER BY timestamp DESC LIMIT ?{} OFFSET ?{}",
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
            source_type: row.get(4)?,
            message: row.get(5)?,
            metadata: row.get(6)?,
            device_id: row.get(7)?,
        })
    }).map_err(|e| crate::database::error::DatabaseError::QueryError { reason: e.to_string() })?;

    rows.collect::<Result<Vec<_>, _>>()
        .map_err(|e| crate::database::error::DatabaseError::QueryError { reason: e.to_string() })
}
