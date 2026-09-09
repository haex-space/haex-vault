use crate::database::core::with_connection;
use crate::database::error::DatabaseError;
use crate::AppState;
use haex_crdt::{HlcError, HlcService};
use tauri::State;

use super::policy::VaultApplyPolicy;
use super::types::{to_crate_change, RemoteColumnChange};

#[cfg(test)]
use super::types::ColumnSig;
#[cfg(test)]
use crate::crdt::shared_space_trigger::{
    DELETED_ROWS_TABLE, SHARED_SPACE_DELETED_ROWS_TABLE, SHARED_SPACE_SYNC_TABLE,
};
#[cfg(test)]
use crate::table_names::{TABLE_CRDT_CONFIGS, TABLE_CRDT_PENDING_TABLES};
#[cfg(test)]
use base64::{engine::general_purpose::STANDARD as BASE64, Engine};
#[cfg(test)]
use rusqlite::types::Value as SqlValue;
#[cfg(test)]
use serde_json::Value as JsonValue;

/// Applies remote changes in a single transaction, with HLC-ordered grouping.
/// Note: lastPullServerTimestamp is now updated by the TypeScript layer after successful apply
///
/// `space_id` is the space this pull was scoped to. It is the only
/// trustworthy anchor for verifying a signature on a row that does not exist
/// locally yet — see [`resolve_row_space_id_for_sig`]. `None` for
/// personal-vault sync, where nothing is signed.
#[tauri::command]
pub fn apply_remote_changes_in_transaction(
    changes: Vec<RemoteColumnChange>,
    backend_id: String,
    max_hlc: String,
    space_id: Option<String>,
    state: State<'_, AppState>,
) -> Result<(), DatabaseError> {
    // Lock HLC via `lock_or_fail` so a poisoned mutex fails LOUD with a
    // banner row. Previous behaviour was `.lock().ok().map(...)` which
    // silently passed `hlc_service=None` to `apply_remote_changes_to_db`
    // — that path applies the remote changes WITHOUT advancing the local
    // HLC clock, so subsequent local writes carry stale timestamps that
    // lose merge conflicts on the next sync round.
    let hlc_service = state.lock_or_fail(
        &state.hlc,
        crate::critical::CriticalFailureCode::HlcMutexPoisoned,
        "crdt::commands::apply_remote_changes_in_transaction",
        serde_json::json!({}),
    )?;
    apply_remote_changes_to_db_scoped(
        &state.db,
        changes,
        Some((&backend_id, &max_hlc)),
        Some(&*hlc_service),
        space_id.as_deref(),
    )
}

/// Inner implementation that applies remote CRDT changes to a database connection.
///
/// If `backend_info` is `Some((backend_id, max_hlc))`, updates `haex_sync_backends`
/// with the push HLC timestamp (used by server sync). For local delivery, pass `None`.
///
/// If `hlc_service` is provided, the local HLC clock is advanced past the highest
/// received remote timestamp after applying all changes. This ensures future local
/// operations generate timestamps strictly greater than any received remote timestamp,
/// preventing incomplete rows on the server during push.
///
/// Equivalent to [`apply_remote_changes_to_db_scoped`] with no expected
/// space. Callers that know which space the batch was pulled for should use
/// the scoped variant so column signatures on newly inserted rows can be
/// anchored — without it, such rows have no trustworthy `space_id` and their
/// signed changes are dropped rather than verified against a
/// batch-supplied (i.e. attacker-supplied) space.
pub fn apply_remote_changes_to_db(
    db: &crate::database::DbConnection,
    changes: Vec<RemoteColumnChange>,
    backend_info: Option<(&str, &str)>,
    hlc_service: Option<&HlcService>,
) -> Result<(), DatabaseError> {
    apply_remote_changes_to_db_scoped(db, changes, backend_info, hlc_service, None)
}

/// [`apply_remote_changes_to_db`] plus the space this batch was pulled for.
///
/// See [`resolve_row_space_id_for_sig`] for why the expected space matters:
/// it is the cross-check that stops a peer from choosing the space its own
/// signatures are verified under.
pub fn apply_remote_changes_to_db_scoped(
    db: &crate::database::DbConnection,
    changes: Vec<RemoteColumnChange>,
    backend_info: Option<(&str, &str)>,
    hlc_service: Option<&HlcService>,
    expected_space_id: Option<&str>,
) -> Result<(), DatabaseError> {
    eprintln!("[SYNC RUST] ========== APPLY REMOTE CHANGES START ==========");
    eprintln!(
        "[SYNC RUST] Changes count: {}, backend: {}",
        changes.len(),
        backend_info.map(|(id, _)| id).unwrap_or("local-delivery"),
    );

    // Identifier safety, HLC validity/drift and grouping are now entirely
    // the crate's own preflight_batch + engine — vault's copies of that
    // logic were fully redundant with it and are not ported.
    let crate_changes: Vec<haex_crdt::ColumnChange> = changes.iter().map(to_crate_change).collect();
    let mut policy = VaultApplyPolicy::new(expected_space_id, backend_info);

    with_connection(db, |conn| {
        // The crate's apply_remote_changes requires a real `&HlcService` to
        // advance the local clock past whatever lands. A caller that passes
        // `None` here deliberately wants NO real clock advanced (see this
        // function's doc — the lock-poisoned fallback and certain internal/
        // test callers rely on that). `HlcService::advance_past_remote` does
        // no I/O of its own: it only mutates its own owned, in-memory
        // `Mutex<Option<HLC>>`. A throwaway, already-initialized instance
        // built here and dropped at the end of this call is therefore
        // observably identical to "no clock advanced" — nothing persisted
        // ever sees it.
        let owned_hlc;
        let hlc_ref: &HlcService = match hlc_service {
            Some(hlc) => hlc,
            None => {
                owned_hlc = HlcService::new_with_uuid(uuid::Uuid::new_v4());
                &owned_hlc
            }
        };

        match haex_crdt::apply_remote_changes(conn, crate_changes, hlc_ref, &mut policy) {
            Ok(_outcome) => Ok(()),
            // The merge already committed; only the post-commit clock
            // advance failed. Preserve today's distinction: a malformed
            // max-HLC can't be fixed by retrying the same batch (log and
            // continue), anything else fails the apply so the pull cursor
            // doesn't advance and the (idempotent) batch is retried.
            Err(haex_crdt::Error::PostCommitClockAdvance { source, .. }) => match source {
                HlcError::Parse(_) => {
                    eprintln!(
                        "[SYNC RUST] CRITICAL: HLC advance skipped (unparseable max HLC): {source:?}"
                    );
                    Ok(())
                }
                other => Err(DatabaseError::DatabaseError {
                    reason: format!("HLC advance failed after apply: {other:?}"),
                }),
            },
            Err(other) => Err(DatabaseError::DatabaseError {
                reason: other.to_string(),
            }),
        }
    })
}

#[cfg(test)]
#[cfg_attr(test, allow(clippy::unwrap_used))]
mod tests {
    use super::*;
    use crate::crdt::column_sig::sign::sign_column;
    use crate::crdt::column_sig::value_bytes;
    use crate::database::migrations::{
        clear_pending_table_inner, get_recoverable_pending_tables_inner,
    };
    use crate::database::DbConnection;
    use std::sync::{Arc, Mutex};

