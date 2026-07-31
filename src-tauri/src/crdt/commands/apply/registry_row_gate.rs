//! Task B.5 — puller-side row-level verification gate for
//! `haex_shared_space_sync` registry rows.
//!
//! Wired into [`super::db::apply_remote_changes_to_db_scoped`] as "Stage 5b":
//! it batches the per-column `RemoteColumnChange`s belonging to one registry
//! row into the [`IncomingRegistryChange`] shape Task B.4's
//! `verify_incoming_registry_change` expects — filling any column the batch
//! does not touch from the row's currently persisted value — and runs
//! BEFORE the existing per-column signature gate (`verify_change_sig` in
//! `db.rs`). Stacked defense-in-depth, not a replacement: a row-sig failure
//! drops the entire row's change set atomically, whereas a column-sig
//! failure is per-column.

use rusqlite::{OptionalExtension, Transaction};
use serde_json::Value as JsonValue;

use crate::crdt::registry_row_sig::puller_verify::{IncomingRegistryChange, PersistedRegistryRow};
use crate::database::error::DatabaseError;
use crate::table_names::{
    COL_SHARED_SPACE_SYNC_AUTHORED_BY_DID, COL_SHARED_SPACE_SYNC_CATEGORY,
    COL_SHARED_SPACE_SYNC_CATEGORY_LABEL, COL_SHARED_SPACE_SYNC_CREATED_AT,
    COL_SHARED_SPACE_SYNC_EXTENSION_NAME, COL_SHARED_SPACE_SYNC_EXTENSION_PUBLIC_KEY,
    COL_SHARED_SPACE_SYNC_ID, COL_SHARED_SPACE_SYNC_ROW_PKS, COL_SHARED_SPACE_SYNC_ROW_SIG,
    COL_SHARED_SPACE_SYNC_SPACE_ID, COL_SHARED_SPACE_SYNC_TABLE_NAME, COL_SHARED_SPACE_SYNC_TYPE,
    COL_SHARED_SPACE_SYNC_TYPE_LABEL, TABLE_SHARED_SPACE_SYNC,
};

use super::super::helpers::json_values_to_sql_params;
use super::types::RemoteColumnChange;

/// Every column that is part of `RegistryRowSigPayload` (the row-sig
/// preimage) other than `id` — the row's own CRDT primary key, which is
/// never carried as a column-level change (mirrors how PK columns never
/// appear in `row_change_list` elsewhere in this pipeline).
///
/// Any incoming change that touches one of these MUST carry a freshly
/// updated `row_sig` in the same batch: the writer side
/// (`sign_registry_row_self`, Task B.3) always re-signs whenever any of
/// these fields change. A batch that changes one of them without also
/// carrying `row_sig` is either dropping data in transit or a forgery
/// attempt (relabel a field, replay the old signature) — either way it
/// cannot be legitimately verified, so [`build_incoming_registry_change`]
/// rejects it outright rather than silently falling back to the row's stale
/// persisted `row_sig`.
const SIGNED_PAYLOAD_COLUMNS: &[&str] = &[
    COL_SHARED_SPACE_SYNC_SPACE_ID,
    COL_SHARED_SPACE_SYNC_TABLE_NAME,
    COL_SHARED_SPACE_SYNC_ROW_PKS,
    COL_SHARED_SPACE_SYNC_EXTENSION_PUBLIC_KEY,
    COL_SHARED_SPACE_SYNC_EXTENSION_NAME,
    COL_SHARED_SPACE_SYNC_CATEGORY,
    COL_SHARED_SPACE_SYNC_TYPE,
    COL_SHARED_SPACE_SYNC_CATEGORY_LABEL,
    COL_SHARED_SPACE_SYNC_TYPE_LABEL,
    COL_SHARED_SPACE_SYNC_CREATED_AT,
    COL_SHARED_SPACE_SYNC_AUTHORED_BY_DID,
];

/// Outcome of assembling one registry row's incoming change.
pub(super) enum RegistryRowChangeOutcome {
    /// The batch touched nothing covered by the row-sig payload on an
    /// existing row (e.g. a hypothetical CRDT-meta-only re-touch). The
    /// existing `row_sig` already covers the row's identity — safe to skip
    /// B.5 verification and fall through to the normal per-column apply.
    NothingSignedTouched,
    /// A signed field changed but the batch did not carry a fresh
    /// `row_sig` alongside it. Rejected without even calling B.4 — there is
    /// nothing legitimate to verify against.
    MissingFreshRowSig,
    /// Ready to hand to `verify_incoming_registry_change`. Boxed — this
    /// variant is far larger than its siblings (`IncomingRegistryChange`
    /// carries a dozen owned `String`/`Option<String>` fields).
    Ready(Box<IncomingRegistryChange>),
}

