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

/// Columns whose corresponding field on both `IncomingRegistryChange` and
/// `RegistryRowSigPayload` (`registry_row_sig::payload`) is a plain,
/// non-`Option` string — i.e. the fields fed through the `text()` closure
/// below, not `opt_text()`. There is no legitimate way for a peer to send
/// an explicit JSON `null` for one of these: unlike an *absent* column
/// (which correctly falls back to the row's persisted value), a
/// *present-but-null* column here can only mean data loss in transit or a
/// forgery attempt, and silently falling back to the persisted value would
/// corrupt signature reconstruction — the real signer's payload had `null`
/// for this field, not the stale persisted string. See
/// `RegistryRowChangeOutcome::RequiredFieldExplicitlyNull`.
const REQUIRED_TEXT_COLUMNS: &[&str] = &[
    COL_SHARED_SPACE_SYNC_SPACE_ID,
    COL_SHARED_SPACE_SYNC_TABLE_NAME,
    COL_SHARED_SPACE_SYNC_ROW_PKS,
    COL_SHARED_SPACE_SYNC_AUTHORED_BY_DID,
    COL_SHARED_SPACE_SYNC_CREATED_AT,
    COL_SHARED_SPACE_SYNC_ROW_SIG,
];

/// Outcome of assembling one registry row's incoming change.
pub(super) enum RegistryRowChangeOutcome {
    /// The batch touched nothing covered by the row-sig payload on an
    /// existing row (e.g. a hypothetical CRDT-meta-only re-touch). The
    /// existing `row_sig` already covers the row's identity — safe to skip
    /// B.5 verification and fall through to the normal per-column apply.
    NothingSignedTouched,
    /// The batch touched `row_sig` and NOTHING ELSE from
    /// [`SIGNED_PAYLOAD_COLUMNS`], on an existing row. A `row_sig` cannot be
    /// verified on its own: verification reconstructs the signed preimage
    /// from the payload columns, which this batch does not carry — it can
    /// only fall back to the row's *already-persisted* payload. Accepting
    /// the incoming `row_sig` in that case would let an attacker overwrite
    /// the persisted signature with a stale-but-internally-valid one lifted
    /// from an earlier version of the same row (a replay), decoupling the
    /// stored `row_sig` from the content it is supposed to cover without
    /// ever running it through verification. Rejected outright — the
    /// mirror-image case of `MissingFreshRowSig` below, but for the payload
    /// side rather than the signature side. Carries the persisted row's
    /// `space_id` and `authored_by_did`, for forensic logging at the call
    /// site.
    RowSigOnlyBatch {
        space_id: String,
        authored_by_did: String,
    },
    /// A signed field changed but the batch did not carry a fresh
    /// `row_sig` alongside it. Rejected without even calling B.4 — there is
    /// nothing legitimate to verify against. Carries the names of the
    /// touched signed column(s), for forensic logging at the call site.
    MissingFreshRowSig(Vec<String>),
    /// The batch carried an explicit JSON `null` for one of
    /// [`REQUIRED_TEXT_COLUMNS`] — a column backing a plain (non-`Option`)
    /// field on the reconstructed payload. Distinct from that column being
    /// merely *absent* from the batch, which is the normal
    /// fall-back-to-persisted-value case. Rejected rather than silently
    /// substituting the stale persisted value, which would decouple the
    /// reconstructed payload from whatever the real signer actually signed.
    /// Carries the name(s) of the offending column(s), for forensic logging
    /// at the call site.
    RequiredFieldExplicitlyNull(Vec<String>),
    /// Ready to hand to `verify_incoming_registry_change`. `change` is
    /// boxed — this variant is far larger than its siblings
    /// (`IncomingRegistryChange` carries a dozen owned
    /// `String`/`Option<String>` fields). `persisted` is the row's existing
    /// `authored_by_did` (already read as part of the same full-row fetch
    /// this function needed anyway for column fallback) — `None` for an
    /// INSERT, `Some` for an UPDATE. The caller feeds it straight into
    /// `verify_incoming_registry_change` without a second SELECT.
    Ready {
        change: Box<IncomingRegistryChange>,
        persisted: Option<PersistedRegistryRow>,
    },
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

