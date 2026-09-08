//! Pass 2 of `scan_space_scoped_tables_for_local_changes`: the
//! registry-driven scan of extension-owned content tables.

use super::fixtures::{
    insert_ext_row, insert_member_row, insert_registry_entry, setup_registry_scan_db, wrap_db,
    EXT_TABLE,
};
use crate::crdt::space_scanner::{
    scan_membership_tables_for_local_changes, scan_space_scoped_tables_for_local_changes,
};

// -----------------------------------------------------------------------
// Registry-driven P2P scan coverage
// -----------------------------------------------------------------------
//
// `scan_space_scoped_tables_for_local_changes` runs two passes:
//
// * Pass 1 — static [`SPACE_SCOPED_CRDT_TABLES`] whitelist for the
//   membership-system tables scoped by their own `space_id` column.
// * Pass 2 — registry-driven scan of extension-owned content tables via
//   `haex_shared_space_sync`, which maps `(table_name, row_pks)` to a
//   `space_id` for tables that carry no `space_id` column of their own.
//   The register and the whitelist are additive; the guard in the pass 2
//   loop drops any register row referencing a whitelisted table so
//   control-plane data cannot be re-emitted through the ext path.
//
// The tests below lock the observable contract of that pipeline in:
//
// * `registered_extension_row_included_in_p2p_scan` — a registered
//   `ext_notes_v1` row appears in the scan output and a sibling
//   unregistered row does not.
// * `registered_row_in_different_space_not_included` — cross-space leak
//   guard: the registry-space filter matches on `registry.space_id`, so
//   a row registered for space B is invisible to a scan targeting
//   space A.
// * `control_plane_scan_still_works_without_registry_entries` —
//   regression guard: pass 1 continues to emit whitelisted rows even

