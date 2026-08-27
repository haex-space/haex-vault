//! Phase 4 Round F1 — typed row helpers for `haex_s3_shared_access`.
//!
//! One row per (space, backend, member) triple. The row carries an
//! AEAD-sealed `ScopedCred` payload (sealing scheme finalised in Round
//! F3 alongside the IAM adapter wire-up) plus the epoch it was sealed
//! under. Any current member can decrypt via the space epoch key
//! flowing through `haex_mls_sync_keys`; a member kicked from the
//! space keeps historical epoch keys but future rows are minted under
//! a new epoch they no longer have — the encryption layer half of the
//! revocation story (the other half is the IAM-provider-side
//! rotation).
//!
//! Everything goes through `execute_with_crdt` / `select_with_crdt` —
//! chokepoint for CRDT bookkeeping (see CLAUDE.md).
//!
//! Round F1 lands the schema and the row helpers. Owner-side minting +
//! per-member-client-side unwrap is Round F3.

use crate::crdt::column_sig::key_cache::SpaceKeyCache;
use crate::crdt::hlc::HlcService;
use crate::database::{
    core::{execute_with_crdt, select_with_crdt},
    error::DatabaseError,
    DbConnection,
};
use crate::table_names::{
    COL_S3_SHARED_ACCESS_BACKEND_ID, COL_S3_SHARED_ACCESS_CREATED_AT,
    COL_S3_SHARED_ACCESS_ENCRYPTED_CRED, COL_S3_SHARED_ACCESS_EPOCH,
    COL_S3_SHARED_ACCESS_EXPIRES_AT, COL_S3_SHARED_ACCESS_ID, COL_S3_SHARED_ACCESS_MEMBER_DID,
    COL_S3_SHARED_ACCESS_SPACE_ID, TABLE_S3_SHARED_ACCESS,
};
use serde_json::Value as JsonValue;
use std::sync::MutexGuard;

#[cfg(test)]
mod tests;

/// One decoded row of `haex_s3_shared_access`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SharedAccessRow {
    pub id: String,
    pub space_id: String,
    pub backend_id: String,
    pub member_did: String,
    /// AEAD-sealed `ScopedCred`. Base64 in-column, opaque bytes at rest.
    pub encrypted_cred: String,
    pub epoch: i64,
    pub expires_at: Option<String>,
    pub created_at: String,
}