    let touches_signed_payload = batch.iter().any(|c| {
        SIGNED_PAYLOAD_COLUMNS
            .iter()
            .any(|s| c.column_name.eq_ignore_ascii_case(s))
    });
    // Non-null on top of presence: an explicit JSON `null` for `row_sig` is
    // never a "fresh" signature — it is caught by the
    // `explicit_null_required_columns` pre-check below (row_sig is itself
    // one of `REQUIRED_TEXT_COLUMNS`). Requiring non-null here too means
    // that check, not this one, is what a null row_sig sees first, since it
    // runs before the branches below that consume this flag.
    let has_fresh_row_sig = batch.iter().any(|c| {
        c.column_name
            .eq_ignore_ascii_case(COL_SHARED_SPACE_SYNC_ROW_SIG)
            && !c.decrypted_value.is_null()
    });

    // A column in REQUIRED_TEXT_COLUMNS present with an explicit JSON
    // `null` is never legitimate (see `RequiredFieldExplicitlyNull`'s
    // doc-comment) — reject before `text()` further down gets a chance to
    // collapse it into the "absent" fallback path. Checked case-insensitively,
    // same as `touches_signed_payload`/`has_fresh_row_sig`.
    //
    // Runs BEFORE the `touches_signed_payload`/`has_fresh_row_sig` branches
    // below: `row_sig` is itself a `REQUIRED_TEXT_COLUMNS` entry, so a batch
    // carrying `row_sig: null` must land here — as
    // `RequiredFieldExplicitlyNull` — rather than being misclassified as
    // `RowSigOnlyBatch` or `MissingFreshRowSig` by the branches that follow.
    let explicit_null_required_columns: Vec<String> = batch
        .iter()
        .filter(|c| {
            c.decrypted_value.is_null()
                && REQUIRED_TEXT_COLUMNS
                    .iter()
                    .any(|s| c.column_name.eq_ignore_ascii_case(s))
        })
        .map(|c| c.column_name.clone())
        .collect();
    if !explicit_null_required_columns.is_empty() {
        return Ok(RegistryRowChangeOutcome::RequiredFieldExplicitlyNull(
            explicit_null_required_columns,
        ));
    }

    // An existing row whose batch touches nothing sig-relevant needs no
    // fresh verification — its persisted row_sig already covers its
    // identity. A brand-new row always goes through Ready below (it needs
    // its very first verification), even in the — practically impossible —
    // case where its INSERT batch happens to touch nothing on this list.
    //
    // EXCEPT: if the batch touches `row_sig` itself while touching nothing
    // else on the payload list, that is not "nothing sig-relevant" — it is
    // exactly the shape of a replay attack (see `RowSigOnlyBatch`'s
    // doc-comment). Reject it before it can reach the per-column apply loop
    // and overwrite the persisted signature unchecked.
    if !touches_signed_payload {
        if let Some(p) = persisted.as_ref() {
            if has_fresh_row_sig {
                return Ok(RegistryRowChangeOutcome::RowSigOnlyBatch {
                    space_id: p.space_id.clone(),
                    authored_by_did: p.authored_by_did.clone(),
                });
            }
            return Ok(RegistryRowChangeOutcome::NothingSignedTouched);
        }
    }

