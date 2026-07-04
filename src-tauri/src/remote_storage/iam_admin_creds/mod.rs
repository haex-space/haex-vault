//! IAM-admin credential storage backed by the password-manager tables.
//!
//! When a user shares an S3 bucket via a space, the vault needs to keep the
//! long-lived IAM-admin credentials (access key id + secret) somewhere the
//! backend can reach programmatically. Rather than introducing a dedicated
//! secret store, we reuse the already-encrypted password-manager tables
//! (`haex_passwords_item_details` + `haex_passwords_item_key_values`).
//!
//! Layout of a single cred:
//! - `haex_passwords_item_details.title    = "iam-admin:<storage_id>"`
//! - `haex_passwords_item_details.username = <access_key_id>`
//! - `haex_passwords_item_details.password = <secret_access_key>`
//! - `haex_passwords_item_key_values` row with `key = "provider_type"`,
//!   `value ∈ {"aws","wasabi","minio"}`
//!
//! All reads/writes go through the CRDT helpers so the entries sync to the
//! owner's other devices the same way manual password entries do.
//!
//! See the design doc `docs/plans/2026-07-04-s3-bucket-sharing-via-spaces-design.md`
//! §3 (Data-Model — "IAM-Admin-Cred im Passwort-Manager").

use crate::database::core::{execute_with_crdt, select_with_crdt};
use crate::database::error::DatabaseError;
use crate::database::row::get_string;
use crate::database::DbConnection;
use serde_json::Value as JsonValue;
use std::sync::MutexGuard;

/// Materialised IAM-admin credential as stored/loaded from the vault.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IamAdminCred {
    /// The IAM access-key id.
    pub access_key_id: String,
    /// The IAM secret-access-key. Handle with care — do not log or expose
    /// through public commands.
    pub secret_access_key: String,
    /// Provider tag stored in the key-values junction. Expected values:
    /// `"aws" | "wasabi" | "minio"`.
    pub provider_type: String,
}

/// Password-manager title used to identify an IAM-admin cred entry.
/// Deterministic on `storage_id` so we can look up + delete without needing
/// to remember the row `id` separately.
#[inline]
pub fn cred_title_for(storage_id: &str) -> String {
    format!("iam-admin:{}", storage_id)
}

const PROVIDER_TYPE_KEY: &str = "provider_type";

/// Load the IAM-admin credential for a given storage backend.
///
/// Returns `Ok(None)` when no entry exists for `storage_id`.
pub fn load(db: &DbConnection, storage_id: &str) -> Result<Option<IamAdminCred>, DatabaseError> {
    let title = cred_title_for(storage_id);

    // Pull the item's id + username (access_key_id) + password (secret) plus
    // the provider_type from the key-values junction in one round-trip.
    // LEFT JOIN so a missing key-values row still surfaces the item (we
    // decide separately below whether that's a broken entry to skip).
    let sql = "SELECT d.id, d.username, d.password, kv.value \
               FROM haex_passwords_item_details d \
               LEFT JOIN haex_passwords_item_key_values kv \
                 ON kv.item_id = d.id AND kv.key = ?2 \
               WHERE d.title = ?1"
        .to_string();

    let rows = select_with_crdt(
        sql,
        vec![
            JsonValue::String(title),
            JsonValue::String(PROVIDER_TYPE_KEY.to_string()),
        ],
        db,
    )?;

    let Some(row) = rows.first() else {
        return Ok(None);
    };

    let access_key_id = get_string(row, 1);
    let secret_access_key = get_string(row, 2);
    let provider_type = get_string(row, 3);

    Ok(Some(IamAdminCred {
        access_key_id,
        secret_access_key,
        provider_type,
    }))
}

/// Persist a new IAM-admin credential for `storage_id`.
///
/// Assumes no prior entry exists for this storage id. Callers that need
/// upsert semantics should invoke [`delete_by_storage`] first.
pub fn store(
    db: &DbConnection,
    hlc: &MutexGuard<crate::crdt::hlc::HlcService>,
    storage_id: &str,
    cred: &IamAdminCred,
) -> Result<(), DatabaseError> {
    let title = cred_title_for(storage_id);
    let item_id = uuid::Uuid::new_v4().to_string();

    // Insert the details row. Only the columns we actually populate are
    // named — everything else in the schema is either nullable or defaults.
    let insert_details = "INSERT INTO haex_passwords_item_details \
                          (id, title, username, password) \
                          VALUES (?1, ?2, ?3, ?4)"
        .to_string();
    execute_with_crdt(
        insert_details,
        vec![
            JsonValue::String(item_id.clone()),
            JsonValue::String(title),
            JsonValue::String(cred.access_key_id.clone()),
            JsonValue::String(cred.secret_access_key.clone()),
        ],
        db,
        hlc,
    )?;

    // Insert the provider_type key-value row linked via FK.
    let kv_id = uuid::Uuid::new_v4().to_string();
    let insert_kv = "INSERT INTO haex_passwords_item_key_values \
                     (id, item_id, key, value) \
                     VALUES (?1, ?2, ?3, ?4)"
        .to_string();
    execute_with_crdt(
        insert_kv,
        vec![
            JsonValue::String(kv_id),
            JsonValue::String(item_id),
            JsonValue::String(PROVIDER_TYPE_KEY.to_string()),
            JsonValue::String(cred.provider_type.clone()),
        ],
        db,
        hlc,
    )?;

    Ok(())
}

/// Remove the IAM-admin credential for `storage_id`, including its
/// key-values rows.
///
/// The schema declares `ON DELETE CASCADE` on the key-values FK, but the
/// CRDT delete-log layer needs both parents and children explicitly deleted
/// so remote devices actually replay both tombstones. We therefore issue
/// the key-values DELETE first, then the details DELETE.
pub fn delete_by_storage(
    db: &DbConnection,
    hlc: &MutexGuard<crate::crdt::hlc::HlcService>,
    storage_id: &str,
) -> Result<(), DatabaseError> {
    let title = cred_title_for(storage_id);

    let delete_kvs = "DELETE FROM haex_passwords_item_key_values \
                      WHERE item_id IN ( \
                          SELECT id FROM haex_passwords_item_details \
                          WHERE title = ?1 \
                      )"
    .to_string();
    execute_with_crdt(delete_kvs, vec![JsonValue::String(title.clone())], db, hlc)?;

    let delete_item = "DELETE FROM haex_passwords_item_details WHERE title = ?1".to_string();
    execute_with_crdt(delete_item, vec![JsonValue::String(title)], db, hlc)?;

    Ok(())
}

#[cfg(test)]
mod tests;
