//! Phase 4 Round F1 — typed row helpers for `haex_file_grants`.
//!
//! Grant rows record the fact "content object X is shared with space Y".
//! One row per (content_key, space_id) pair; the UNIQUE index on those two
//! columns is the enforcement point. Own-vault-only files (no cross-space
//! sharing) never appear here — their `own/<hex32>.m` sidecar is enough on
//! its own, and adding a stub row would confuse the space-scoped CRDT
//! stream (`SPACE_SCOPED_CRDT_TABLES` treats every row as belonging to its
//! `space_id`).
//!
//! Everything goes through `execute_with_crdt` / `select_with_crdt` — the
//! CRDT chokepoint is what installs the HLC + column signatures that make
//! rows shippable via the space delivery stream. Direct rusqlite writes
//! are a bug, not a shortcut.
//!
//! Round F1 lands the schema and the row helpers. The wire-up
//! (`share_file` / `unshare_file` Tauri commands, sidecar write coupling)
//! is Round F2/F4.

use crate::crdt::column_sig::key_cache::SpaceKeyCache;
use crate::crdt::hlc::HlcService;
use crate::database::{
    core::{execute_with_crdt, select_with_crdt},
    error::DatabaseError,
    DbConnection,
};
use crate::table_names::{
    COL_FILE_GRANTS_CONTENT_KEY, COL_FILE_GRANTS_CREATED_AT, COL_FILE_GRANTS_EPOCH,
    COL_FILE_GRANTS_ID, COL_FILE_GRANTS_SIDECAR_KEY, COL_FILE_GRANTS_SPACE_ID, TABLE_FILE_GRANTS,
};
use serde_json::Value as JsonValue;
use std::sync::MutexGuard;

#[cfg(test)]
mod tests;

/// One row of `haex_file_grants`, decoded.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileGrantRow {
    pub id: String,
    pub content_key: String,
    pub space_id: String,
    pub sidecar_key: String,
    pub epoch: i64,
    pub created_at: String,
}

