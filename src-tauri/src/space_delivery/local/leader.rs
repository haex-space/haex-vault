//! Leader-side logic: buffering MLS messages, key packages, welcomes, and pending commits.
//!
//! These operations use `with_connection` directly (rather than `core::select`/`core::execute`)
//! because the buffer tables contain BLOB columns. The JSON-based core functions cannot
//! round-trip blob data faithfully (blobs become Null on read).

use crate::database::core::with_connection;
use crate::database::DbConnection;
use rusqlite::OptionalExtension;
use uuid::Uuid;

use super::error::DeliveryError;

/// Map a `DatabaseError` into `DeliveryError::Database`.
fn map_db(e: crate::database::error::DatabaseError) -> DeliveryError {
    DeliveryError::Database {
        reason: e.to_string(),
    }
}

/// Store an MLS message in the leader buffer. Returns the auto-incremented ID.
pub fn store_message(
    db: &DbConnection,
    space_id: &str,
    sender_did: &str,
    message_type: &str,
    message_blob: &[u8],
) -> Result<i64, DeliveryError> {
    with_connection(db, |conn| {
        conn.execute(
            "INSERT INTO haex_local_delivery_messages_no_sync (space_id, sender_did, message_type, message_blob) VALUES (?1, ?2, ?3, ?4)",
            rusqlite::params![space_id, sender_did, message_type, message_blob],
        ).map_err(|e| crate::database::error::DatabaseError::DatabaseError {
            reason: e.to_string(),
        })?;
        let id = conn.last_insert_rowid();
        Ok(id)
    })
    .map_err(map_db)
}

/// Fetch MLS messages after a given ID.
/// Returns tuples of (id, sender_did, message_type, message_blob, created_at).
pub fn fetch_messages(
    db: &DbConnection,
    space_id: &str,
    after_id: Option<i64>,
) -> Result<Vec<(i64, String, String, Vec<u8>, String)>, DeliveryError> {
    with_connection(db, |conn| {
        let after = after_id.unwrap_or(0);
        let mut stmt = conn
            .prepare(
                "SELECT id, sender_did, message_type, message_blob, created_at \
                 FROM haex_local_delivery_messages_no_sync \
                 WHERE space_id = ?1 AND id > ?2 \
                 ORDER BY id ASC",
            )
            .map_err(|e| crate::database::error::DatabaseError::DatabaseError {
                reason: e.to_string(),
            })?;

        let rows = stmt
            .query_map(rusqlite::params![space_id, after], |row| {
                Ok((
                    row.get::<_, i64>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, Vec<u8>>(3)?,
                    row.get::<_, String>(4)?,
                ))
            })
            .map_err(|e| crate::database::error::DatabaseError::DatabaseError {
                reason: e.to_string(),
            })?;

        let mut result = Vec::new();
        for row in rows {
            result.push(row.map_err(|e| crate::database::error::DatabaseError::DatabaseError {
                reason: e.to_string(),
            })?);
        }
        Ok(result)
    })
    .map_err(map_db)
}

/// Store a key package for a target DID. Returns the generated UUID.
pub fn store_key_package(
    db: &DbConnection,
    space_id: &str,
    target_did: &str,
    package_blob: &[u8],
) -> Result<String, DeliveryError> {
    let id = Uuid::new_v4().to_string();
    with_connection(db, |conn| {
        conn.execute(
            "INSERT INTO haex_local_delivery_key_packages_no_sync (id, space_id, target_did, package_blob) VALUES (?1, ?2, ?3, ?4)",
            rusqlite::params![id, space_id, target_did, package_blob],
        ).map_err(|e| crate::database::error::DatabaseError::DatabaseError {
            reason: e.to_string(),
        })?;
        Ok(id)
    })
    .map_err(map_db)
}

/// Fetch and consume (delete) one key package for a target DID.
/// Single-use per MLS spec: SELECT one, then DELETE it.
pub fn consume_key_package(
    db: &DbConnection,
    space_id: &str,
    target_did: &str,
) -> Result<Option<Vec<u8>>, DeliveryError> {
    with_connection(db, |conn| {
        // Find the oldest key package for this target
        let result: Option<(String, Vec<u8>)> = conn
            .query_row(
                "SELECT id, package_blob FROM haex_local_delivery_key_packages_no_sync \
                 WHERE space_id = ?1 AND target_did = ?2 \
                 ORDER BY created_at ASC LIMIT 1",
                rusqlite::params![space_id, target_did],
                |row| Ok((row.get::<_, String>(0)?, row.get::<_, Vec<u8>>(1)?)),
            )
            .optional()
            .map_err(|e: rusqlite::Error| crate::database::error::DatabaseError::DatabaseError {
                reason: e.to_string(),
            })?;

        match result {
            Some((id, blob)) => {
                conn.execute(
                    "DELETE FROM haex_local_delivery_key_packages_no_sync WHERE id = ?1",
                    rusqlite::params![id],
                )
                .map_err(|e| crate::database::error::DatabaseError::DatabaseError {
                    reason: e.to_string(),
                })?;
                Ok(Some(blob))
            }
            None => Ok(None),
        }
    })
    .map_err(map_db)
}

