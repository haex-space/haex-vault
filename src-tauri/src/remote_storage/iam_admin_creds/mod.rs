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

use crate::database::core::{select_with_crdt, with_connection};
use crate::database::error::DatabaseError;
use crate::database::row::get_string;
use crate::database::DbConnection;
use crate::extension::database::executor::SqlExecutor;
use serde_json::Value as JsonValue;
use std::sync::MutexGuard;

/// Materialised IAM-admin credential as stored/loaded from the vault.
///
/// Custom `Debug` impl redacts both key fields to prevent accidental leaks
/// via `tracing::debug!`, `dbg!`, or panic messages.
#[derive(Clone, PartialEq, Eq)]
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

impl std::fmt::Debug for IamAdminCred {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("IamAdminCred")
            .field("access_key_id", &"<redacted>")
            .field("secret_access_key", &"<redacted>")
            .field("provider_type", &self.provider_type)
            .finish()
    }
}

/// Password-manager title used to identify an IAM-admin cred entry.
/// Deterministic on `storage_id` so we can look up + delete without needing
/// to remember the row `id` separately.
///
/// The `"iam-admin:"` prefix is reserved by the vault; user-created password
/// entries that collide would be misread by [`load`]. Enforced by convention;
/// consider filtering the prefix from password-manager UI list views if this
/// becomes user-visible.
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
    // INNER JOIN so an item without its provider_type key-value row is
    // treated as a broken entry (yields `Ok(None)`). A silent
    // `provider_type = ""` fallback would surface downstream as an opaque
    // IAM signing failure — much harder to diagnose than "cred not found".
    let sql = "SELECT d.id, d.username, d.password, kv.value \
               FROM haex_passwords_item_details d \
               INNER JOIN haex_passwords_item_key_values kv \
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
///
/// **Atomicity:** both INSERTs (`item_details` + `item_key_values`) run inside
/// a single SQLite transaction. If the second INSERT fails, the first is
/// rolled back, so no orphan `item_details` row is left behind. Both writes
/// share one HLC timestamp via the transaction-scoped `current_hlc()` UDF,
/// mirroring `execute_with_crdt`'s single-statement invariant.
pub fn store(
    db: &DbConnection,
    hlc: &MutexGuard<crate::crdt::hlc::HlcService>,
    storage_id: &str,
    cred: &IamAdminCred,
) -> Result<(), DatabaseError> {
    let title = cred_title_for(storage_id);
    let item_id = uuid::Uuid::new_v4().to_string();
    let kv_id = uuid::Uuid::new_v4().to_string();

    // Insert the details row. Only the columns we actually populate are
    // named — everything else in the schema is either nullable or defaults.
    let insert_details = "INSERT INTO haex_passwords_item_details \
                          (id, title, username, password) \
                          VALUES (?1, ?2, ?3, ?4)";
    // Insert the provider_type key-value row linked via FK.
    let insert_kv = "INSERT INTO haex_passwords_item_key_values \
                     (id, item_id, key, value) \
                     VALUES (?1, ?2, ?3, ?4)";

    with_connection(db, |conn| {
        let tx = conn.transaction().map_err(DatabaseError::from)?;

        SqlExecutor::execute_internal(
            &tx,
            hlc,
            insert_details,
            &[
                JsonValue::String(item_id.clone()),
                JsonValue::String(title.clone()),
                JsonValue::String(cred.access_key_id.clone()),
                JsonValue::String(cred.secret_access_key.clone()),
            ],
        )?;

        SqlExecutor::execute_internal(
            &tx,
            hlc,
            insert_kv,
            &[
                JsonValue::String(kv_id.clone()),
                JsonValue::String(item_id.clone()),
                JsonValue::String(PROVIDER_TYPE_KEY.to_string()),
                JsonValue::String(cred.provider_type.clone()),
            ],
        )?;

        tx.commit().map_err(DatabaseError::from)?;
        Ok(())
    })
}

/// Remove the IAM-admin credential for `storage_id`, including its
/// key-values rows.
///
/// The production schema (`haex_passwords_item_key_values.item_id`) declares
/// `ON DELETE CASCADE`, but the CRDT delete-log layer needs both parent and
/// child rows explicitly deleted so remote devices actually replay both
/// tombstones. We therefore issue the key-values DELETE first, then the
/// details DELETE.
///
/// **Atomicity:** both DELETEs run inside a single SQLite transaction. If the
/// second DELETE fails, the first is rolled back so we do not leave the
/// key-values orphaned (which would then fail on FK re-insert during retry).
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
                      )";
    let delete_item = "DELETE FROM haex_passwords_item_details WHERE title = ?1";

    with_connection(db, |conn| {
        let tx = conn.transaction().map_err(DatabaseError::from)?;

        SqlExecutor::execute_internal(&tx, hlc, delete_kvs, &[JsonValue::String(title.clone())])?;

        SqlExecutor::execute_internal(&tx, hlc, delete_item, &[JsonValue::String(title.clone())])?;

        tx.commit().map_err(DatabaseError::from)?;
        Ok(())
    })
}

#[cfg(test)]
mod tests;