/// Errors surfaced from grant-row operations. Wraps `DatabaseError` so
/// callers who don't care about the finer distinction can `?`-propagate.
#[derive(Debug, thiserror::Error)]
pub enum GrantError {
    #[error(transparent)]
    Db(#[from] DatabaseError),
    #[error("row shape mismatch: {reason}")]
    RowShape { reason: String },
}

/// Insert a grant row. Idempotent via the UNIQUE(content_key, space_id)
/// index: a repeated insert for the same (content_key, space_id) will be
/// rejected by SQLite, which surfaces here as a `DatabaseError`. Callers
/// wanting upsert semantics should use [`upsert_grant`] instead.
///
/// `id` is expected to be a freshly-generated UUID — Round F2 mints one
/// per share command, but this helper takes it explicitly so callers can
/// deterministically stamp an id in tests.
pub fn insert_grant(
    db: &DbConnection,
    hlc: &MutexGuard<HlcService>,
    column_sig_key_cache: &SpaceKeyCache,
    id: &str,
    content_key: &str,
    space_id: &str,
    sidecar_key: &str,
    epoch: u64,
) -> Result<(), GrantError> {
    let sql = format!(
        "INSERT INTO {TABLE_FILE_GRANTS} \
         ({COL_FILE_GRANTS_ID}, {COL_FILE_GRANTS_CONTENT_KEY}, {COL_FILE_GRANTS_SPACE_ID}, \
          {COL_FILE_GRANTS_SIDECAR_KEY}, {COL_FILE_GRANTS_EPOCH}) \
         VALUES (?1, ?2, ?3, ?4, ?5)"
    );
    execute_with_crdt(
        sql,
        vec![
            JsonValue::String(id.to_string()),
            JsonValue::String(content_key.to_string()),
            JsonValue::String(space_id.to_string()),
            JsonValue::String(sidecar_key.to_string()),
            JsonValue::from(epoch),
        ],
        db,
        hlc,
        column_sig_key_cache,
    )?;
    Ok(())
}

/// Upsert a grant row: insert or overwrite the `sidecar_key`/`epoch`
/// pair for the matching (content_key, space_id). Used when rewrapping a
/// grant under a new epoch — the row's identity (uuid) stays stable so
/// receivers see it as an update, not a new grant.
///
/// Kept separate from `insert_grant` so accidental overwrites are
/// explicit at the call site.
pub fn upsert_grant(
    db: &DbConnection,
    hlc: &MutexGuard<HlcService>,
    column_sig_key_cache: &SpaceKeyCache,
    id: &str,
    content_key: &str,
    space_id: &str,
    sidecar_key: &str,
    epoch: u64,
) -> Result<(), GrantError> {
    let sql = format!(
        "INSERT INTO {TABLE_FILE_GRANTS} \
         ({COL_FILE_GRANTS_ID}, {COL_FILE_GRANTS_CONTENT_KEY}, {COL_FILE_GRANTS_SPACE_ID}, \
          {COL_FILE_GRANTS_SIDECAR_KEY}, {COL_FILE_GRANTS_EPOCH}) \
         VALUES (?1, ?2, ?3, ?4, ?5) \
         ON CONFLICT ({COL_FILE_GRANTS_CONTENT_KEY}, {COL_FILE_GRANTS_SPACE_ID}) \
         DO UPDATE SET \
           {COL_FILE_GRANTS_SIDECAR_KEY} = excluded.{COL_FILE_GRANTS_SIDECAR_KEY}, \
           {COL_FILE_GRANTS_EPOCH} = excluded.{COL_FILE_GRANTS_EPOCH}"
    );
    execute_with_crdt(
        sql,
        vec![
            JsonValue::String(id.to_string()),
            JsonValue::String(content_key.to_string()),
            JsonValue::String(space_id.to_string()),
            JsonValue::String(sidecar_key.to_string()),
            JsonValue::from(epoch),
        ],
        db,
        hlc,
        column_sig_key_cache,
    )?;
    Ok(())
}

/// Delete the grant row for `(content_key, space_id)`, matching the
/// UNIQUE index. Returns Ok(()) whether or not a row was present — an
/// absent row means the caller and receiver agree the grant is gone.
///
/// A DELETE via `execute_with_crdt` writes to the delete-log
/// (`haex_shared_space_deleted_rows`) which then flows through the same
/// space-scoped sync as the grant insert.
pub fn delete_grant(
    db: &DbConnection,
    hlc: &MutexGuard<HlcService>,
    column_sig_key_cache: &SpaceKeyCache,
    content_key: &str,
    space_id: &str,
) -> Result<(), GrantError> {
    let sql = format!(
        "DELETE FROM {TABLE_FILE_GRANTS} \
         WHERE {COL_FILE_GRANTS_CONTENT_KEY} = ?1 AND {COL_FILE_GRANTS_SPACE_ID} = ?2"
    );
    execute_with_crdt(
        sql,
        vec![
            JsonValue::String(content_key.to_string()),
            JsonValue::String(space_id.to_string()),
        ],
        db,
        hlc,
        column_sig_key_cache,
    )?;
    Ok(())
}

/// List all grants for a given `content_key`. Used by the UI to render
/// "shared with: [Alpha, Beta]" and by the reconciliation task to detect
/// bucket-vs-CRDT drift.
pub fn list_grants_for_content(
    db: &DbConnection,
    content_key: &str,
) -> Result<Vec<FileGrantRow>, GrantError> {
    let sql = format!(
        "SELECT {COL_FILE_GRANTS_ID}, {COL_FILE_GRANTS_CONTENT_KEY}, {COL_FILE_GRANTS_SPACE_ID}, \
         {COL_FILE_GRANTS_SIDECAR_KEY}, {COL_FILE_GRANTS_EPOCH}, {COL_FILE_GRANTS_CREATED_AT} \
         FROM {TABLE_FILE_GRANTS} \
         WHERE {COL_FILE_GRANTS_CONTENT_KEY} = ?1 \
         ORDER BY {COL_FILE_GRANTS_SPACE_ID} ASC"
    );
    let rows = select_with_crdt(sql, vec![JsonValue::String(content_key.to_string())], db)?;
    rows.into_iter().map(row_to_grant).collect()
}

/// List all grants targeting `space_id`. Used when we need to iterate
/// every file the current user knows to be shared into a given space —
/// e.g. an epoch-rotation rewrap pass.
pub fn list_grants_for_space(
    db: &DbConnection,
    space_id: &str,
) -> Result<Vec<FileGrantRow>, GrantError> {
    let sql = format!(
        "SELECT {COL_FILE_GRANTS_ID}, {COL_FILE_GRANTS_CONTENT_KEY}, {COL_FILE_GRANTS_SPACE_ID}, \
         {COL_FILE_GRANTS_SIDECAR_KEY}, {COL_FILE_GRANTS_EPOCH}, {COL_FILE_GRANTS_CREATED_AT} \
         FROM {TABLE_FILE_GRANTS} \
         WHERE {COL_FILE_GRANTS_SPACE_ID} = ?1 \
         ORDER BY {COL_FILE_GRANTS_CONTENT_KEY} ASC"
    );
    let rows = select_with_crdt(sql, vec![JsonValue::String(space_id.to_string())], db)?;
    rows.into_iter().map(row_to_grant).collect()
}

fn row_to_grant(row: Vec<JsonValue>) -> Result<FileGrantRow, GrantError> {
    if row.len() != 6 {
        return Err(GrantError::RowShape {
            reason: format!("expected 6 columns, got {}", row.len()),
        });
    }
    Ok(FileGrantRow {
        id: take_string(&row[0], "id")?,
        content_key: take_string(&row[1], "content_key")?,
        space_id: take_string(&row[2], "space_id")?,
        sidecar_key: take_string(&row[3], "sidecar_key")?,
        epoch: row[4].as_i64().ok_or_else(|| GrantError::RowShape {
            reason: "epoch column is not an integer".into(),
        })?,
        created_at: take_string(&row[5], "created_at")?,
    })
}

fn take_string(v: &JsonValue, col: &str) -> Result<String, GrantError> {
    v.as_str()
        .map(str::to_string)
        .ok_or_else(|| GrantError::RowShape {
            reason: format!("column `{col}` is not a string"),
        })
}