    // PR #741 finding 5 (investigated 2026-08-01, deferred — see
    // docs/plans/2026-08-01-pr741-finding-5-defer.md): this branch also
    // fires for a *legitimate* split — a peer that pushed a signed column
    // in one batch and its fresh `row_sig` in a later one. Today that split
    // cannot actually happen:
    //   - Writer side: `sign_registry_row_self` (execute.rs) always
    //     re-signs `row_sig` inside the SAME `execute_with_crdt` transaction
    //     as any write to a `SIGNED_PAYLOAD_COLUMNS` field, so the two
    //     changes are structurally born with the same HLC.
    //   - Wire side: `group_by_transaction_hlc` groups by HLC == one
    //     transaction, and both the sender's pagination
    //     (`scanner::paginate_changes`) and the puller's page buffering
    //     (`sync_loop::pull::split_complete_groups` /
    //     `apply_groups_advancing_cursor`) never apply or advance the
    //     cursor past a group until it is received whole — so same-HLC
    //     changes always arrive, and apply, together.
    // The one path that bypasses this atomicity is the owner-vault
    // pending-column schema-drift recovery (`run_owner_pending_column_recovery`
    // / `handle_owner_pull_columns`), which pulls one `(table, column)` pair
    // at a time with no `row_sig` pairing. It cannot split `row_sig` from a
    // `SIGNED_PAYLOAD_COLUMNS` field TODAY because migration 0014 introduced
    // both together — no schema state exists where a device knows one but
    // not the other. If a FUTURE migration adds a new signed registry column
    // after `row_sig` already exists, that recovery path could reintroduce a
    // genuine split; at that point a `PendingSplitBatch` outcome plus the
    // existing `TABLE_CRDT_PENDING_COLUMNS` marker/no-HLC-advance pattern
    // (see `apply/db.rs`'s unknown-column handling) is the right fix.
    if touches_signed_payload && !has_fresh_row_sig {
        let touched_signed_columns: Vec<String> = batch
            .iter()
            .filter(|c| SIGNED_PAYLOAD_COLUMNS.contains(&c.column_name.as_str()))
            .map(|c| c.column_name.clone())
            .collect();
        return Ok(RegistryRowChangeOutcome::MissingFreshRowSig(
            touched_signed_columns,
        ));
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

    // Extract after `change` is built — `p` (borrowed from `persisted`)
    // is done being read by that point, so `persisted` can be consumed here
    // instead of re-fetched. This is the same row this function's own
    // `fetch_persisted_registry_row_full` call above already read; a
    // second SELECT for just `authored_by_did` would be redundant.
    let persisted_authored_by_did = persisted.map(|full| PersistedRegistryRow {
        authored_by_did: full.authored_by_did,
    });

    Ok(RegistryRowChangeOutcome::Ready {
        change: Box::new(change),
        persisted: persisted_authored_by_did,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use rusqlite::Connection;

    /// Minimal schema for `build_incoming_registry_change` — just the
    /// registry columns this module reads directly. The CRDT bookkeeping
    /// columns (`haex_hlc`, `haex_column_hlcs`, ...) the full apply pipeline
    /// needs live one level up in `db.rs` and are irrelevant to this
    /// function.
    fn setup_db() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(&format!(
            "CREATE TABLE \"{TABLE_SHARED_SPACE_SYNC}\" (
                id TEXT PRIMARY KEY NOT NULL,
                table_name TEXT NOT NULL,
                row_pks TEXT NOT NULL,
                space_id TEXT NOT NULL,
                extension_public_key TEXT,
                extension_name TEXT,
                category TEXT,
                type TEXT,
                type_label TEXT,
                category_label TEXT,
                authored_by_did TEXT DEFAULT '' NOT NULL,
                row_sig TEXT DEFAULT '' NOT NULL,
                created_at TEXT DEFAULT (CURRENT_TIMESTAMP)
            );"
        ))
        .unwrap();
        conn
    }

    fn row_sig_only_change(value: &str) -> RemoteColumnChange {
        RemoteColumnChange {
            table_name: TABLE_SHARED_SPACE_SYNC.to_string(),
            row_pks: r#"{"id":"reg-1"}"#.to_string(),
            column_name: COL_SHARED_SPACE_SYNC_ROW_SIG.to_string(),
            hlc_timestamp: "2/bbb".to_string(),
            decrypted_value: JsonValue::String(value.to_string()),
            sig: None,
        }
    }

    /// The attack this fix closes: a batch touching ONLY `row_sig` on an
    /// existing row — no payload column present to verify it against —
    /// must be rejected as `RowSigOnlyBatch`, not silently treated as
    /// `NothingSignedTouched` (which would let the per-column apply loop
    /// write the attacker-supplied `row_sig` unchecked).
    #[test]
    fn build_incoming_registry_change_rejects_batch_with_only_row_sig() {
        let mut conn = setup_db();
        conn.execute(
            &format!(
                "INSERT INTO \"{TABLE_SHARED_SPACE_SYNC}\" \
                 (id, table_name, row_pks, space_id, authored_by_did, row_sig, created_at) \
                 VALUES ('reg-1', 'ext_calendar_v1', '{{\"id\":\"evt-1\"}}', 'space-1', \
                         'did:key:alice', 'original-sig', '2026-01-01T00:00:00Z')"
            ),
            [],
        )
        .unwrap();

        let tx = conn.transaction().unwrap();
        let row_pks_map = serde_json::json!({ "id": "reg-1" })
            .as_object()
            .unwrap()
            .clone();
        let batch = vec![row_sig_only_change("attacker-old-sig")];

        let outcome = build_incoming_registry_change(
            &tx,
            "id = ?1",
            &[JsonValue::String("reg-1".to_string())],
            &row_pks_map,
            &batch,
        )
        .unwrap();

        match outcome {
            RegistryRowChangeOutcome::RowSigOnlyBatch {
                space_id,
                authored_by_did,
            } => {
                assert_eq!(space_id, "space-1");
                assert_eq!(authored_by_did, "did:key:alice");
            }
            RegistryRowChangeOutcome::NothingSignedTouched => {
                panic!("expected RowSigOnlyBatch, got NothingSignedTouched")
            }
            RegistryRowChangeOutcome::MissingFreshRowSig(_) => {
                panic!("expected RowSigOnlyBatch, got MissingFreshRowSig")
            }
            RegistryRowChangeOutcome::RequiredFieldExplicitlyNull(_) => {
                panic!("expected RowSigOnlyBatch, got RequiredFieldExplicitlyNull")
            }
            RegistryRowChangeOutcome::Ready { .. } => {
                panic!("expected RowSigOnlyBatch, got Ready")
            }
        }
    }

    /// A batch touching neither `row_sig` nor any payload column on an
    /// existing row is still the benign `NothingSignedTouched` case — the
    /// new guard must not over-reject.
    #[test]
    fn build_incoming_registry_change_allows_batch_touching_neither_row_sig_nor_payload() {
        let mut conn = setup_db();
        conn.execute(
            &format!(
                "INSERT INTO \"{TABLE_SHARED_SPACE_SYNC}\" \
                 (id, table_name, row_pks, space_id, authored_by_did, row_sig, created_at) \
                 VALUES ('reg-1', 'ext_calendar_v1', '{{\"id\":\"evt-1\"}}', 'space-1', \
                         'did:key:alice', 'original-sig', '2026-01-01T00:00:00Z')"
            ),
            [],
        )
        .unwrap();

        let tx = conn.transaction().unwrap();
        let row_pks_map = serde_json::json!({ "id": "reg-1" })
            .as_object()
            .unwrap()
            .clone();
        // A change to some hypothetical non-signed column — none of the
        // payload list, and not row_sig either.
        let batch = vec![RemoteColumnChange {
            table_name: TABLE_SHARED_SPACE_SYNC.to_string(),
            row_pks: r#"{"id":"reg-1"}"#.to_string(),
            column_name: "some_untracked_column".to_string(),
            hlc_timestamp: "2/bbb".to_string(),
            decrypted_value: JsonValue::Null,
            sig: None,
        }];

        let outcome = build_incoming_registry_change(
            &tx,
            "id = ?1",
            &[JsonValue::String("reg-1".to_string())],
            &row_pks_map,
            &batch,
        )
        .unwrap();

        assert!(matches!(
            outcome,
            RegistryRowChangeOutcome::NothingSignedTouched
        ));
    }

    /// Same attack as `..._rejects_batch_with_only_row_sig`, but the
    /// incoming column name is mixed-case (`Row_Sig`). The local schema is
    /// fixed lowercase today, so this can only arise from a malformed/
    /// forged batch — but the case-sensitive `==`/`contains` checks this
    /// guards against would otherwise let it slip through as
    /// `NothingSignedTouched` (matching neither `SIGNED_PAYLOAD_COLUMNS` nor
    /// `COL_SHARED_SPACE_SYNC_ROW_SIG` literally), even though SQLite itself
    /// resolves the column name case-insensitively at the eventual write.
    /// `eq_ignore_ascii_case` must catch it and still produce
    /// `RowSigOnlyBatch`.
    #[test]
    fn build_incoming_registry_change_rejects_mixed_case_row_sig_column() {
        let mut conn = setup_db();
        conn.execute(
            &format!(
                "INSERT INTO \"{TABLE_SHARED_SPACE_SYNC}\" \
                 (id, table_name, row_pks, space_id, authored_by_did, row_sig, created_at) \
                 VALUES ('reg-1', 'ext_calendar_v1', '{{\"id\":\"evt-1\"}}', 'space-1', \
                         'did:key:alice', 'original-sig', '2026-01-01T00:00:00Z')"
            ),
            [],
        )
        .unwrap();

        let tx = conn.transaction().unwrap();
        let row_pks_map = serde_json::json!({ "id": "reg-1" })
            .as_object()
            .unwrap()
            .clone();
        let batch = vec![RemoteColumnChange {
            table_name: TABLE_SHARED_SPACE_SYNC.to_string(),
            row_pks: r#"{"id":"reg-1"}"#.to_string(),
            column_name: "Row_Sig".to_string(),
            hlc_timestamp: "2/bbb".to_string(),
            decrypted_value: JsonValue::String("attacker-old-sig".to_string()),
            sig: None,
        }];

        let outcome = build_incoming_registry_change(
            &tx,
            "id = ?1",
            &[JsonValue::String("reg-1".to_string())],
            &row_pks_map,
            &batch,
        )
        .unwrap();

        match outcome {
            RegistryRowChangeOutcome::RowSigOnlyBatch {
                space_id,
                authored_by_did,
            } => {
                assert_eq!(space_id, "space-1");
                assert_eq!(authored_by_did, "did:key:alice");
            }
            RegistryRowChangeOutcome::NothingSignedTouched => {
                panic!("expected RowSigOnlyBatch, got NothingSignedTouched")
            }
            RegistryRowChangeOutcome::MissingFreshRowSig(_) => {
                panic!("expected RowSigOnlyBatch, got MissingFreshRowSig")
            }
            RegistryRowChangeOutcome::RequiredFieldExplicitlyNull(_) => {
                panic!("expected RowSigOnlyBatch, got RequiredFieldExplicitlyNull")
            }
            RegistryRowChangeOutcome::Ready { .. } => {
                panic!("expected RowSigOnlyBatch, got Ready")
            }
        }
    }

    fn column_change(column_name: &str, value: JsonValue) -> RemoteColumnChange {
        RemoteColumnChange {
            table_name: TABLE_SHARED_SPACE_SYNC.to_string(),
            row_pks: r#"{"id":"reg-1"}"#.to_string(),
            column_name: column_name.to_string(),
            hlc_timestamp: "2/bbb".to_string(),
            decrypted_value: value,
            sig: None,
        }
    }

    fn insert_reg1_with_category(conn: &Connection, category: &str) {
        conn.execute(
            &format!(
                "INSERT INTO \"{TABLE_SHARED_SPACE_SYNC}\" \
                 (id, table_name, row_pks, space_id, category, authored_by_did, row_sig, created_at) \
                 VALUES ('reg-1', 'ext_calendar_v1', '{{\"id\":\"evt-1\"}}', 'space-1', '{category}', \
                         'did:key:alice', 'original-sig', '2026-01-01T00:00:00Z')"
            ),
            [],
        )
        .unwrap();
    }

    fn reg1_pk_args() -> (serde_json::Map<String, JsonValue>, Vec<JsonValue>) {
        let row_pks_map = serde_json::json!({ "id": "reg-1" })
            .as_object()
            .unwrap()
            .clone();
        (row_pks_map, vec![JsonValue::String("reg-1".to_string())])
    }

    /// A batch that does not touch `category` at all must fall back to the
    /// row's persisted `category` (the normal "absent column" case). A
    /// batch that touches `category` with an explicit JSON `null` must
    /// clear it to `None` instead (an explicit clear from the peer) — the
    /// two are distinct wire states and must not collapse into the same
    /// outcome.
    #[test]
    fn build_incoming_registry_change_distinguishes_absent_from_null_category() {
        // Case A: category absent from the batch -> falls back to persisted.
        let mut conn = setup_db();
        insert_reg1_with_category(&conn, "work");
        let tx = conn.transaction().unwrap();
        let (row_pks_map, pk_values) = reg1_pk_args();
        let batch = vec![
            column_change(
                COL_SHARED_SPACE_SYNC_TYPE_LABEL,
                JsonValue::String("Updated Label".to_string()),
            ),
            column_change(
                COL_SHARED_SPACE_SYNC_ROW_SIG,
                JsonValue::String("fresh-sig".to_string()),
            ),
        ];
        let outcome =
            build_incoming_registry_change(&tx, "id = ?1", &pk_values, &row_pks_map, &batch)
                .unwrap();
        match outcome {
            RegistryRowChangeOutcome::Ready { change, .. } => {
                assert_eq!(change.category.as_deref(), Some("work"));
            }
            _ => panic!("expected Ready"),
        }
        drop(tx);

        // Case B: category present with explicit JSON null -> cleared.
        let mut conn = setup_db();
        insert_reg1_with_category(&conn, "work");
        let tx = conn.transaction().unwrap();
        let (row_pks_map, pk_values) = reg1_pk_args();
        let batch = vec![
            column_change(COL_SHARED_SPACE_SYNC_CATEGORY, JsonValue::Null),
            column_change(
                COL_SHARED_SPACE_SYNC_ROW_SIG,
                JsonValue::String("fresh-sig".to_string()),
            ),
        ];
        let outcome =
            build_incoming_registry_change(&tx, "id = ?1", &pk_values, &row_pks_map, &batch)
                .unwrap();
        match outcome {
            RegistryRowChangeOutcome::Ready { change, .. } => {
                assert_eq!(change.category, None);
            }
            _ => panic!("expected Ready"),
        }
    }

    /// A batch touching ONLY `row_sig` — present, but with an explicit JSON
    /// `null` rather than a fresh signature string — on an existing row.
    /// `row_sig` is itself one of `REQUIRED_TEXT_COLUMNS`, so this must be
    /// rejected as `RequiredFieldExplicitlyNull`, not `RowSigOnlyBatch`: the
    /// latter would misclassify "peer sent a null" as "peer sent a
    /// bare/replayed signature", which matters for forensic logging even
    /// though both outcomes reject the row.
    #[test]
    fn row_sig_present_null_alone_is_required_field_explicit_null() {
        let mut conn = setup_db();
        insert_reg1_with_category(&conn, "work");
        let tx = conn.transaction().unwrap();
        let (row_pks_map, pk_values) = reg1_pk_args();
        let batch = vec![column_change(
            COL_SHARED_SPACE_SYNC_ROW_SIG,
            JsonValue::Null,
        )];

        let outcome =
            build_incoming_registry_change(&tx, "id = ?1", &pk_values, &row_pks_map, &batch)
                .unwrap();

        match outcome {
            RegistryRowChangeOutcome::RequiredFieldExplicitlyNull(cols) => {
                assert!(cols
                    .iter()
                    .any(|c| c.eq_ignore_ascii_case(COL_SHARED_SPACE_SYNC_ROW_SIG)));
            }
            other => panic!(
                "expected RequiredFieldExplicitlyNull, got a different outcome: {}",
                match other {
                    RegistryRowChangeOutcome::NothingSignedTouched => "NothingSignedTouched",
                    RegistryRowChangeOutcome::RowSigOnlyBatch { .. } => "RowSigOnlyBatch",
                    RegistryRowChangeOutcome::MissingFreshRowSig(_) => "MissingFreshRowSig",
                    RegistryRowChangeOutcome::Ready { .. } => "Ready",
                    RegistryRowChangeOutcome::RequiredFieldExplicitlyNull(_) => unreachable!(),
                }
            ),
        }
    }

    /// A batch touching a signed-payload column (`type_label`) AND
    /// `row_sig` — present, but explicit JSON `null` instead of a fresh
    /// signature string. Must still be rejected as
    /// `RequiredFieldExplicitlyNull`, not `MissingFreshRowSig` — the latter
    /// implies "no row_sig at all in the batch", which is a different wire
    /// state (and a different forensic message) than "row_sig present but
    /// null".
    #[test]
    fn row_sig_present_null_with_payload_columns_is_required_field_explicit_null() {
        let mut conn = setup_db();
        insert_reg1_with_category(&conn, "work");
        let tx = conn.transaction().unwrap();
        let (row_pks_map, pk_values) = reg1_pk_args();
        let batch = vec![
            column_change(
                COL_SHARED_SPACE_SYNC_TYPE_LABEL,
                JsonValue::String("Updated Label".to_string()),
            ),
            column_change(COL_SHARED_SPACE_SYNC_ROW_SIG, JsonValue::Null),
        ];

        let outcome =
            build_incoming_registry_change(&tx, "id = ?1", &pk_values, &row_pks_map, &batch)
                .unwrap();

        match outcome {
            RegistryRowChangeOutcome::RequiredFieldExplicitlyNull(cols) => {
                assert!(cols
                    .iter()
                    .any(|c| c.eq_ignore_ascii_case(COL_SHARED_SPACE_SYNC_ROW_SIG)));
            }
            other => panic!(
                "expected RequiredFieldExplicitlyNull, got a different outcome: {}",
                match other {
                    RegistryRowChangeOutcome::NothingSignedTouched => "NothingSignedTouched",
                    RegistryRowChangeOutcome::RowSigOnlyBatch { .. } => "RowSigOnlyBatch",
                    RegistryRowChangeOutcome::MissingFreshRowSig(_) => "MissingFreshRowSig",
                    RegistryRowChangeOutcome::Ready { .. } => "Ready",
                    RegistryRowChangeOutcome::RequiredFieldExplicitlyNull(_) => unreachable!(),
                }
            ),
        }
    }

    /// `authored_by_did` backs a plain (non-`Option`) field on both
    /// `IncomingRegistryChange` and `RegistryRowSigPayload` — there is no
    /// legitimate way for a peer to send it as an explicit JSON `null`.
    /// Must be rejected as `RequiredFieldExplicitlyNull`, not silently
    /// collapsed into the "absent -> use persisted value" fallback path.
    #[test]
    fn build_incoming_registry_change_rejects_required_field_explicit_null() {
        let mut conn = setup_db();
        insert_reg1_with_category(&conn, "work");
        let tx = conn.transaction().unwrap();
        let (row_pks_map, pk_values) = reg1_pk_args();
        let batch = vec![
            column_change(COL_SHARED_SPACE_SYNC_AUTHORED_BY_DID, JsonValue::Null),
            column_change(
                COL_SHARED_SPACE_SYNC_ROW_SIG,
                JsonValue::String("fresh-sig".to_string()),
            ),
        ];
        let outcome =
            build_incoming_registry_change(&tx, "id = ?1", &pk_values, &row_pks_map, &batch)
                .unwrap();
        match outcome {
            RegistryRowChangeOutcome::RequiredFieldExplicitlyNull(cols) => {
                assert!(cols
                    .iter()
                    .any(|c| c.eq_ignore_ascii_case(COL_SHARED_SPACE_SYNC_AUTHORED_BY_DID)));
            }
            other => panic!(
                "expected RequiredFieldExplicitlyNull, got a different outcome: {}",
                match other {
                    RegistryRowChangeOutcome::NothingSignedTouched => "NothingSignedTouched",
                    RegistryRowChangeOutcome::RowSigOnlyBatch { .. } => "RowSigOnlyBatch",
                    RegistryRowChangeOutcome::MissingFreshRowSig(_) => "MissingFreshRowSig",
                    RegistryRowChangeOutcome::Ready { .. } => "Ready",
                    RegistryRowChangeOutcome::RequiredFieldExplicitlyNull(_) => unreachable!(),
                }
            ),
        }
    }
}