/// Errors surfaced from shared-access row operations.
#[derive(Debug, thiserror::Error)]
pub enum SharedAccessError {
    #[error(transparent)]
    Db(#[from] DatabaseError),
    #[error("row shape mismatch: {reason}")]
    RowShape { reason: String },
}

/// Insert or overwrite a shared-access row for `(space_id, backend_id,
/// member_did)`. Overwrite semantics because minting a fresh
/// `ScopedCred` for the same member is the normal re-provisioning
/// path — either the epoch rolled and the payload has to be resealed,
/// or the underlying IAM credentials were rotated.
///
/// `id` is stamped explicitly (usually a UUID minted by the caller);
/// keeping the id stable across upserts lets receivers treat the row as
/// an update, not a distinct grant.
pub fn upsert_shared_access(
    db: &DbConnection,
    hlc: &MutexGuard<HlcService>,
    column_sig_key_cache: &SpaceKeyCache,
    id: &str,
    space_id: &str,
    backend_id: &str,
    member_did: &str,
    encrypted_cred: &str,
    epoch: u64,
    expires_at: Option<&str>,
) -> Result<(), SharedAccessError> {
    let sql = format!(
        "INSERT INTO {TABLE_S3_SHARED_ACCESS} \
         ({COL_S3_SHARED_ACCESS_ID}, {COL_S3_SHARED_ACCESS_SPACE_ID}, \
          {COL_S3_SHARED_ACCESS_BACKEND_ID}, {COL_S3_SHARED_ACCESS_MEMBER_DID}, \
          {COL_S3_SHARED_ACCESS_ENCRYPTED_CRED}, {COL_S3_SHARED_ACCESS_EPOCH}, \
          {COL_S3_SHARED_ACCESS_EXPIRES_AT}) \
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7) \
         ON CONFLICT ({COL_S3_SHARED_ACCESS_SPACE_ID}, {COL_S3_SHARED_ACCESS_BACKEND_ID}, \
                      {COL_S3_SHARED_ACCESS_MEMBER_DID}) \
         DO UPDATE SET \
           {COL_S3_SHARED_ACCESS_ENCRYPTED_CRED} = excluded.{COL_S3_SHARED_ACCESS_ENCRYPTED_CRED}, \
           {COL_S3_SHARED_ACCESS_EPOCH} = excluded.{COL_S3_SHARED_ACCESS_EPOCH}, \
           {COL_S3_SHARED_ACCESS_EXPIRES_AT} = excluded.{COL_S3_SHARED_ACCESS_EXPIRES_AT}"
    );
    let expires_json = match expires_at {
        Some(v) => JsonValue::String(v.to_string()),
        None => JsonValue::Null,
    };
    execute_with_crdt(
        sql,
        vec![
            JsonValue::String(id.to_string()),
            JsonValue::String(space_id.to_string()),
            JsonValue::String(backend_id.to_string()),
            JsonValue::String(member_did.to_string()),
            JsonValue::String(encrypted_cred.to_string()),
            JsonValue::from(epoch),
            expires_json,
        ],
        db,
        hlc,
        column_sig_key_cache,
    )?;
    Ok(())
}

/// Delete a shared-access row for `(space_id, backend_id, member_did)`.
/// Used on member kick — combined with an IAM-provider-side rotation,
/// this is the full-stack revocation step.
pub fn delete_shared_access(
    db: &DbConnection,
    hlc: &MutexGuard<HlcService>,
    column_sig_key_cache: &SpaceKeyCache,
    space_id: &str,
    backend_id: &str,
    member_did: &str,
) -> Result<(), SharedAccessError> {
    let sql = format!(
        "DELETE FROM {TABLE_S3_SHARED_ACCESS} \
         WHERE {COL_S3_SHARED_ACCESS_SPACE_ID} = ?1 \
           AND {COL_S3_SHARED_ACCESS_BACKEND_ID} = ?2 \
           AND {COL_S3_SHARED_ACCESS_MEMBER_DID} = ?3"
    );
    execute_with_crdt(
        sql,
        vec![
            JsonValue::String(space_id.to_string()),
            JsonValue::String(backend_id.to_string()),
            JsonValue::String(member_did.to_string()),
        ],
        db,
        hlc,
        column_sig_key_cache,
    )?;
    Ok(())
}

/// Look up the shared-access row for a specific `(space_id, backend_id,
/// member_did)`. Returns `Ok(None)` if no row exists.
pub fn find_shared_access(
    db: &DbConnection,
    space_id: &str,
    backend_id: &str,
    member_did: &str,
) -> Result<Option<SharedAccessRow>, SharedAccessError> {
    let sql = format!(
        "SELECT {COL_S3_SHARED_ACCESS_ID}, {COL_S3_SHARED_ACCESS_SPACE_ID}, \
                {COL_S3_SHARED_ACCESS_BACKEND_ID}, {COL_S3_SHARED_ACCESS_MEMBER_DID}, \
                {COL_S3_SHARED_ACCESS_ENCRYPTED_CRED}, {COL_S3_SHARED_ACCESS_EPOCH}, \
                {COL_S3_SHARED_ACCESS_EXPIRES_AT}, {COL_S3_SHARED_ACCESS_CREATED_AT} \
         FROM {TABLE_S3_SHARED_ACCESS} \
         WHERE {COL_S3_SHARED_ACCESS_SPACE_ID} = ?1 \
           AND {COL_S3_SHARED_ACCESS_BACKEND_ID} = ?2 \
           AND {COL_S3_SHARED_ACCESS_MEMBER_DID} = ?3 \
         LIMIT 1"
    );
    let rows = select_with_crdt(
        sql,
        vec![
            JsonValue::String(space_id.to_string()),
            JsonValue::String(backend_id.to_string()),
            JsonValue::String(member_did.to_string()),
        ],
        db,
    )?;
    rows.into_iter()
        .next()
        .map(row_to_shared_access)
        .transpose()
}

/// List every shared-access row for a `space_id`. Used by the owner to
/// enumerate members when rewrapping under a new epoch.
pub fn list_shared_access_for_space(
    db: &DbConnection,
    space_id: &str,
) -> Result<Vec<SharedAccessRow>, SharedAccessError> {
    let sql = format!(
        "SELECT {COL_S3_SHARED_ACCESS_ID}, {COL_S3_SHARED_ACCESS_SPACE_ID}, \
                {COL_S3_SHARED_ACCESS_BACKEND_ID}, {COL_S3_SHARED_ACCESS_MEMBER_DID}, \
                {COL_S3_SHARED_ACCESS_ENCRYPTED_CRED}, {COL_S3_SHARED_ACCESS_EPOCH}, \
                {COL_S3_SHARED_ACCESS_EXPIRES_AT}, {COL_S3_SHARED_ACCESS_CREATED_AT} \
         FROM {TABLE_S3_SHARED_ACCESS} \
         WHERE {COL_S3_SHARED_ACCESS_SPACE_ID} = ?1 \
         ORDER BY {COL_S3_SHARED_ACCESS_MEMBER_DID} ASC, {COL_S3_SHARED_ACCESS_BACKEND_ID} ASC"
    );
    let rows = select_with_crdt(sql, vec![JsonValue::String(space_id.to_string())], db)?;
    rows.into_iter().map(row_to_shared_access).collect()
}

fn row_to_shared_access(row: Vec<JsonValue>) -> Result<SharedAccessRow, SharedAccessError> {
    if row.len() != 8 {
        return Err(SharedAccessError::RowShape {
            reason: format!("expected 8 columns, got {}", row.len()),
        });
    }
    Ok(SharedAccessRow {
        id: take_string(&row[0], "id")?,
        space_id: take_string(&row[1], "space_id")?,
        backend_id: take_string(&row[2], "backend_id")?,
        member_did: take_string(&row[3], "member_did")?,
        encrypted_cred: take_string(&row[4], "encrypted_cred")?,
        epoch: row[5].as_i64().ok_or_else(|| SharedAccessError::RowShape {
            reason: "epoch column is not an integer".into(),
        })?,
        expires_at: match &row[6] {
            JsonValue::Null => None,
            v => {
                Some(
                    v.as_str()
                        .map(str::to_string)
                        .ok_or_else(|| SharedAccessError::RowShape {
                            reason: "expires_at column is neither NULL nor a string".into(),
                        })?,
                )
            }
        },
        created_at: take_string(&row[7], "created_at")?,
    })
}

fn take_string(v: &JsonValue, col: &str) -> Result<String, SharedAccessError> {
    v.as_str()
        .map(str::to_string)
        .ok_or_else(|| SharedAccessError::RowShape {
            reason: format!("column `{col}` is not a string"),
        })
}