/// Store a welcome message for a recipient. Returns the generated UUID.
pub fn store_welcome(
    db: &DbConnection,
    space_id: &str,
    recipient_did: &str,
    welcome_blob: &[u8],
) -> Result<String, DeliveryError> {
    let id = Uuid::new_v4().to_string();
    with_connection(db, |conn| {
        conn.execute(
            "INSERT INTO haex_local_delivery_welcomes_no_sync (id, space_id, recipient_did, welcome_blob) VALUES (?1, ?2, ?3, ?4)",
            rusqlite::params![id, space_id, recipient_did, welcome_blob],
        ).map_err(|e| crate::database::error::DatabaseError::DatabaseError {
            reason: e.to_string(),
        })?;
        Ok(id)
    })
    .map_err(map_db)
}

/// Fetch and mark consumed all welcomes for a recipient DID.
/// Returns the welcome blobs. Marks them as consumed so they are not returned again.
pub fn consume_welcomes(
    db: &DbConnection,
    space_id: &str,
    recipient_did: &str,
) -> Result<Vec<Vec<u8>>, DeliveryError> {
    with_connection(db, |conn| {
        // Fetch all unconsumed welcomes
        let mut stmt = conn
            .prepare(
                "SELECT id, welcome_blob FROM haex_local_delivery_welcomes_no_sync \
                 WHERE space_id = ?1 AND recipient_did = ?2 AND consumed = 0 \
                 ORDER BY created_at ASC",
            )
            .map_err(|e| crate::database::error::DatabaseError::DatabaseError {
                reason: e.to_string(),
            })?;

        let rows = stmt
            .query_map(rusqlite::params![space_id, recipient_did], |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, Vec<u8>>(1)?))
            })
            .map_err(|e| crate::database::error::DatabaseError::DatabaseError {
                reason: e.to_string(),
            })?;

        let mut ids = Vec::new();
        let mut blobs = Vec::new();
        for row in rows {
            let (id, blob) =
                row.map_err(|e| crate::database::error::DatabaseError::DatabaseError {
                    reason: e.to_string(),
                })?;
            ids.push(id);
            blobs.push(blob);
        }

        // Mark all fetched welcomes as consumed
        for id in &ids {
            conn.execute(
                "UPDATE haex_local_delivery_welcomes_no_sync SET consumed = 1 WHERE id = ?1",
                rusqlite::params![id],
            )
            .map_err(|e| crate::database::error::DatabaseError::DatabaseError {
                reason: e.to_string(),
            })?;
        }

        Ok(blobs)
    })
    .map_err(map_db)
}

/// Store a pending commit (for crash recovery). Returns the generated UUID.
pub fn store_pending_commit(
    db: &DbConnection,
    space_id: &str,
    commit_blob: &[u8],
) -> Result<String, DeliveryError> {
    let id = Uuid::new_v4().to_string();
    with_connection(db, |conn| {
        conn.execute(
            "INSERT INTO haex_local_delivery_pending_commits_no_sync (id, space_id, commit_blob) VALUES (?1, ?2, ?3)",
            rusqlite::params![id, space_id, commit_blob],
        ).map_err(|e| crate::database::error::DatabaseError::DatabaseError {
            reason: e.to_string(),
        })?;
        Ok(id)
    })
    .map_err(map_db)
}

/// Clear all buffer tables for a space (called when leadership ends).
pub fn clear_buffers(db: &DbConnection, space_id: &str) -> Result<(), DeliveryError> {
    with_connection(db, |conn| {
        for table in &[
            "haex_local_delivery_messages_no_sync",
            "haex_local_delivery_key_packages_no_sync",
            "haex_local_delivery_welcomes_no_sync",
            "haex_local_delivery_pending_commits_no_sync",
        ] {
            conn.execute(
                &format!("DELETE FROM {table} WHERE space_id = ?1"),
                rusqlite::params![space_id],
            )
            .map_err(|e| crate::database::error::DatabaseError::DatabaseError {
                reason: e.to_string(),
            })?;
        }
        Ok(())
    })
    .map_err(map_db)
}