#[test]
fn registered_extension_row_included_in_p2p_scan() {
    // Given `(space-A, EXT_TABLE, {"id":"row-1"})` is registered in
    // haex_shared_space_sync and the corresponding row exists in
    // EXT_TABLE, the space-scoped P2P scan returns it. A second row in
    // the same table that is NOT registered stays out of the output —
    // the register is the sole entry point for extension-owned rows
    // into pass 2 of `scan_space_scoped_tables_for_local_changes`.
    let conn = setup_registry_scan_db();
    insert_ext_row(
        &conn,
        "row-1",
        "hello",
        "1000000000000000000/aabbccdd",
        Some("space-A"),
    );
    insert_ext_row(
        &conn,
        "row-unregistered",
        "sekrit",
        "2000000000000000000/aabbccdd",
        Some("space-A"),
    );
    insert_registry_entry(&conn, "reg-1", "space-A", EXT_TABLE, r#"{"id":"row-1"}"#);

    let db = wrap_db(conn);
    let changes =
        scan_space_scoped_tables_for_local_changes(&db, "space-A", None, "device-A", None).unwrap();

    assert!(
        changes
            .iter()
            .any(|c| c.table_name == EXT_TABLE && c.row_pks.contains("row-1")),
        "registered extension row must be scanned for P2P push, got: {changes:?}",
    );
    assert!(
        !changes
            .iter()
            .any(|c| c.row_pks.contains("row-unregistered")),
        "unregistered rows must not leak into P2P scan, got: {changes:?}",
    );

    // Sig-forwarding (W1 → W2 contract). The receiver's W2 gate REJECTS
    // a registered extension row that arrives without a per-column sig
    // for the requested space, so the outbound scan MUST forward the
    // sig it read off the row. The fixture writes the sig via direct
    // INSERT (same shape `execute_with_crdt` would produce); if a
    // future refactor stops seeding sigs here, this assertion will
    // fail spuriously and needs re-inspection alongside the W1 write
    // path.
    let extension_changes: Vec<_> = changes
        .iter()
        .filter(|c| c.table_name == EXT_TABLE)
        .collect();
    assert!(
        !extension_changes.is_empty(),
        "at least one ext change is required for the sig-forwarding assertion to be meaningful",
    );
    assert!(
        extension_changes.iter().all(|c| c.sig.is_some()),
        "registered extension rows must carry a per-column signature (W1): {extension_changes:?}",
    );
}

#[test]
fn registered_row_in_different_space_not_included() {
    // Cross-space leak guard. The row is registered for SPACE_B but
    // the scan requests SPACE_A — the row must NOT appear. Passes
    // trivially today (ext tables are never scanned) and becomes
    // load-bearing once Task 5 iterates the register: a naive impl
    // that reads every register row without matching `space_id`
    // against the scan space would leak SPACE_B rows to a SPACE_A
    // peer.
    let conn = setup_registry_scan_db();
    insert_ext_row(
        &conn,
        "row-1",
        "hidden",
        "1000000000000000000/aabbccdd",
        Some("space-B"),
    );
    insert_registry_entry(&conn, "reg-b", "space-B", EXT_TABLE, r#"{"id":"row-1"}"#);

    let db = wrap_db(conn);
    let changes =
        scan_space_scoped_tables_for_local_changes(&db, "space-A", None, "device-A", None).unwrap();

    assert!(
        !changes.iter().any(|c| c.table_name == EXT_TABLE),
        "row registered in space-B must never appear in a space-A scan, got: {changes:?}",
    );
}

/// Composite-PK smoke test for the registry-driven pass.
///
/// Regression guard for the "PK JSON key ordering" concern (Task 5 code
/// review): the writer path (`extension_space_assign`) stores `row_pks`
/// verbatim from the caller, and TS extensions produce PK JSON via
/// `JSON.stringify` on object literals built in schema-PK-declaration
/// order (matching the TS reader `tableScanner.ts:446-449`). The outbound
/// scanner MUST produce the same schema-declaration-order form, or a
/// `HashSet::contains(pk_json)` filter would miss composite-PK rows and
/// silently drop them.
///
/// The scanner's canonical form is **schema-declaration order** — the
/// order PK columns are declared in the CREATE TABLE. This test locks
/// that contract in from the read side: a registry entry stored in
/// schema-declaration order (matching the writer wire form) MUST match.
#[test]
fn registered_composite_pk_row_matches_schema_order_wire_form() {
    let conn = setup_registry_scan_db();

    // Composite-PK extension table where schema order (b, a) != alphabetical (a, b).
    conn.execute_batch(
        "CREATE TABLE ext_composite_v1 (
            b TEXT NOT NULL,
            a TEXT NOT NULL,
            body TEXT,
            haex_hlc_no_trigger TEXT,
            haex_column_hlcs_no_trigger TEXT NOT NULL DEFAULT '{}',
            haex_column_sigs_no_trigger TEXT NOT NULL DEFAULT '{}',
            PRIMARY KEY (b, a)
        );",
    )
    .unwrap();

    let hlc = "1000000000000000000/aabbccdd";
    let hlcs = format!("{{\"body\":\"{hlc}\"}}");
    let sigs = serde_json::json!({
        "body": {
            "space-A": {
                "authorDid": "did:key:test",
                "sig": "",
                "storageClass": "text",
            }
        }
    })
    .to_string();
    conn.execute(
        "INSERT INTO ext_composite_v1 (b, a, body, haex_hlc_no_trigger, haex_column_hlcs_no_trigger, haex_column_sigs_no_trigger)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
        rusqlite::params!["yb", "xa", "hello", hlc, hlcs, sigs],
    )
    .unwrap();

    // Register with SCHEMA-DECLARATION key order — matches the writer wire
    // form (`JSON.stringify({b: ..., a: ...})` on a schema-order object
    // literal, which is what TS extensions produce and what
    // `extension_space_assign` stores verbatim).
    insert_registry_entry(
        &conn,
        "reg-schema-order",
        "space-A",
        "ext_composite_v1",
        r#"{"b":"yb","a":"xa"}"#,
    );

    let db = wrap_db(conn);
    let changes =
        scan_space_scoped_tables_for_local_changes(&db, "space-A", None, "device-A", None).unwrap();

    let composite_hits: Vec<_> = changes
        .iter()
        .filter(|c| c.table_name == "ext_composite_v1")
        .collect();
    assert!(
        !composite_hits.is_empty(),
        "composite-PK row registered in canonical (schema-declaration) key order must be \
         included in the space-scoped scan, got: {changes:?}",
    );
    // Sanity: the scanner emitted the row_pks in schema-declaration order too,
    // proving both writer and scanner agree on the canonical form.
    assert_eq!(
        composite_hits[0].row_pks, r#"{"b":"yb","a":"xa"}"#,
        "scanner must emit composite PKs in schema-declaration key order (matches writer wire form)"
    );
}

/// Negative counterpart: a register entry stored in alphabetical PK
/// order (non-canonical under the current wire form) MUST NOT match,
/// because the outbound scanner produces schema-declaration-order PK
/// JSON. If this test starts failing, someone likely changed the scanner
/// to emit alphabetical form (e.g. reintroduced `serde_json::Map`) —
/// see the doc on `is_registered_for_space` for the wire contract.
#[test]
fn registered_composite_pk_row_ignored_when_registry_key_order_differs() {
    let conn = setup_registry_scan_db();
    conn.execute_batch(
        "CREATE TABLE ext_composite_v1 (
            b TEXT NOT NULL,
            a TEXT NOT NULL,
            body TEXT,
            haex_hlc_no_trigger TEXT,
            haex_column_hlcs_no_trigger TEXT NOT NULL DEFAULT '{}',
            haex_column_sigs_no_trigger TEXT NOT NULL DEFAULT '{}',
            PRIMARY KEY (b, a)
        );",
    )
    .unwrap();
    let hlc = "1000000000000000000/aabbccdd";
    let hlcs = format!("{{\"body\":\"{hlc}\"}}");
    conn.execute(
        "INSERT INTO ext_composite_v1 (b, a, body, haex_hlc_no_trigger, haex_column_hlcs_no_trigger)
             VALUES (?1, ?2, ?3, ?4, ?5)",
        rusqlite::params!["yb", "xa", "hello", hlc, hlcs],
    )
    .unwrap();
    // Register with ALPHABETICAL (a, b) key order — NOT the canonical
    // schema-declaration form the scanner produces.
    insert_registry_entry(
        &conn,
        "reg-alphabetical",
        "space-A",
        "ext_composite_v1",
        r#"{"a":"xa","b":"yb"}"#,
    );

    let db = wrap_db(conn);
    let changes =
        scan_space_scoped_tables_for_local_changes(&db, "space-A", None, "device-A", None).unwrap();

    assert!(
        !changes.iter().any(|c| c.table_name == "ext_composite_v1"),
        "register entry using non-canonical (alphabetical) key order MUST NOT match — \
         the register writer is responsible for producing schema-declaration-order \
         PK JSON: {changes:?}",
    );
}

