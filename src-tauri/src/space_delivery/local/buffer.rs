//! MLS message buffer operations for the leader.
//!
//! All buffer tables are `_no_sync` (not CRDT-synced), so we use `core::execute` / `core::select`.
//! BLOBs are returned as Base64-encoded strings by the JSON-based core functions.

use base64::Engine;

use crate::database::core;
use crate::database::DbConnection;
use uuid::Uuid;

use super::error::DeliveryError;

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
    let blob_b64 = base64::engine::general_purpose::STANDARD.encode(message_blob);
    let rows = core::execute(
        "INSERT INTO haex_local_delivery_messages_no_sync (space_id, sender_did, message_type, message_blob) VALUES (?1, ?2, ?3, ?4) RETURNING id".to_string(),
        vec![
            serde_json::Value::String(space_id.to_string()),
            serde_json::Value::String(sender_did.to_string()),
            serde_json::Value::String(message_type.to_string()),
            serde_json::Value::String(blob_b64),
        ],
        db,
    ).map_err(map_db)?;

    rows.first()
        .and_then(|r| r.first()?.as_i64())
        .ok_or_else(|| DeliveryError::Database {
            reason: "No ID returned from INSERT".to_string(),
        })
}

/// Fetch MLS messages after a given ID.
pub fn fetch_messages(
    db: &DbConnection,
    space_id: &str,
    after_id: Option<i64>,
) -> Result<Vec<(i64, String, String, Vec<u8>, String)>, DeliveryError> {
    let after = after_id.unwrap_or(0);
    let rows = core::select(
        "SELECT id, sender_did, message_type, message_blob, created_at \
         FROM haex_local_delivery_messages_no_sync \
         WHERE space_id = ?1 AND id > ?2 \
         ORDER BY id ASC".to_string(),
        vec![
            serde_json::Value::String(space_id.to_string()),
            serde_json::Value::Number(serde_json::Number::from(after)),
        ],
        db,
    ).map_err(map_db)?;

    let mut result = Vec::new();
    for row in rows {
        let id = row.get(0).and_then(|v| v.as_i64()).unwrap_or(0);
        let sender_did = row.get(1).and_then(|v| v.as_str()).unwrap_or_default().to_string();
        let msg_type = row.get(2).and_then(|v| v.as_str()).unwrap_or_default().to_string();
        let blob_b64 = row.get(3).and_then(|v| v.as_str()).unwrap_or_default();
        let blob = base64::engine::general_purpose::STANDARD.decode(blob_b64)
            .unwrap_or_default();
        let created_at = row.get(4).and_then(|v| v.as_str()).unwrap_or_default().to_string();
        result.push((id, sender_did, msg_type, blob, created_at));
    }
    Ok(result)
}

/// Store a key package for a target DID. Returns the generated UUID.
pub fn store_key_package(
    db: &DbConnection,
    space_id: &str,
    target_did: &str,
    package_blob: &[u8],
) -> Result<String, DeliveryError> {
    let id = Uuid::new_v4().to_string();
    let blob_b64 = base64::engine::general_purpose::STANDARD.encode(package_blob);
    core::execute(
        "INSERT INTO haex_local_delivery_key_packages_no_sync (id, space_id, target_did, package_blob) VALUES (?1, ?2, ?3, ?4)".to_string(),
        vec![
            serde_json::Value::String(id.clone()),
            serde_json::Value::String(space_id.to_string()),
            serde_json::Value::String(target_did.to_string()),
            serde_json::Value::String(blob_b64),
        ],
        db,
    ).map_err(map_db)?;
    Ok(id)
}

/// Fetch and consume (delete) one key package for a target DID.
/// Single-use per MLS spec: SELECT one, then DELETE it.
pub fn consume_key_package(
    db: &DbConnection,
    space_id: &str,
    target_did: &str,
) -> Result<Option<Vec<u8>>, DeliveryError> {
    let rows = core::select(
        "SELECT id, package_blob FROM haex_local_delivery_key_packages_no_sync \
         WHERE space_id = ?1 AND target_did = ?2 \
         ORDER BY created_at ASC LIMIT 1".to_string(),
        vec![
            serde_json::Value::String(space_id.to_string()),
            serde_json::Value::String(target_did.to_string()),
        ],
        db,
    ).map_err(map_db)?;

    let Some(row) = rows.first() else { return Ok(None) };
    let id = row.get(0).and_then(|v| v.as_str()).unwrap_or_default().to_string();
    let blob_b64 = row.get(1).and_then(|v| v.as_str()).unwrap_or_default();
    let blob = base64::engine::general_purpose::STANDARD.decode(blob_b64)
        .unwrap_or_default();

    // Delete after consuming
    core::execute(
        "DELETE FROM haex_local_delivery_key_packages_no_sync WHERE id = ?1".to_string(),
        vec![serde_json::Value::String(id)],
        db,
    ).map_err(map_db)?;

    Ok(Some(blob))
}