/// Every field of a persisted `haex_shared_space_sync` row, used to fill in
/// columns the incoming batch does not touch. `None` when the row does not
/// exist locally yet (the INSERT case).
struct PersistedRegistryRowFull {
    space_id: String,
    table_name: String,
    row_pks: String,
    extension_public_key: Option<String>,
    extension_name: Option<String>,
    category: Option<String>,
    r#type: Option<String>,
    category_label: Option<String>,
    type_label: Option<String>,
    authored_by_did: String,
    created_at: String,
    row_sig: String,
}

fn pk_params_refs(pk_values: &[rusqlite::types::Value]) -> Vec<&dyn rusqlite::ToSql> {
    pk_values
        .iter()
        .map(|v| v as &dyn rusqlite::ToSql)
        .collect()
}

fn fetch_persisted_registry_row_full(
    tx: &Transaction,
    pk_where_clause: &str,
    pk_values_for_query: &[JsonValue],
) -> Result<Option<PersistedRegistryRowFull>, DatabaseError> {
    let sql = format!(
        "SELECT {COL_SHARED_SPACE_SYNC_SPACE_ID}, {COL_SHARED_SPACE_SYNC_TABLE_NAME}, \
                {COL_SHARED_SPACE_SYNC_ROW_PKS}, {COL_SHARED_SPACE_SYNC_EXTENSION_PUBLIC_KEY}, \
                {COL_SHARED_SPACE_SYNC_EXTENSION_NAME}, {COL_SHARED_SPACE_SYNC_CATEGORY}, \
                {COL_SHARED_SPACE_SYNC_TYPE}, {COL_SHARED_SPACE_SYNC_CATEGORY_LABEL}, \
                {COL_SHARED_SPACE_SYNC_TYPE_LABEL}, {COL_SHARED_SPACE_SYNC_AUTHORED_BY_DID}, \
                {COL_SHARED_SPACE_SYNC_CREATED_AT}, {COL_SHARED_SPACE_SYNC_ROW_SIG} \
         FROM \"{TABLE_SHARED_SPACE_SYNC}\" WHERE {pk_where_clause}"
    );
    let pk_values = json_values_to_sql_params(pk_values_for_query)?;
    let mut stmt = tx.prepare(&sql).map_err(DatabaseError::from)?;
    stmt.query_row(&*pk_params_refs(&pk_values), |row| {
        Ok(PersistedRegistryRowFull {
            space_id: row.get(0)?,
            table_name: row.get(1)?,
            row_pks: row.get(2)?,
            extension_public_key: row.get(3)?,
            extension_name: row.get(4)?,
            category: row.get(5)?,
            r#type: row.get(6)?,
            category_label: row.get(7)?,
            type_label: row.get(8)?,
            authored_by_did: row.get(9)?,
            created_at: row.get(10)?,
            row_sig: row.get(11)?,
        })
    })
    .optional()
    .map_err(DatabaseError::from)
}

/// Fetch just the persisted `authored_by_did` of a `haex_shared_space_sync`
/// row — the only field B.4's immutability check needs. `None` when the row
/// does not exist locally yet (the INSERT case).
pub(super) fn fetch_persisted_registry_authored_by_did(
    tx: &Transaction,
    pk_where_clause: &str,
    pk_values_for_query: &[JsonValue],
) -> Result<Option<PersistedRegistryRow>, DatabaseError> {
    let sql = format!(
        "SELECT {COL_SHARED_SPACE_SYNC_AUTHORED_BY_DID} \
         FROM \"{TABLE_SHARED_SPACE_SYNC}\" WHERE {pk_where_clause}"
    );
    let pk_values = json_values_to_sql_params(pk_values_for_query)?;
    let mut stmt = tx.prepare(&sql).map_err(DatabaseError::from)?;
    let authored_by_did: Option<String> = stmt
        .query_row(&*pk_params_refs(&pk_values), |row| row.get(0))
        .optional()
        .map_err(DatabaseError::from)?;
    Ok(authored_by_did.map(|authored_by_did| PersistedRegistryRow { authored_by_did }))
}

