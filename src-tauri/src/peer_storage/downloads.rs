//! Local registry of peer-to-peer downloads (haex_peer_downloads_no_sync).
//!
//! Lets `peer_storage_remote_read` skip a redundant re-download when the
//! user re-clicks a file in the file browser. Keyed by
//! (endpoint_id, remote_path) — "have I downloaded THIS file from THIS
//! peer before?" — so two peers exposing files that happen to share a
//! name are tracked independently.
//!
//! Local-only (`_no_sync`): `local_path` is a filesystem path on desktop
//! and a JSON-encoded Android `FsUri` on Android, both inherently
//! per-device.

use crate::database::{core, DbConnection};
use crate::peer_storage::error::PeerStorageError;
use time::format_description::well_known::Rfc3339;
use time::OffsetDateTime;

#[derive(Debug, Clone)]
pub struct DownloadRecord {
    pub size: u64,
    pub modified: Option<u64>,
    pub local_path: String,
}

fn map_db(e: crate::database::error::DatabaseError) -> PeerStorageError {
    PeerStorageError::Database {
        reason: e.to_string(),
    }
}

/// Look up a previously-recorded download for (endpoint_id, remote_path).
/// Returns `None` if no row exists.
pub fn find(
    db: &DbConnection,
    endpoint_id: &str,
    remote_path: &str,
) -> Result<Option<DownloadRecord>, PeerStorageError> {
    let rows = core::select(
        "SELECT size, modified, local_path \
         FROM haex_peer_downloads_no_sync \
         WHERE endpoint_id = ?1 AND remote_path = ?2 \
         LIMIT 1"
            .to_string(),
        vec![
            serde_json::Value::String(endpoint_id.to_string()),
            serde_json::Value::String(remote_path.to_string()),
        ],
        db,
    )
    .map_err(map_db)?;

    let Some(row) = rows.first() else {
        return Ok(None);
    };

    let size = row.first().and_then(|v| v.as_u64()).unwrap_or(0);
    let modified = row.get(1).and_then(|v| v.as_u64());
    let local_path = row
        .get(2)
        .and_then(|v| v.as_str())
        .unwrap_or_default()
        .to_string();

    Ok(Some(DownloadRecord {
        size,
        modified,
        local_path,
    }))
}

/// Insert or update the registry row for a successfully completed download.
/// Uses the composite primary key (endpoint_id, remote_path) for UPSERT —
/// re-downloading the same source after upstream-edit replaces the row.
pub fn upsert(
    db: &DbConnection,
    endpoint_id: &str,
    remote_path: &str,
    size: u64,
    modified: Option<u64>,
    local_path: &str,
) -> Result<(), PeerStorageError> {
    let now =
        OffsetDateTime::now_utc()
            .format(&Rfc3339)
            .map_err(|e| PeerStorageError::Database {
                reason: format!("rfc3339: {e}"),
            })?;

    let modified_json = modified
        .and_then(|m| serde_json::Number::from_u128(m as u128).map(serde_json::Value::Number))
        .unwrap_or(serde_json::Value::Null);

    core::execute(
        "INSERT INTO haex_peer_downloads_no_sync \
           (endpoint_id, remote_path, size, modified, local_path, downloaded_at) \
         VALUES (?1, ?2, ?3, ?4, ?5, ?6) \
         ON CONFLICT(endpoint_id, remote_path) DO UPDATE SET \
           size = excluded.size, \
           modified = excluded.modified, \
           local_path = excluded.local_path, \
           downloaded_at = excluded.downloaded_at"
            .to_string(),
        vec![
            serde_json::Value::String(endpoint_id.to_string()),
            serde_json::Value::String(remote_path.to_string()),
            serde_json::Value::Number(size.into()),
            modified_json,
            serde_json::Value::String(local_path.to_string()),
            serde_json::Value::String(now),
        ],
        db,
    )
    .map_err(map_db)?;

    Ok(())
}

/// Drop a stale registry row — the recorded local_path no longer exists or
/// its size/mtime no longer matches what the peer says, so the cached
/// reference is dead.
pub fn delete(
    db: &DbConnection,
    endpoint_id: &str,
    remote_path: &str,
) -> Result<(), PeerStorageError> {
    core::execute(
        "DELETE FROM haex_peer_downloads_no_sync \
         WHERE endpoint_id = ?1 AND remote_path = ?2"
            .to_string(),
        vec![
            serde_json::Value::String(endpoint_id.to_string()),
            serde_json::Value::String(remote_path.to_string()),
        ],
        db,
    )
    .map_err(map_db)?;
    Ok(())
}

/// Sanitize a user-controlled folder segment (space name) for use as a
/// directory name. Strips path separators and control characters and
/// trims, falling back to the spaceId when the result would be empty.
/// Same on every platform so the dedup key stays stable.
pub fn sanitize_folder_segment(raw: &str, fallback: &str) -> String {
    let cleaned: String = raw
        .chars()
        .map(|c| match c {
            '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|' | '\0' => '_',
            c if c.is_control() => '_',
            c => c,
        })
        .collect();
    let trimmed = cleaned.trim().trim_matches('.');
    if trimmed.is_empty() {
        fallback.to_string()
    } else {
        trimmed.to_string()
    }
}

#[cfg(test)]
mod tests;