/// Store a welcome message for a recipient. Returns the generated UUID.
pub fn store_welcome(
    db: &DbConnection,
    space_id: &str,
    recipient_did: &str,
    welcome_blob: &[u8],
) -> Result<String, DeliveryError> {
    let id = Uuid::new_v4().to_string();
    let blob_b64 = base64::engine::general_purpose::STANDARD.encode(welcome_blob);
    core::execute(
        "INSERT INTO haex_local_delivery_welcomes_no_sync (id, space_id, recipient_did, welcome_blob) VALUES (?1, ?2, ?3, ?4)".to_string(),
        vec![
            serde_json::Value::String(id.clone()),
            serde_json::Value::String(space_id.to_string()),
            serde_json::Value::String(recipient_did.to_string()),
            serde_json::Value::String(blob_b64),
        ],
        db,
    ).map_err(map_db)?;
    Ok(id)
}

/// Fetch unconsumed welcomes for a recipient DID without marking them consumed.
/// Call `mark_welcome_consumed` after successful processing of each welcome.
pub fn fetch_welcomes(
    db: &DbConnection,
    space_id: &str,
    recipient_did: &str,
) -> Result<Vec<(String, Vec<u8>)>, DeliveryError> {
    let rows = core::select(
        "SELECT id, welcome_blob FROM haex_local_delivery_welcomes_no_sync \
         WHERE space_id = ?1 AND recipient_did = ?2 AND consumed = 0 \
         ORDER BY created_at ASC".to_string(),
        vec![
            serde_json::Value::String(space_id.to_string()),
            serde_json::Value::String(recipient_did.to_string()),
        ],
        db,
    ).map_err(map_db)?;

    let mut results = Vec::new();
    for row in &rows {
        let id = row.get(0).and_then(|v| v.as_str()).unwrap_or_default().to_string();
        let blob_b64 = row.get(1).and_then(|v| v.as_str()).unwrap_or_default();
        let blob = base64::engine::general_purpose::STANDARD.decode(blob_b64)
            .unwrap_or_default();
        results.push((id, blob));
    }

    Ok(results)
}

/// Mark a single welcome as consumed after successful processing.
pub fn mark_welcome_consumed(db: &DbConnection, welcome_id: &str) -> Result<(), DeliveryError> {
    core::execute(
        "UPDATE haex_local_delivery_welcomes_no_sync SET consumed = 1 WHERE id = ?1".to_string(),
        vec![serde_json::Value::String(welcome_id.to_string())],
        db,
    ).map_err(map_db)?;
    Ok(())
}

// ---------------------------------------------------------------------------
// Pending-commit ACK tracking
// ---------------------------------------------------------------------------

/// Store a pending commit entry tracking which members must ACK. Returns the generated UUID.
pub fn store_pending_commit(
    db: &DbConnection,
    space_id: &str,
    message_id: i64,
    expected_dids: &[String],
) -> Result<String, DeliveryError> {
    let id = Uuid::new_v4().to_string();
    let expected_json = serde_json::to_string(expected_dids).unwrap_or_else(|_| "[]".to_string());
    core::execute(
        "INSERT INTO haex_local_delivery_pending_commits_no_sync \
         (id, space_id, message_id, expected_dids, acked_dids) \
         VALUES (?1, ?2, ?3, ?4, '[]')"
            .to_string(),
        vec![
            serde_json::Value::String(id.clone()),
            serde_json::Value::String(space_id.to_string()),
            serde_json::Value::Number(serde_json::Number::from(message_id)),
            serde_json::Value::String(expected_json),
        ],
        db,
    )
    .map_err(map_db)?;
    Ok(id)
}