    // Minimal apply harness: the CRDT configs table (for the triggers-enabled
    // toggle) + a target table with a NOT NULL no-default column (`space_id`)
    // next to a nullable one (`avatar`), mirroring haex_space_devices.
    fn setup_db() -> DbConnection {
        let conn = rusqlite::Connection::open_in_memory().unwrap();
        conn.execute_batch(&format!(
            "CREATE TABLE {TABLE_CRDT_CONFIGS} (key TEXT PRIMARY KEY, type TEXT, value TEXT);
             CREATE TABLE {DELETED_ROWS_TABLE} (
                 id TEXT PRIMARY KEY,
                 table_name TEXT NOT NULL,
                 row_pks TEXT NOT NULL,
                 haex_hlc_no_sync TEXT,
                 haex_column_hlcs_no_sync TEXT NOT NULL DEFAULT '{{}}'
             );
             CREATE TABLE devices (
                 id TEXT PRIMARY KEY,
                 space_id TEXT NOT NULL,
                 avatar TEXT,
                 haex_hlc_no_sync TEXT,
                 haex_column_hlcs_no_sync TEXT NOT NULL DEFAULT '{{}}'
             );"
        ))
        .unwrap();
        DbConnection(Arc::new(Mutex::new(Some(conn))))
    }

    // Extended harness that also creates the pending tables marker table.
    fn setup_db_with_pending_tables() -> DbConnection {
        let conn = rusqlite::Connection::open_in_memory().unwrap();
        conn.execute_batch(&format!(
            "CREATE TABLE {TABLE_CRDT_CONFIGS} (key TEXT PRIMARY KEY, type TEXT, value TEXT);
             CREATE TABLE {TABLE_CRDT_PENDING_TABLES} (table_name TEXT PRIMARY KEY NOT NULL);
             CREATE TABLE {DELETED_ROWS_TABLE} (
                 id TEXT PRIMARY KEY,
                 table_name TEXT NOT NULL,
                 row_pks TEXT NOT NULL,
                 haex_hlc_no_sync TEXT,
                 haex_column_hlcs_no_sync TEXT NOT NULL DEFAULT '{{}}'
             );"
        ))
        .unwrap();
        DbConnection(Arc::new(Mutex::new(Some(conn))))
    }

    fn change(pk: &str, col: &str, val: &str, hlc: &str) -> RemoteColumnChange {
        RemoteColumnChange {
            table_name: "devices".to_string(),
            row_pks: pk.to_string(),
            column_name: col.to_string(),
            hlc_timestamp: hlc.to_string(),
            decrypted_value: JsonValue::String(val.to_string()),
            sig: None,
        }
    }

    fn row_count(db: &DbConnection, where_sql: &str) -> i64 {
        let guard = db.0.lock().unwrap();
        let conn = guard.as_ref().unwrap();
        conn.query_row(
            &format!("SELECT COUNT(*) FROM devices WHERE {where_sql}"),
            [],
            |r| r.get(0),
        )
        .unwrap()
    }

    // Regression for the haex_space_devices sync wedge: a change set carrying
    // only a nullable column for a row that doesn't exist locally (its NOT NULL
    // creation columns are below the pull cursor, or the row is partial on the
    // leader) can never satisfy a partial INSERT. Throwing here wedged the
    // whole sync loop forever — the cursor never advanced and the same batch
    // was re-pulled every cycle. The apply must skip the row and succeed.
    #[test]
    fn partial_insert_missing_notnull_is_skipped_not_wedged() {
        let db = setup_db();
        let changes = vec![change(
            r#"{"id":"dev-1"}"#,
            "avatar",
            "face.png",
            "2/abcdef",
        )];

        let result = apply_remote_changes_to_db(&db, changes, None, None);

        assert!(
            result.is_ok(),
            "partial-column INSERT must not error the whole apply: {result:?}"
        );
        assert_eq!(
            row_count(&db, "id = 'dev-1'"),
            0,
            "row with a missing NOT NULL column must be skipped, not inserted"
        );
    }