/// Batch this row's `RemoteColumnChange`s into an [`IncomingRegistryChange`],
/// filling every column the batch does not touch from the row's persisted
/// value (or `""`/`None` on the INSERT path, where there is no persisted
/// value at all).
///
/// `row_pks_map` is the registry row's own CRDT primary key (`{"id": ...}`,
/// already parsed by the caller) — NOT to be confused with the
/// `row_pks` *column* on the register row itself, which identifies the
/// target extension row this registry entry is about.
pub(super) fn build_incoming_registry_change(
    tx: &Transaction,
    pk_where_clause: &str,
    pk_values_for_query: &[JsonValue],
    row_pks_map: &serde_json::Map<String, JsonValue>,
    batch: &[RemoteColumnChange],
) -> Result<RegistryRowChangeOutcome, DatabaseError> {
    let persisted = fetch_persisted_registry_row_full(tx, pk_where_clause, pk_values_for_query)?;

    let touches_signed_payload = batch
        .iter()
        .any(|c| SIGNED_PAYLOAD_COLUMNS.contains(&c.column_name.as_str()));

    // An existing row whose batch touches nothing sig-relevant needs no
    // fresh verification — its persisted row_sig already covers its
    // identity. A brand-new row always goes through Ready below (it needs
    // its very first verification), even in the — practically impossible —
    // case where its INSERT batch happens to touch nothing on this list.
    if !touches_signed_payload && persisted.is_some() {
        return Ok(RegistryRowChangeOutcome::NothingSignedTouched);
    }

    let has_fresh_row_sig = batch
        .iter()
        .any(|c| c.column_name == COL_SHARED_SPACE_SYNC_ROW_SIG);
    if touches_signed_payload && !has_fresh_row_sig {
        return Ok(RegistryRowChangeOutcome::MissingFreshRowSig);
    }

    // Batch value wins; otherwise fall back to the persisted value (empty/
    // None when the row doesn't exist yet — an absent required field there
    // fails cleanly downstream as `RowSigMissingOrEmpty` or a signature
    // mismatch rather than panicking here).
    let text = |col: &str, fallback: &str| -> String {
        batch
            .iter()
            .find(|c| c.column_name == col)
            .and_then(|c| c.decrypted_value.as_str())
            .map(str::to_string)
            .unwrap_or_else(|| fallback.to_string())
    };
    let opt_text = |col: &str, fallback: Option<&str>| -> Option<String> {
        match batch.iter().find(|c| c.column_name == col) {
            Some(c) => c.decrypted_value.as_str().map(str::to_string),
            None => fallback.map(str::to_string),
        }
    };

    let id = row_pks_map
        .get(COL_SHARED_SPACE_SYNC_ID)
        .and_then(JsonValue::as_str)
        .unwrap_or_default()
        .to_string();

    let p = persisted.as_ref();
    let change = IncomingRegistryChange {
        id,
        space_id: text(
            COL_SHARED_SPACE_SYNC_SPACE_ID,
            p.map_or("", |r| r.space_id.as_str()),
        ),
        table_name: text(
            COL_SHARED_SPACE_SYNC_TABLE_NAME,
            p.map_or("", |r| r.table_name.as_str()),
        ),
        row_pks: text(
            COL_SHARED_SPACE_SYNC_ROW_PKS,
            p.map_or("", |r| r.row_pks.as_str()),
        ),
        extension_public_key: opt_text(
            COL_SHARED_SPACE_SYNC_EXTENSION_PUBLIC_KEY,
            p.and_then(|r| r.extension_public_key.as_deref()),
        ),
        extension_name: opt_text(
            COL_SHARED_SPACE_SYNC_EXTENSION_NAME,
            p.and_then(|r| r.extension_name.as_deref()),
        ),
        category: opt_text(
            COL_SHARED_SPACE_SYNC_CATEGORY,
            p.and_then(|r| r.category.as_deref()),
        ),
        r#type: opt_text(
            COL_SHARED_SPACE_SYNC_TYPE,
            p.and_then(|r| r.r#type.as_deref()),
        ),
        category_label: opt_text(
            COL_SHARED_SPACE_SYNC_CATEGORY_LABEL,
            p.and_then(|r| r.category_label.as_deref()),
        ),
        type_label: opt_text(
            COL_SHARED_SPACE_SYNC_TYPE_LABEL,
            p.and_then(|r| r.type_label.as_deref()),
        ),
        authored_by_did: text(
            COL_SHARED_SPACE_SYNC_AUTHORED_BY_DID,
            p.map_or("", |r| r.authored_by_did.as_str()),
        ),
        created_at: text(
            COL_SHARED_SPACE_SYNC_CREATED_AT,
            p.map_or("", |r| r.created_at.as_str()),
        ),
        row_sig: text(
            COL_SHARED_SPACE_SYNC_ROW_SIG,
            p.map_or("", |r| r.row_sig.as_str()),
        ),
    };

    Ok(RegistryRowChangeOutcome::Ready(Box::new(change)))
}