#[test]
fn control_plane_scan_still_works_without_registry_entries() {
    // Regression guard. Whitelisted control-plane tables must keep
    // being scanned even when `haex_shared_space_sync` holds no rows
    // for the scan space. Locks in that Task 5 does not accidentally
    // gate whitelisted tables behind a register lookup — the whitelist
    // and the register are additive, not one-or-the-other.
    let conn = setup_registry_scan_db();
    insert_member_row(
        &conn,
        "mem-1",
        "space-A",
        "id-alice",
        "1000000000000000000/aabbccdd",
    );

    let db = wrap_db(conn);
    let changes =
        scan_space_scoped_tables_for_local_changes(&db, "space-A", None, "device-A", None).unwrap();

    assert!(
        changes
            .iter()
            .any(|c| c.table_name == "haex_space_members"),
        "whitelisted control-plane row must be scanned even with an empty registry, got: {changes:?}",
    );
}

#[test]
fn read_only_push_excludes_registered_extension_rows() {
    // Regression guard. Read-only members push via
    // `scan_membership_tables_for_local_changes`, which filters to
    // `MEMBERSHIP_SYSTEM_TABLES` only. Registered extension rows are
    // content, not membership, and must never leak into that pipe —
    // pushing them with Read capability would make the leader reject
    // the whole batch and wedge the read-only push cursor at t=0.
    //
    // Passes trivially today (the whitelist filter can't contain an
    // extension table), but Task 5 taught `scan_space_scoped_...` to
    // consult the registry; this test locks the read-only pipe down
    // against a future refactor that accidentally routes extension
    // rows through it.
    let conn = setup_registry_scan_db();
    // Seed a registered extension row into space-A — same fixture
    // shape as `registered_extension_row_included_in_p2p_scan`.
    insert_ext_row(
        &conn,
        "row-1",
        "hello",
        "1000000000000000000/aabbccdd",
        Some("space-A"),
    );
    insert_registry_entry(&conn, "reg-1", "space-A", EXT_TABLE, r#"{"id":"row-1"}"#);
    // Also seed a whitelisted membership row so the assertion below is
    // non-vacuous — the test would trivially pass on "no changes at
    // all", we want it to confirm the read-only pipe still surfaces
    // whitelisted rows while dropping extension rows.
    insert_member_row(
        &conn,
        "mem-1",
        "space-A",
        "id-alice",
        "2000000000000000000/aabbccdd",
    );

    let db = wrap_db(conn);
    let changes =
        scan_membership_tables_for_local_changes(&db, "space-A", None, "device-A", None).unwrap();

    assert!(
        !changes.iter().any(|c| c.table_name == EXT_TABLE),
        "read-only push MUST NOT include registered extension rows: {changes:?}",
    );
    assert!(
        changes
            .iter()
            .any(|c| c.table_name == "haex_space_members"),
        "read-only push must still surface whitelisted MEMBERSHIP_SYSTEM_TABLES rows, got: {changes:?}",
    );
}