    // The skip is surgical: a complete row in the same batch still applies and
    // the apply as a whole succeeds (so the sync cursor advances).
    #[test]
    fn complete_row_applies_while_partial_sibling_is_skipped() {
        let db = setup_db();
        let changes = vec![
            change(r#"{"id":"ok"}"#, "space_id", "s1", "1/abcdef"),
            change(r#"{"id":"ok"}"#, "avatar", "a.png", "1/abcdef"),
            change(r#"{"id":"bad"}"#, "avatar", "b.png", "2/abcdef"),
        ];

        let result = apply_remote_changes_to_db(&db, changes, None, None);

        assert!(result.is_ok(), "{result:?}");
        assert_eq!(
            row_count(&db, "id = 'ok'"),
            1,
            "complete row must be inserted"
        );
        assert_eq!(
            row_count(&db, "id = 'bad'"),
            0,
            "partial row must be skipped"
        );
    }

    // A change for a table that does not exist locally inserts a marker into
    // haex_crdt_pending_tables_no_sync, and the apply still returns Ok (the
    // sync cursor must advance past this batch).
    #[test]
    fn missing_table_inserts_pending_marker_and_returns_ok() {
        let db = setup_db_with_pending_tables();
        let changes = vec![RemoteColumnChange {
            table_name: "haex_ext_not_installed".to_string(),
            row_pks: r#"{"id":"row-1"}"#.to_string(),
            column_name: "value".to_string(),
            hlc_timestamp: "1/aabbcc".to_string(),
            decrypted_value: JsonValue::String("data".to_string()),
            sig: None,
        }];

        let result = apply_remote_changes_to_db(&db, changes, None, None);
        assert!(
            result.is_ok(),
            "apply must return Ok even when a table is missing: {result:?}"
        );

        let guard = db.0.lock().unwrap();
        let conn = guard.as_ref().unwrap();
        let count: i64 = conn
            .query_row(
                &format!(
                    "SELECT COUNT(*) FROM {} WHERE table_name = 'haex_ext_not_installed'",
                    TABLE_CRDT_PENDING_TABLES
                ),
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(
            count, 1,
            "pending-table marker must be inserted for the skipped table"
        );
    }

    // get_recoverable_pending_tables_inner returns a marker only once the table
    // exists locally; clear_pending_table_inner removes it.
    #[test]
    fn recoverable_pending_tables_filtered_by_existence_and_clearable() {
        let db = setup_db_with_pending_tables();

        let guard = db.0.lock().unwrap();
        let conn = guard.as_ref().unwrap();

        // Seed a marker for a table that does NOT yet exist.
        conn.execute(
            &format!(
                "INSERT INTO {} (table_name) VALUES ('haex_not_yet')",
                TABLE_CRDT_PENDING_TABLES
            ),
            [],
        )
        .unwrap();

        // Not recoverable yet — the table doesn't exist.
        let before = get_recoverable_pending_tables_inner(conn).unwrap();
        assert!(
            before.is_empty(),
            "marker for non-existent table must not be recoverable: {before:?}"
        );

        // Create the table locally (simulates extension install).
        conn.execute_batch("CREATE TABLE haex_not_yet (id TEXT PRIMARY KEY)")
            .unwrap();

        let after = get_recoverable_pending_tables_inner(conn).unwrap();
        assert_eq!(
            after,
            vec!["haex_not_yet".to_string()],
            "marker must be returned once the table exists locally"
        );

        // Clear the marker.
        clear_pending_table_inner(conn, "haex_not_yet").unwrap();

        let cleared = get_recoverable_pending_tables_inner(conn).unwrap();
        assert!(
            cleared.is_empty(),
            "marker must be gone after clear: {cleared:?}"
        );
    }

    // Regression: applying an older remote change to column B must not regress
    // the row's haex_hlc_no_sync below the current value (set by a newer local change
    // to column A). A regressed haex_hlc_no_sync would let an older remote delete win
    // against the newer local write (delete_propagation.rs resurrection check).
    #[test]
    fn row_hlc_never_regresses_when_applying_older_changes() {
        let db = setup_db();

        // Seed an existing row: column A written at T=10, column B written at T=3,
        // so the row's haex_hlc_no_sync is T=10.
        {
            let guard = db.0.lock().unwrap();
            let conn = guard.as_ref().unwrap();
            conn.execute(
                "INSERT INTO devices (id, space_id, avatar, haex_hlc_no_sync, haex_column_hlcs_no_sync) \
                 VALUES ('dev-1', 's1', 'old.png', '10/aaa', '{\"space_id\":\"10/aaa\",\"avatar\":\"3/aaa\"}')",
                [],
            )
            .unwrap();
        }

        // Apply a remote change to column B (avatar) with HLC T=5 — newer than
        // the column B's current HLC (T=3) so it applies, but older than the
        // row's current haex_hlc_no_sync (T=10).
        let changes = vec![change(r#"{"id":"dev-1"}"#, "avatar", "new.png", "5/aaa")];
        let result = apply_remote_changes_to_db(&db, changes, None, None);
        assert!(result.is_ok(), "apply must succeed: {result:?}");

        // Column B must carry the new value and its column HLC must be T=5.
        let (avatar_val, col_hlcs_str, row_hlc): (String, String, String) = {
            let guard = db.0.lock().unwrap();
            let conn = guard.as_ref().unwrap();
            conn.query_row(
                "SELECT avatar, haex_column_hlcs_no_sync, haex_hlc_no_sync FROM devices WHERE id = 'dev-1'",
                [],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
            )
            .unwrap()
        };
        assert_eq!(avatar_val, "new.png", "column B value must be updated");
        let col_hlcs: serde_json::Map<String, JsonValue> =
            serde_json::from_str(&col_hlcs_str).unwrap();
        assert_eq!(
            col_hlcs.get("avatar").and_then(|v| v.as_str()),
            Some("5/aaa"),
            "column B HLC must be T=5"
        );
        assert_eq!(
            row_hlc, "10/aaa",
            "row haex_hlc_no_sync must not regress below T=10 after applying an older T=5 change"
        );
    }

    // -----------------------------------------------------------------------
    // Runde-5 sig-verifier plumbing (Task G1c + G1d)
    // -----------------------------------------------------------------------

    use crate::ucan::verify::did_key_from_public_key;
    use ed25519_dalek::SigningKey;

    /// Extension of `setup_db()` that also has `haex_identities` so the
    /// Runde-5 `ensure_identity_stub` path has somewhere to insert into,
    /// and seeds a device row so sig verification can read its `space_id`.
    fn setup_db_with_identities() -> DbConnection {
        let conn = rusqlite::Connection::open_in_memory().unwrap();
        conn.execute_batch(&format!(
            "CREATE TABLE {TABLE_CRDT_CONFIGS} (key TEXT PRIMARY KEY, type TEXT, value TEXT);
             CREATE TABLE {DELETED_ROWS_TABLE} (
                 id TEXT PRIMARY KEY,
                 table_name TEXT NOT NULL,
                 row_pks TEXT NOT NULL,
                 haex_hlc_no_sync TEXT,
                 haex_column_hlcs_no_sync TEXT NOT NULL DEFAULT '{{}}'
             );
             -- `name TEXT NOT NULL` without a default mirrors migration 0000
             -- and is load-bearing: a stub INSERT that omits it is silently
             -- swallowed by `OR IGNORE`, which is the bug
             -- `apply_ensures_identity_stub_for_new_author_did` must be able
             -- to see.
             CREATE TABLE haex_identities (
                 id TEXT PRIMARY KEY NOT NULL,
                 did TEXT NOT NULL,
                 name TEXT NOT NULL,
                 source TEXT DEFAULT 'contact' NOT NULL
             );
             CREATE UNIQUE INDEX haex_identities_did_unique ON haex_identities (did);
             CREATE TABLE devices (
                 id TEXT PRIMARY KEY,
                 space_id TEXT NOT NULL,
                 avatar TEXT,
                 haex_hlc_no_sync TEXT,
                 haex_column_hlcs_no_sync TEXT NOT NULL DEFAULT '{{}}'
             );
             INSERT INTO devices (id, space_id, avatar, haex_hlc_no_sync, haex_column_hlcs_no_sync) \
              VALUES ('dev-1', 's1', 'old.png', '10/aaa', '{{\"space_id\":\"10/aaa\",\"avatar\":\"3/aaa\"}}');"
        ))
        .unwrap();
        DbConnection(Arc::new(Mutex::new(Some(conn))))
    }

    /// Runde 5 G1d — a change carrying a well-formed but invalid Ed25519
    /// signature is dropped from the batch (row-scoped rejection). The
    /// target row stays untouched and the invalid author DID is NOT stub-
    /// inserted into `haex_identities` (that path only fires for verified
    /// sigs — otherwise a peer could flood the table with attacker DIDs).
    ///
    /// The signature is produced by a real key over a *different* value
    /// than the one claimed in the change, so verification fails on
    /// `VerifyColumnSigError::InvalidSignature` — not on a malformed-DID
    /// short-circuit, which would make this test pass for the wrong reason.
    #[test]
    fn apply_rejects_change_with_invalid_signature() {
        let db = setup_db_with_identities();

        let seed: [u8; 32] = rand::random();
        let signing_key = SigningKey::from_bytes(&seed);
        let did = did_key_from_public_key(&signing_key.verifying_key());
        let space_id = "s1"; // seeded on the row in setup
        let hlc = "20/abc"; // > existing avatar HLC 3/aaa so it would apply if verified

        // Sign a *different* value than the one the change actually carries,
        // so the recomputed preimage on the verifier side never matches.
        let signed_value_bytes = value_bytes::to_canonical_bytes(&SqlValue::Text(
            "value-that-was-not-tampered.png".to_string(),
        ));
        let sig = sign_column(
            &signing_key,
            space_id.as_bytes(),
            b"devices",
            br#"{"id":"dev-1"}"#,
            b"avatar",
            hlc.as_bytes(),
            did.as_bytes(),
            &signed_value_bytes,
        );

        let change = RemoteColumnChange {
            table_name: "devices".to_string(),
            row_pks: r#"{"id":"dev-1"}"#.to_string(),
            column_name: "avatar".to_string(),
            hlc_timestamp: hlc.to_string(),
            decrypted_value: JsonValue::String("tampered.png".to_string()),
            sig: Some(ColumnSig {
                author_did: did.clone(),
                sig: BASE64.encode(sig.to_bytes()),
                storage_class: crate::crdt::column_sig::value_bytes::StorageClass::Text,
            }),
        };

        apply_remote_changes_to_db(&db, vec![change], None, None)
            .expect("apply must succeed (row-scoped rejection, not batch abort)");

        // The row's avatar must not have been overwritten.
        let avatar: String = {
            let guard = db.0.lock().unwrap();
            let conn = guard.as_ref().unwrap();
            conn.query_row("SELECT avatar FROM devices WHERE id = 'dev-1'", [], |r| {
                r.get(0)
            })
            .unwrap()
        };
        assert_eq!(
            avatar, "old.png",
            "invalid-sig change must be dropped, existing value preserved"
        );

        // No identity stub — invalid sigs must not seed `haex_identities`.
        let stub_count: i64 = {
            let guard = db.0.lock().unwrap();
            let conn = guard.as_ref().unwrap();
            conn.query_row(
                "SELECT COUNT(*) FROM haex_identities WHERE did = ?",
                [&did],
                |r| r.get(0),
            )
            .unwrap()
        };
        assert_eq!(
            stub_count, 0,
            "invalid-sig author DID must NOT get a stub row"
        );
    }

    #[test]
    fn apply_scoped_rejects_unsigned_shared_space_change() {
        let db = setup_db_with_identities();
        let change = RemoteColumnChange {
            table_name: "devices".to_string(),
            row_pks: r#"{"id":"dev-1"}"#.to_string(),
            column_name: "avatar".to_string(),
            hlc_timestamp: "20/abc".to_string(),
            decrypted_value: JsonValue::String("unsigned.png".to_string()),
            sig: None,
        };

        apply_remote_changes_to_db_scoped(&db, vec![change], None, None, Some("s1"))
            .expect("unsigned rejection is row-scoped");

        let avatar: String = {
            let guard = db.0.lock().unwrap();
            let conn = guard.as_ref().unwrap();
            conn.query_row("SELECT avatar FROM devices WHERE id = 'dev-1'", [], |r| {
                r.get(0)
            })
            .unwrap()
        };
        assert_eq!(avatar, "old.png");
    }

    /// Runde 5 G1c — a change with a **valid** sig from a DID we've never
    /// seen locally triggers `ensure_identity_stub`, which INSERT OR IGNOREs
    /// a row into `haex_identities` so downstream FKs / joins can bind. The
    /// change itself also lands (updates the row).
    #[test]
    fn apply_ensures_identity_stub_for_new_author_did() {
        let db = setup_db_with_identities();

        let seed: [u8; 32] = rand::random();
        let signing_key = SigningKey::from_bytes(&seed);
        let did = did_key_from_public_key(&signing_key.verifying_key());
        let space_id = "s1"; // seeded on the row in setup
        let new_avatar = "verified.png";
        let hlc = "20/abc";

        let value_bytes_vec =
            value_bytes::to_canonical_bytes(&SqlValue::Text(new_avatar.to_string()));
        let sig = sign_column(
            &signing_key,
            space_id.as_bytes(),
            b"devices",
            br#"{"id":"dev-1"}"#,
            b"avatar",
            hlc.as_bytes(),
            did.as_bytes(),
            &value_bytes_vec,
        );

        let change = RemoteColumnChange {
            table_name: "devices".to_string(),
            row_pks: r#"{"id":"dev-1"}"#.to_string(),
            column_name: "avatar".to_string(),
            hlc_timestamp: hlc.to_string(),
            decrypted_value: JsonValue::String(new_avatar.to_string()),
            sig: Some(ColumnSig {
                author_did: did.clone(),
                sig: BASE64.encode(sig.to_bytes()),
                storage_class: crate::crdt::column_sig::value_bytes::StorageClass::Text,
            }),
        };

        apply_remote_changes_to_db(&db, vec![change], None, None).expect("apply must succeed");

        let stub_count: i64 = {
            let guard = db.0.lock().unwrap();
            let conn = guard.as_ref().unwrap();
            conn.query_row(
                "SELECT COUNT(*) FROM haex_identities WHERE did = ?",
                [&did],
                |r| r.get(0),
            )
            .unwrap()
        };
        assert_eq!(
            stub_count, 1,
            "verified-sig new author DID must produce exactly one stub"
        );

        // And the change itself must have landed.
        let avatar: String = {
            let guard = db.0.lock().unwrap();
            let conn = guard.as_ref().unwrap();
            conn.query_row("SELECT avatar FROM devices WHERE id = 'dev-1'", [], |r| {
                r.get(0)
            })
            .unwrap()
        };
        assert_eq!(avatar, new_avatar);
    }

    /// Helper: sign `value` for `space_id` on `devices.avatar` at `hlc` and
    /// wrap it in a ready-to-apply `RemoteColumnChange`.
    fn signed_avatar_change(
        signing_key: &SigningKey,
        space_id: &str,
        value: &str,
        hlc: &str,
    ) -> RemoteColumnChange {
        let did = did_key_from_public_key(&signing_key.verifying_key());
        let value_bytes_vec = value_bytes::to_canonical_bytes(&SqlValue::Text(value.to_string()));
        let sig = sign_column(
            signing_key,
            space_id.as_bytes(),
            b"devices",
            br#"{"id":"dev-1"}"#,
            b"avatar",
            hlc.as_bytes(),
            did.as_bytes(),
            &value_bytes_vec,
        );
        RemoteColumnChange {
            table_name: "devices".to_string(),
            row_pks: r#"{"id":"dev-1"}"#.to_string(),
            column_name: "avatar".to_string(),
            hlc_timestamp: hlc.to_string(),
            decrypted_value: JsonValue::String(value.to_string()),
            sig: Some(ColumnSig {
                author_did: did,
                sig: BASE64.encode(sig.to_bytes()),
                storage_class: crate::crdt::column_sig::value_bytes::StorageClass::Text,
            }),
        }
    }

    fn read_avatar(db: &DbConnection) -> String {
        let guard = db.0.lock().unwrap();
        let conn = guard.as_ref().unwrap();
        conn.query_row("SELECT avatar FROM devices WHERE id = 'dev-1'", [], |r| {
            r.get(0)
        })
        .unwrap()
    }

    /// The verification anchor must be the row's PERSISTED `space_id`, never
    /// a `space_id` column change riding along in the same batch.
    ///
    /// Attack shape: a peer holds a signing key for its own space `s_evil`
    /// and pushes `{space_id: "s_evil", avatar: "pwned.png"}` at `dev-1`,
    /// which locally belongs to `s1`. If the resolver preferred the batch's
    /// claim, both changes would verify under `s_evil` and the avatar update
    /// would land on the victim row — the space binding in the preimage
    /// (ADR 0002 §4b) defeated because the attacker chose the binding.
    #[test]
    fn apply_anchors_sig_on_persisted_space_id_not_batch_claim() {
        let db = setup_db_with_identities();

        let seed: [u8; 32] = rand::random();
        let attacker_key = SigningKey::from_bytes(&seed);
        let hlc = "20/abc";

        // Both changes are correctly signed — but for `s_evil`, not `s1`.
        let avatar_change = signed_avatar_change(&attacker_key, "s_evil", "pwned.png", hlc);
        let mut space_change = signed_avatar_change(&attacker_key, "s_evil", "s_evil", hlc);
        space_change.column_name = "space_id".to_string();

        apply_remote_changes_to_db(&db, vec![space_change, avatar_change], None, None)
            .expect("apply must succeed — rejection is column-scoped, not fatal");

        assert_eq!(
            read_avatar(&db),
            "old.png",
            "sig signed for a foreign space must not verify against the row's own space"
        );
    }

    /// Same row, same attacker key, but this time signed for the row's real
    /// space `s1` — proving the test above fails for the right reason (wrong
    /// space) rather than because the harness is broken.
    #[test]
    fn apply_accepts_sig_matching_persisted_space_id() {
        let db = setup_db_with_identities();

        let seed: [u8; 32] = rand::random();
        let key = SigningKey::from_bytes(&seed);
        let mut space_change = signed_avatar_change(&key, "s1", "s_evil", "20/abc");
        space_change.column_name = "space_id".to_string();
        let avatar_change = signed_avatar_change(&key, "s1", "accepted.png", "20/abc");

        apply_remote_changes_to_db(&db, vec![space_change, avatar_change], None, None)
            .expect("apply must succeed");

        assert_eq!(read_avatar(&db), "accepted.png");
    }

    /// A row that does not exist locally has no persisted anchor. Without an
    /// `expected_space_id` from the caller there is no way to tell an honest
    /// claim from a forged one, so signed changes are dropped rather than
    /// verified against whatever the batch asserts.
    #[test]
    fn apply_drops_signed_insert_when_no_expected_space_is_given() {
        let db = setup_db_with_identities();

        let seed: [u8; 32] = rand::random();
        let key = SigningKey::from_bytes(&seed);
        let mut space_change = signed_avatar_change(&key, "s_new", "s_new", "20/abc");
        space_change.column_name = "space_id".to_string();
        let mut avatar_change = signed_avatar_change(&key, "s_new", "new-row.png", "20/abc");
        avatar_change.row_pks = r#"{"id":"dev-2"}"#.to_string();
        space_change.row_pks = r#"{"id":"dev-2"}"#.to_string();

        apply_remote_changes_to_db(&db, vec![space_change, avatar_change], None, None)
            .expect("apply must succeed");

        let count: i64 = {
            let guard = db.0.lock().unwrap();
            let conn = guard.as_ref().unwrap();
            conn.query_row("SELECT COUNT(*) FROM devices WHERE id = 'dev-2'", [], |r| {
                r.get(0)
            })
            .unwrap()
        };
        assert_eq!(
            count, 0,
            "unanchored signed insert must be dropped, not applied unverified"
        );
    }

    /// With the pull scope supplied, a signed INSERT whose claimed `space_id`
    /// agrees with that scope verifies and lands.
    #[test]
    fn apply_scoped_accepts_signed_insert_matching_expected_space() {
        let db = setup_db_with_identities();

        let seed: [u8; 32] = rand::random();
        let key = SigningKey::from_bytes(&seed);
        // NOTE: row_pks must match what was signed, so build then retarget
        // both changes consistently before signing is irrelevant — the pks
        // are part of the preimage, so sign for dev-2 directly.
        let did = did_key_from_public_key(&key.verifying_key());
        let hlc = "20/abc";
        let mk = |column: &str, value: &str| {
            let vb = value_bytes::to_canonical_bytes(&SqlValue::Text(value.to_string()));
            let sig = sign_column(
                &key,
                b"s_new",
                b"devices",
                br#"{"id":"dev-2"}"#,
                column.as_bytes(),
                hlc.as_bytes(),
                did.as_bytes(),
                &vb,
            );
            RemoteColumnChange {
                table_name: "devices".to_string(),
                row_pks: r#"{"id":"dev-2"}"#.to_string(),
                column_name: column.to_string(),
                hlc_timestamp: hlc.to_string(),
                decrypted_value: JsonValue::String(value.to_string()),
                sig: Some(ColumnSig {
                    author_did: did.clone(),
                    sig: BASE64.encode(sig.to_bytes()),
                    storage_class: crate::crdt::column_sig::value_bytes::StorageClass::Text,
                }),
            }
        };

        apply_remote_changes_to_db_scoped(
            &db,
            vec![mk("space_id", "s_new"), mk("avatar", "new-row.png")],
            None,
            None,
            Some("s_new"),
        )
        .expect("apply must succeed");

        let avatar: Option<String> = {
            let guard = db.0.lock().unwrap();
            let conn = guard.as_ref().unwrap();
            conn.query_row("SELECT avatar FROM devices WHERE id = 'dev-2'", [], |r| {
                r.get(0)
            })
            .ok()
        };
        assert_eq!(avatar.as_deref(), Some("new-row.png"));
    }

    /// The cross-check bites: a signed INSERT claiming a space other than the
    /// one the pull was scoped to is refused even though its signature is
    /// internally consistent.
    #[test]
    fn apply_scoped_rejects_signed_insert_claiming_a_different_space() {
        let db = setup_db_with_identities();

        let seed: [u8; 32] = rand::random();
        let key = SigningKey::from_bytes(&seed);
        let did = did_key_from_public_key(&key.verifying_key());
        let hlc = "20/abc";
        let mk = |column: &str, value: &str| {
            let vb = value_bytes::to_canonical_bytes(&SqlValue::Text(value.to_string()));
            let sig = sign_column(
                &key,
                b"s_evil",
                b"devices",
                br#"{"id":"dev-2"}"#,
                column.as_bytes(),
                hlc.as_bytes(),
                did.as_bytes(),
                &vb,
            );
            RemoteColumnChange {
                table_name: "devices".to_string(),
                row_pks: r#"{"id":"dev-2"}"#.to_string(),
                column_name: column.to_string(),
                hlc_timestamp: hlc.to_string(),
                decrypted_value: JsonValue::String(value.to_string()),
                sig: Some(ColumnSig {
                    author_did: did.clone(),
                    sig: BASE64.encode(sig.to_bytes()),
                    storage_class: crate::crdt::column_sig::value_bytes::StorageClass::Text,
                }),
            }
        };

        apply_remote_changes_to_db_scoped(
            &db,
            vec![mk("space_id", "s_evil"), mk("avatar", "new-row.png")],
            None,
            None,
            Some("s_expected"),
        )
        .expect("apply must succeed");

        let count: i64 = {
            let guard = db.0.lock().unwrap();
            let conn = guard.as_ref().unwrap();
            conn.query_row("SELECT COUNT(*) FROM devices WHERE id = 'dev-2'", [], |r| {
                r.get(0)
            })
            .unwrap()
        };
        assert_eq!(count, 0, "space-mismatched signed insert must be dropped");
    }

    // -----------------------------------------------------------------------
    // Owner-space trust: unsigned changes must land when the expected space
    // is this vault's own owner-space (VAULT-type row in haex_spaces). See
    // owner_sync::scope::is_owner_space for the rationale.
    // -----------------------------------------------------------------------

    /// Test schema mirroring `setup_db_with_identities` but *with* a
    /// `haex_spaces` row of `type='vault'` so `is_owner_space` can resolve.
    /// `owner_space_id` is the vault-space id.
    fn setup_db_with_owner_space(owner_space_id: &str) -> DbConnection {
        let conn = rusqlite::Connection::open_in_memory().unwrap();
        conn.execute_batch(&format!(
            "CREATE TABLE {TABLE_CRDT_CONFIGS} (key TEXT PRIMARY KEY, type TEXT, value TEXT);
             CREATE TABLE {DELETED_ROWS_TABLE} (
                 id TEXT PRIMARY KEY,
                 table_name TEXT NOT NULL,
                 row_pks TEXT NOT NULL,
                 haex_hlc_no_sync TEXT,
                 haex_column_hlcs_no_sync TEXT NOT NULL DEFAULT '{{}}'
             );
             CREATE TABLE haex_identities (
                 id TEXT PRIMARY KEY NOT NULL,
                 did TEXT NOT NULL,
                 name TEXT NOT NULL,
                 source TEXT DEFAULT 'contact' NOT NULL
             );
             CREATE UNIQUE INDEX haex_identities_did_unique ON haex_identities (did);
             CREATE TABLE haex_spaces (
                 id TEXT PRIMARY KEY,
                 type TEXT NOT NULL,
                 owner_identity_id TEXT
             );
             CREATE TABLE devices (
                 id TEXT PRIMARY KEY,
                 space_id TEXT NOT NULL,
                 avatar TEXT,
                 haex_hlc_no_sync TEXT,
                 haex_column_hlcs_no_sync TEXT NOT NULL DEFAULT '{{}}'
             );
             INSERT INTO haex_identities (id, did, name) VALUES ('id-owner', 'did:key:zOwner', 'owner');
             INSERT INTO haex_spaces (id, type, owner_identity_id) VALUES ('{owner_space_id}', 'vault', 'id-owner');
             INSERT INTO devices (id, space_id, avatar, haex_hlc_no_sync, haex_column_hlcs_no_sync) \
              VALUES ('dev-1', '{owner_space_id}', 'old.png', '10/aaa', '{{\"space_id\":\"10/aaa\",\"avatar\":\"3/aaa\"}}');"
        ))
        .unwrap();
        DbConnection(Arc::new(Mutex::new(Some(conn))))
    }

    /// Owner-sync between two devices of the same identity carries
    /// `expected_space_id` = the vault-space id, but the wire payload is
    /// unsigned by design (`sign_column_for_spaces` yields `{}` for owner-
    /// private rows). Before the trust-own-vault gate, this batch was
    /// silently dropped and every owner-private CRDT row failed to converge
    /// (see the failing e2e test
    /// `tests/sync/owner-sync-delete-convergence.spec.ts`).
    #[test]
    fn apply_remote_changes_to_db_scoped_accepts_unsigned_when_expected_space_is_owner_space() {
        let owner_space = "vault-owner-space";
        let db = setup_db_with_owner_space(owner_space);

        let change = RemoteColumnChange {
            table_name: "devices".to_string(),
            row_pks: r#"{"id":"dev-1"}"#.to_string(),
            column_name: "avatar".to_string(),
            hlc_timestamp: "20/abc".to_string(),
            decrypted_value: JsonValue::String("unsigned-owner.png".to_string()),
            sig: None,
        };

        apply_remote_changes_to_db_scoped(&db, vec![change], None, None, Some(owner_space))
            .expect("apply must succeed for owner-space unsigned change");

        let avatar: String = {
            let guard = db.0.lock().unwrap();
            let conn = guard.as_ref().unwrap();
            conn.query_row("SELECT avatar FROM devices WHERE id = 'dev-1'", [], |r| {
                r.get(0)
            })
            .unwrap()
        };
        assert_eq!(
            avatar, "unsigned-owner.png",
            "unsigned owner-space change must land: sig enforcement is off on the owner-space route"
        );
    }

    /// Regression guard: the "shared-space unsigned change is dropped"
    /// semantic must survive even with the owner-space gate in place. When
    /// `expected_space_id` points to a space that is NOT this vault's
    /// owner-space (either a shared space id or an id that does not exist in
    /// `haex_spaces` at all), the strict Phase-1 gate stays on.
    #[test]
    fn apply_remote_changes_to_db_scoped_rejects_unsigned_when_expected_space_is_shared() {
        let owner_space = "vault-owner-space";
        let db = setup_db_with_owner_space(owner_space);

        // Shared space id (any id != the vault-space id). No row for it needs
        // to exist in haex_spaces — is_owner_space is a positive check on the
        // vault-space row, everything else is "not owner-space" → enforce.
        let shared_space = "shared-space-abc";

        let change = RemoteColumnChange {
            table_name: "devices".to_string(),
            row_pks: r#"{"id":"dev-1"}"#.to_string(),
            column_name: "avatar".to_string(),
            hlc_timestamp: "20/abc".to_string(),
            decrypted_value: JsonValue::String("dropped.png".to_string()),
            sig: None,
        };

        apply_remote_changes_to_db_scoped(&db, vec![change], None, None, Some(shared_space))
            .expect("apply must succeed — rejection is column-scoped, not fatal");

        let avatar: String = {
            let guard = db.0.lock().unwrap();
            let conn = guard.as_ref().unwrap();
            conn.query_row("SELECT avatar FROM devices WHERE id = 'dev-1'", [], |r| {
                r.get(0)
            })
            .unwrap()
        };
        assert_eq!(
            avatar, "old.png",
            "unsigned shared-space change must be dropped: sig enforcement stays on for non-owner spaces"
        );
    }

    // -----------------------------------------------------------------------
    // Adversarial apply-pipeline hardening (security review follow-up).
    //
    // Findings, verified directly against this file's logic:
    //
    //  - CRDT conflict-resolution rule: per-COLUMN Hybrid-Logical-Clock
    //    last-write-wins with a STRICT greater-than gate (`hlc_is_newer`,
    //    `crdt::hlc`). Equal or older claimed HLCs never overwrite — this is
    //    NOT "last received wins": a stale/replayed op received after a
    //    newer one has already landed is silently dropped, not blindly
    //    applied. See `apply_v10_then_receiving_stale_v7_does_not_roll_back`.
    //
    //  - Forged HLC handling: REJECT, not "accept but reorder". A signed
    //    column change's Ed25519 preimage includes the claimed
    //    `hlc_timestamp` bytes (`verify_change_sig` -> `build_preimage`), so
    //    an attacker who takes a validly-signed change and swaps only the
    //    wire `hlc_timestamp` (e.g. to force a false LWW win) invalidates
    //    the signature. See `apply_rejects_change_with_forged_hlc_timestamp`.
    // -----------------------------------------------------------------------

    /// Scenario 1a: replaying an identical, unsigned change set (INSERT +
    /// follow-up UPDATE-shaped re-delivery) must be a no-op the second time —
    /// same value, same per-column HLCs, no duplicate row. Simulates a
    /// network/server re-sending an already-accepted push.
    #[test]
    fn apply_is_idempotent_when_identical_unsigned_change_set_is_applied_twice() {
        let db = setup_db();
        let changes = || {
            vec![
                change(r#"{"id":"dev-idem"}"#, "space_id", "s1", "5/aaa"),
                change(r#"{"id":"dev-idem"}"#, "avatar", "first.png", "5/aaa"),
            ]
        };

        apply_remote_changes_to_db(&db, changes(), None, None).expect("first apply must succeed");
        let read_state = |db: &DbConnection| -> (String, String, String) {
            let guard = db.0.lock().unwrap();
            let conn = guard.as_ref().unwrap();
            conn.query_row(
                "SELECT avatar, haex_column_hlcs_no_sync, haex_hlc_no_sync FROM devices WHERE id = 'dev-idem'",
                [],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
            )
            .unwrap()
        };
        let after_first = read_state(&db);

        // Replay: the exact same wire batch delivered a second time.
        apply_remote_changes_to_db(&db, changes(), None, None)
            .expect("replay of an already-applied batch must not error");
        let after_replay = read_state(&db);

        assert_eq!(
            after_first, after_replay,
            "replaying an identical change set must not change avatar/column-HLCs/row-HLC"
        );
        assert_eq!(
            row_count(&db, "id = 'dev-idem'"),
            1,
            "replay must not create a duplicate row"
        );
    }

    /// Scenario 1b: replaying an identical, validly-signed change must also
    /// be idempotent at the signature-verification layer — in particular it
    /// must not seed a second `haex_identities` stub for the same author DID
    /// (ties I2/I7 together: authenticity + replay resistance).
    #[test]
    fn apply_is_idempotent_when_signed_change_is_replayed() {
        let db = setup_db_with_identities();

        let seed: [u8; 32] = rand::random();
        let signing_key = SigningKey::from_bytes(&seed);
        let did = did_key_from_public_key(&signing_key.verifying_key());
        let space_id = "s1"; // seeded on the row in setup
        let new_avatar = "replayed.png";
        let hlc = "20/abc";

        let make_change = || signed_avatar_change(&signing_key, space_id, new_avatar, hlc);

        apply_remote_changes_to_db(&db, vec![make_change()], None, None)
            .expect("first apply of a validly-signed change must succeed");
        // Replay: the identical signed wire message delivered a second time
        // — e.g. a malicious or buggy server re-sending an already-accepted
        // push.
        apply_remote_changes_to_db(&db, vec![make_change()], None, None)
            .expect("replay of an already-verified signed change must not error");

        assert_eq!(read_avatar(&db), new_avatar);

        let stub_count: i64 = {
            let guard = db.0.lock().unwrap();
            let conn = guard.as_ref().unwrap();
            conn.query_row(
                "SELECT COUNT(*) FROM haex_identities WHERE did = ?",
                [&did],
                |r| r.get(0),
            )
            .unwrap()
        };
        assert_eq!(
            stub_count, 1,
            "replaying an already-verified signed change must not duplicate the identity stub"
        );
    }

    /// Scenario 2: a change carrying a genuinely valid Ed25519 signature
    /// (Alice really signed this exact preimage) but whose wire envelope
    /// claims a DIFFERENT author DID (Bob's) must be rejected. The attacker
    /// does not hold Bob's private key — they are attempting to relabel an
    /// honestly-signed operation as someone else's. `author_did` is itself
    /// part of the signed preimage (`build_preimage`) and also selects which
    /// public key verification uses, so this fails on both counts.
    #[test]
    fn apply_rejects_change_with_forged_author_did() {
        let db = setup_db_with_identities();

        let alice_seed: [u8; 32] = rand::random();
        let alice_key = SigningKey::from_bytes(&alice_seed);
        let alice_did = did_key_from_public_key(&alice_key.verifying_key());

        // Bob is an unrelated identity the attacker wants to frame — the
        // attacker never touches bob's private key.
        let bob_seed: [u8; 32] = rand::random();
        let bob_key = SigningKey::from_bytes(&bob_seed);
        let bob_did = did_key_from_public_key(&bob_key.verifying_key());

        let space_id = "s1"; // seeded on the row in setup
        let hlc = "20/abc";
        let new_avatar = "framed.png";

        // Alice signs honestly, over her own DID — a completely legitimate
        // signature for a completely legitimate change.
        let mut change = signed_avatar_change(&alice_key, space_id, new_avatar, hlc);
        // Attacker relabels the wire envelope's author_did to Bob's,
        // keeping Alice's genuine signature bytes untouched.
        change.sig.as_mut().unwrap().author_did = bob_did.clone();

        apply_remote_changes_to_db(&db, vec![change], None, None)
            .expect("apply must succeed — rejection is column-scoped, not fatal");

        assert_eq!(
            read_avatar(&db),
            "old.png",
            "author-forged change must be dropped, existing value preserved"
        );

        let stub_count: i64 = {
            let guard = db.0.lock().unwrap();
            let conn = guard.as_ref().unwrap();
            conn.query_row(
                "SELECT COUNT(*) FROM haex_identities WHERE did IN (?, ?)",
                [&alice_did, &bob_did],
                |r| r.get(0),
            )
            .unwrap()
        };
        assert_eq!(
            stub_count, 0,
            "a rejected forged-author change must not seed either identity"
        );
    }

    /// Scenario 5: a change carrying a valid signature for HLC `H`, but
    /// whose wire envelope claims a different (here: far-future, LWW-
    /// winning) HLC `H'`. Finding: this codebase's answer is REJECT, not
    /// "accept but order correctly" — `hlc_timestamp` bytes are part of the
    /// signed preimage (`verify_change_sig` -> `build_preimage`), so
    /// swapping the claimed HLC without re-signing invalidates the
    /// signature and the change is dropped before it can win any LWW race.
    #[test]
    fn apply_rejects_change_with_forged_hlc_timestamp() {
        let db = setup_db_with_identities();

        let seed: [u8; 32] = rand::random();
        let signing_key = SigningKey::from_bytes(&seed);
        let space_id = "s1"; // seeded on the row in setup
        let signed_hlc = "20/abc"; // what was actually signed
        let claimed_hlc = "999999/abc"; // forged: far future, would win any LWW race
        let new_avatar = "forged-time.png";

        // Attacker takes a legitimately-signed change and swaps ONLY the
        // claimed hlc_timestamp on the wire, hoping the inflated HLC wins
        // the per-column LWW race without needing a fresh signature.
        let mut change = signed_avatar_change(&signing_key, space_id, new_avatar, signed_hlc);
        change.hlc_timestamp = claimed_hlc.to_string();

        apply_remote_changes_to_db(&db, vec![change], None, None)
            .expect("apply must succeed — rejection is column-scoped, not fatal");

        assert_eq!(
            read_avatar(&db),
            "old.png",
            "a claimed HLC that does not match what was actually signed must be rejected \
             (hlc_timestamp is part of the signed preimage) — the codebase's handling of a \
             forged HLC is REJECT, not accept-and-reorder"
        );
    }

    /// Scenario 4: two SEPARATE apply() calls on the same column — a newer
    /// value (HLC 10) followed by a stale one (HLC 7) arriving afterwards
    /// (out-of-order delivery, or a lagging/malicious server replaying an
    /// old push). The actual CRDT rule is per-column HLC last-write-wins
    /// with a strict greater-than gate, NOT "last received wins": the stale
    /// op must lose even though it is the one most recently delivered.
    #[test]
    fn apply_v10_then_receiving_stale_v7_does_not_roll_back() {
        let db = setup_db();
        {
            let guard = db.0.lock().unwrap();
            let conn = guard.as_ref().unwrap();
            conn.execute(
                "INSERT INTO devices (id, space_id, avatar, haex_hlc_no_sync, haex_column_hlcs_no_sync) \
                 VALUES ('dev-1', 's1', 'seed.png', '0/aaa', '{}')",
                [],
            )
            .unwrap();
        }

        apply_remote_changes_to_db(
            &db,
            vec![change(r#"{"id":"dev-1"}"#, "avatar", "v10", "10/aaa")],
            None,
            None,
        )
        .expect("v10 must apply");

        // Stale v7 delivered AFTER v10 was already applied and committed.
        apply_remote_changes_to_db(
            &db,
            vec![change(r#"{"id":"dev-1"}"#, "avatar", "v7", "7/aaa")],
            None,
            None,
        )
        .expect("stale delivery must not error — rejection is silent/row-scoped");

        let (avatar, row_hlc): (String, String) = {
            let guard = db.0.lock().unwrap();
            let conn = guard.as_ref().unwrap();
            conn.query_row(
                "SELECT avatar, haex_hlc_no_sync FROM devices WHERE id = 'dev-1'",
                [],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .unwrap()
        };
        assert_eq!(
            avatar, "v10",
            "a stale (older-HLC) op received after a newer one must not roll back the value"
        );
        assert_eq!(
            row_hlc, "10/aaa",
            "row HLC must not regress from a stale delivery"
        );
    }

    /// Scenario 3: a fixed set of 3 independent column-updates to the SAME
    /// field, delivered via 3 separate apply() calls in every one of the
    /// 3! = 6 possible orders (simulating out-of-order network delivery).
    /// Every ordering must converge to the same final state — the
    /// highest-HLC write wins regardless of delivery order, never
    /// "whichever happened to arrive last physically".
    #[test]
    fn apply_converges_to_same_state_regardless_of_delivery_order() {
        // (value, hlc) triples — deliberately NOT HLC-sorted in this list.
        let op_specs = [("v10", "10/aaa"), ("v30", "30/aaa"), ("v20", "20/aaa")];
        let orderings: [[usize; 3]; 6] = [
            [0, 1, 2],
            [0, 2, 1],
            [1, 0, 2],
            [1, 2, 0],
            [2, 0, 1],
            [2, 1, 0],
        ];

        let mut results: Vec<(String, String, String)> = Vec::new();
        for order in orderings {
            let db = setup_db();
            {
                let guard = db.0.lock().unwrap();
                let conn = guard.as_ref().unwrap();
                conn.execute(
                    "INSERT INTO devices (id, space_id, avatar, haex_hlc_no_sync, haex_column_hlcs_no_sync) \
                     VALUES ('dev-1', 's1', 'seed.png', '0/aaa', '{}')",
                    [],
                )
                .unwrap();
            }
            for i in order {
                let (val, hlc) = op_specs[i];
                apply_remote_changes_to_db(
                    &db,
                    vec![change(r#"{"id":"dev-1"}"#, "avatar", val, hlc)],
                    None,
                    None,
                )
                .expect("each individual delivery must succeed");
            }
            let state: (String, String, String) = {
                let guard = db.0.lock().unwrap();
                let conn = guard.as_ref().unwrap();
                conn.query_row(
                    "SELECT avatar, haex_column_hlcs_no_sync, haex_hlc_no_sync FROM devices WHERE id = 'dev-1'",
                    [],
                    |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
                )
                .unwrap()
            };
            results.push(state);
        }

        let baseline = results[0].clone();
        assert_eq!(
            baseline.0, "v30",
            "the highest-HLC write (30/aaa) must win regardless of delivery order"
        );
        for (i, r) in results.iter().enumerate() {
            assert_eq!(
                r, &baseline,
                "delivery order {:?} produced a different final state than order {:?} — \
                 the CRDT merge must be commutative",
                orderings[i], orderings[0]
            );
        }
    }

    // ------------------------------------------------------------------
    // Task 5 regression: a shared-space delete-log claim that this
    // batch's own policy rejects must not propagate, but a replay of an
    // already-admitted one that only lost LWW (Stale) still must —
    // mirroring haex-crdt's own
    // `rejected_delete_replay_does_not_delete_but_admitted_stale_replay_does`
    // at vault's per-space layer.
    // ------------------------------------------------------------------

    fn setup_shared_space_delete_db() -> DbConnection {
        let conn = rusqlite::Connection::open_in_memory().unwrap();
        conn.execute_batch(&format!(
            "CREATE TABLE {TABLE_CRDT_CONFIGS} (key TEXT PRIMARY KEY, type TEXT, value TEXT);
             CREATE TABLE {DELETED_ROWS_TABLE} (
                 id TEXT PRIMARY KEY, table_name TEXT NOT NULL, row_pks TEXT NOT NULL,
                 haex_hlc_no_sync TEXT, haex_column_hlcs_no_sync TEXT NOT NULL DEFAULT '{{}}'
             );
             CREATE TABLE {SHARED_SPACE_DELETED_ROWS_TABLE} (
                 id TEXT PRIMARY KEY NOT NULL,
                 space_id TEXT NOT NULL,
                 table_name TEXT NOT NULL,
                 row_pks TEXT NOT NULL,
                 haex_hlc_no_sync TEXT,
                 haex_column_hlcs_no_sync TEXT NOT NULL DEFAULT '{{}}'
             );
             CREATE TABLE {SHARED_SPACE_SYNC_TABLE} (
                 id TEXT PRIMARY KEY NOT NULL,
                 table_name TEXT NOT NULL,
                 row_pks TEXT NOT NULL,
                 space_id TEXT NOT NULL,
                 haex_hlc_no_sync TEXT
             );
             CREATE TABLE ext_items (
                 id TEXT PRIMARY KEY NOT NULL,
                 body TEXT,
                 haex_hlc_no_sync TEXT,
                 haex_column_hlcs_no_sync TEXT NOT NULL DEFAULT '{{}}',
                 haex_column_sigs_no_sync TEXT NOT NULL DEFAULT '{{}}'
             );"
        ))
        .unwrap();
        DbConnection(Arc::new(Mutex::new(Some(conn))))
    }

    fn seed_shared_delete_scenario(
        db: &DbConnection,
        business_hlc: &str,
        delete_log_hlc: &str,
        delete_log_column_hlcs: &str,
    ) {
        let guard = db.0.lock().unwrap();
        let conn = guard.as_ref().unwrap();
        conn.execute(
            "INSERT INTO ext_items (id, body, haex_hlc_no_sync) VALUES ('item-1', 'kept', ?1)",
            [business_hlc],
        )
        .unwrap();
        conn.execute(
            &format!(
                "INSERT INTO {SHARED_SPACE_SYNC_TABLE} (id, table_name, row_pks, space_id, haex_hlc_no_sync) \
                 VALUES ('reg-1', 'ext_items', '{{\"id\":\"item-1\"}}', 'space-x', '1/aaa')"
            ),
            [],
        )
        .unwrap();
        conn.execute(
            &format!(
                "INSERT INTO {SHARED_SPACE_DELETED_ROWS_TABLE} \
                 (id, space_id, table_name, row_pks, haex_hlc_no_sync, haex_column_hlcs_no_sync) \
                 VALUES ('del-1', 'space-x', 'ext_items', '{{\"id\":\"item-1\"}}', ?1, ?2)"
            ),
            rusqlite::params![delete_log_hlc, delete_log_column_hlcs],
        )
        .unwrap();
    }

    fn item_one_exists(db: &DbConnection) -> bool {
        let guard = db.0.lock().unwrap();
        let conn = guard.as_ref().unwrap();
        conn.query_row(
            "SELECT COUNT(*) FROM ext_items WHERE id = 'item-1'",
            [],
            |r| r.get::<_, i64>(0),
        )
        .unwrap()
            > 0
    }

    fn shared_delete_replay_change(hlc: &str) -> RemoteColumnChange {
        RemoteColumnChange {
            table_name: SHARED_SPACE_DELETED_ROWS_TABLE.to_string(),
            row_pks: r#"{"id":"del-1"}"#.to_string(),
            column_name: "table_name".to_string(),
            hlc_timestamp: hlc.to_string(),
            decrypted_value: JsonValue::String("ext_items".to_string()),
            sig: None,
        }
    }

    #[test]
    fn shared_space_delete_replay_rejected_by_policy_does_not_propagate() {
        let db = setup_shared_space_delete_db();
        // Business row older than the delete-log entry, so propagation
        // would otherwise proceed (no resurrection, register present).
        seed_shared_delete_scenario(&db, "1/aaa", "2/bbb", r#"{"table_name":"2/bbb"}"#);

        // Shared-space apply (expected_space_id = Some, no owner config in
        // this fixture => enforce_sigs = true) with an UNSIGNED change: the
        // policy drops it, so it must never reach the propagation set.
        apply_remote_changes_to_db_scoped(
            &db,
            vec![shared_delete_replay_change("3/ccc")],
            None,
            None,
            Some("space-x"),
        )
        .expect("apply must succeed — rejection is column-scoped, not fatal");

        assert!(
            item_one_exists(&db),
            "a policy-rejected shared-space delete claim must not propagate"
        );
    }

    #[test]
    fn shared_space_delete_admitted_stale_replay_still_propagates() {
        let db = setup_shared_space_delete_db();
        seed_shared_delete_scenario(&db, "1/aaa", "2/bbb", r#"{"table_name":"2/bbb"}"#);

        // Owner-mode apply (expected_space_id = None => enforce_sigs =
        // false) replaying the SAME column at the SAME HLC already
        // recorded: LWW rejects it as Stale, but an admitted replay must
        // still propagate the already-recorded tombstone.
        apply_remote_changes_to_db_scoped(
            &db,
            vec![shared_delete_replay_change("2/bbb")],
            None,
            None,
            None,
        )
        .expect("apply must succeed");

        assert!(
            !item_one_exists(&db),
            "a stale replay of an already-admitted shared-space delete must still propagate"
        );
    }
}