/// Record ACKs from `member_did` for the given `message_ids`.
/// Returns the list of message_ids that are now fully ACKed (all expected DIDs have ACKed).
pub fn ack_commits(
    db: &DbConnection,
    space_id: &str,
    member_did: &str,
    message_ids: &[i64],
) -> Result<Vec<i64>, DeliveryError> {
    let mut fully_acked = Vec::new();

    for &msg_id in message_ids {
        let rows = core::select(
            "SELECT expected_dids, acked_dids FROM haex_local_delivery_pending_commits_no_sync \
             WHERE space_id = ?1 AND message_id = ?2"
                .to_string(),
            vec![
                serde_json::Value::String(space_id.to_string()),
                serde_json::Value::Number(serde_json::Number::from(msg_id)),
            ],
            db,
        )
        .map_err(map_db)?;

        let Some(row) = rows.first() else { continue };

        let expected_str = row.get(0).and_then(|v| v.as_str()).unwrap_or("[]");
        let acked_str = row.get(1).and_then(|v| v.as_str()).unwrap_or("[]");

        let expected: Vec<String> =
            serde_json::from_str(expected_str).unwrap_or_default();
        let mut acked: Vec<String> =
            serde_json::from_str(acked_str).unwrap_or_default();

        if !acked.contains(&member_did.to_string()) {
            acked.push(member_did.to_string());
        }

        let acked_json = serde_json::to_string(&acked).unwrap_or_else(|_| "[]".to_string());
        core::execute(
            "UPDATE haex_local_delivery_pending_commits_no_sync \
             SET acked_dids = ?1 WHERE space_id = ?2 AND message_id = ?3"
                .to_string(),
            vec![
                serde_json::Value::String(acked_json),
                serde_json::Value::String(space_id.to_string()),
                serde_json::Value::Number(serde_json::Number::from(msg_id)),
            ],
            db,
        )
        .map_err(map_db)?;

        // Check if fully acked
        if expected.iter().all(|did| acked.contains(did)) {
            fully_acked.push(msg_id);
        }
    }

    Ok(fully_acked)
}

/// Delete pending commit entries and their corresponding messages for fully-ACKed message_ids.
pub fn cleanup_acked_commits(
    db: &DbConnection,
    space_id: &str,
    message_ids: &[i64],
) -> Result<(), DeliveryError> {
    for &msg_id in message_ids {
        core::execute(
            "DELETE FROM haex_local_delivery_pending_commits_no_sync \
             WHERE space_id = ?1 AND message_id = ?2"
                .to_string(),
            vec![
                serde_json::Value::String(space_id.to_string()),
                serde_json::Value::Number(serde_json::Number::from(msg_id)),
            ],
            db,
        )
        .map_err(map_db)?;

        core::execute(
            "DELETE FROM haex_local_delivery_messages_no_sync \
             WHERE space_id = ?1 AND id = ?2"
                .to_string(),
            vec![
                serde_json::Value::String(space_id.to_string()),
                serde_json::Value::Number(serde_json::Number::from(msg_id)),
            ],
            db,
        )
        .map_err(map_db)?;
    }
    Ok(())
}

/// Get message IDs where `member_did` is expected but has not yet ACKed.
pub fn get_unacked_message_ids_for_member(
    db: &DbConnection,
    space_id: &str,
    member_did: &str,
) -> Result<Vec<i64>, DeliveryError> {
    let rows = core::select(
        "SELECT message_id, expected_dids, acked_dids \
         FROM haex_local_delivery_pending_commits_no_sync \
         WHERE space_id = ?1"
            .to_string(),
        vec![serde_json::Value::String(space_id.to_string())],
        db,
    )
    .map_err(map_db)?;

    let mut result = Vec::new();
    for row in rows {
        let msg_id = row.get(0).and_then(|v| v.as_i64()).unwrap_or(0);
        let expected_str = row.get(1).and_then(|v| v.as_str()).unwrap_or("[]");
        let acked_str = row.get(2).and_then(|v| v.as_str()).unwrap_or("[]");

        let expected: Vec<String> =
            serde_json::from_str(expected_str).unwrap_or_default();
        let acked: Vec<String> =
            serde_json::from_str(acked_str).unwrap_or_default();

        if expected.contains(&member_did.to_string()) && !acked.contains(&member_did.to_string()) {
            result.push(msg_id);
        }
    }

    Ok(result)
}

/// Clear all buffer tables for a space (called when leadership ends).
pub fn clear_buffers(db: &DbConnection, space_id: &str) -> Result<(), DeliveryError> {
    for table in &[
        "haex_local_delivery_messages_no_sync",
        "haex_local_delivery_key_packages_no_sync",
        "haex_local_delivery_welcomes_no_sync",
        "haex_local_delivery_pending_commits_no_sync",
    ] {
        core::execute(
            format!("DELETE FROM {table} WHERE space_id = ?1"),
            vec![serde_json::Value::String(space_id.to_string())],
            db,
        ).map_err(map_db)?;
    }
    Ok(())
}
