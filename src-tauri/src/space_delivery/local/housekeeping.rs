//! Cleanup routines for expired buffer data (messages, key packages, welcomes, pending commits).

use serde_json::json;

use crate::database::core;
use crate::database::DbConnection;

use super::error::DeliveryError;

/// Default TTL values (used when vault settings are not configured)
pub const DEFAULT_MESSAGE_TTL_DAYS: i64 = 7;
pub const DEFAULT_KEY_PACKAGE_TTL_HOURS: i64 = 24;
pub const DEFAULT_WELCOME_TTL_DAYS: i64 = 7;
pub const DEFAULT_PENDING_COMMIT_TTL_HOURS: i64 = 1;

/// Statistics from a cleanup run.
#[derive(Debug, Default)]
pub struct CleanupStats {
    pub messages_deleted: usize,
    pub key_packages_deleted: usize,
    pub welcomes_deleted: usize,
    pub pending_commits_deleted: usize,
}

/// Run all cleanup routines for a space.
pub fn cleanup_space(
    db: &DbConnection,
    space_id: &str,
    message_ttl_days: i64,
    key_package_ttl_hours: i64,
    welcome_ttl_days: i64,
    pending_commit_ttl_hours: i64,
) -> Result<CleanupStats, DeliveryError> {
    // Delete expired messages
    core::execute(
        "DELETE FROM haex_local_delivery_messages_no_sync WHERE space_id = ?1 AND created_at < datetime('now', ?2)".to_string(),
        vec![json!(space_id), json!(format!("-{message_ttl_days} days"))],
        db,
    )
    .map_err(|e| DeliveryError::Database {
        reason: e.to_string(),
    })?;

    // Delete expired key packages
    core::execute(
        "DELETE FROM haex_local_delivery_key_packages_no_sync WHERE space_id = ?1 AND created_at < datetime('now', ?2)".to_string(),
        vec![json!(space_id), json!(format!("-{key_package_ttl_hours} hours"))],
        db,
    )
    .map_err(|e| DeliveryError::Database {
        reason: e.to_string(),
    })?;

    // Delete consumed or expired welcomes
    core::execute(
        "DELETE FROM haex_local_delivery_welcomes_no_sync WHERE space_id = ?1 AND (consumed = 1 OR created_at < datetime('now', ?2))".to_string(),
        vec![json!(space_id), json!(format!("-{welcome_ttl_days} days"))],
        db,
    )
    .map_err(|e| DeliveryError::Database {
        reason: e.to_string(),
    })?;

    // Delete expired pending commits
    core::execute(
        "DELETE FROM haex_local_delivery_pending_commits_no_sync WHERE space_id = ?1 AND created_at < datetime('now', ?2)".to_string(),
        vec![json!(space_id), json!(format!("-{pending_commit_ttl_hours} hours"))],
        db,
    )
    .map_err(|e| DeliveryError::Database {
        reason: e.to_string(),
    })?;

    Ok(CleanupStats::default())
}
