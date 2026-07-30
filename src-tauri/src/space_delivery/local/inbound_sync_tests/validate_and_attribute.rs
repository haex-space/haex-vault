//! `validate_and_attribute` — table-scope acceptance + attribution.
//!
//! The table-scope check accepts a change iff its table is on the static
//! `SPACE_SCOPED_CRDT_TABLES` whitelist OR the `(table_name, row_pks,
//! space_id)` triple is registered in `haex_shared_space_sync`. All other
//! checks (column-level `space_id`, `authored_by_did` strip + re-inject)
//! remain pure transforms.

use serde_json::json;

use crate::crdt::scanner::LocalColumnChange;
use crate::space_delivery::local::inbound_sync::validate_and_attribute;

use super::helpers::{
    expect_accepted, expect_rejected, insert_registered, make_change, setup_authz_db,
};

#[test]
fn rejects_non_whitelisted_table() {
    let db = setup_authz_db();
    let changes = vec![make_change(
        "haex_identities",
        "row-1",
        "private_key",
        "1000/abcd",
        json!("leaked-key"),
    )];
    let reason = expect_rejected(validate_and_attribute(
        &db,
        "space-A",
        "did:key:zAlice",
        changes,
    ));
    assert!(
        reason.contains("haex_identities"),
        "reason should name the bad table: {reason}"
    );
}

#[test]
fn rejects_foreign_space_id_column_value() {
    let db = setup_authz_db();
    let changes = vec![make_change(
        "haex_peer_shares",
        "row-1",
        "space_id",
        "1000/abcd",
        json!("space-B"),
    )];
    let reason = expect_rejected(validate_and_attribute(
        &db,
        "space-A",
        "did:key:zAlice",
        changes,
    ));
    assert!(
        reason.contains("space-A") || reason.contains("space-B"),
        "reason should mention the space_id mismatch: {reason}"
    );
}

#[test]
fn accepts_matching_space_id_column_value() {
    let db = setup_authz_db();
    let changes = vec![make_change(
        "haex_peer_shares",
        "row-1",
        "space_id",
        "1000/abcd",
        json!("space-A"),
    )];
    let out = expect_accepted(validate_and_attribute(
        &db,
        "space-A",
        "did:key:zAlice",
        changes,
    ));
    assert!(out.iter().any(|c| c.column_name == "space_id"));
}

#[test]
fn strips_client_supplied_authored_by_did() {
    let db = setup_authz_db();
    // Attacker pushes a row and tries to claim Bob wrote it.
    let changes = vec![
        make_change(
            "haex_peer_shares",
            "row-1",
            "space_id",
            "1000/abcd",
            json!("space-A"),
        ),
        make_change(
            "haex_peer_shares",
            "row-1",
            "name",
            "2000/abcd",
            json!("evil-share"),
        ),
        make_change(
            "haex_peer_shares",
            "row-1",
            "authored_by_did",
            "3000/abcd",
            json!("did:key:zBob"),
        ),
    ];
    let out = expect_accepted(validate_and_attribute(
        &db,
        "space-A",
        "did:key:zMallory",
        changes,
    ));

    let author_changes: Vec<&LocalColumnChange> = out
        .iter()
        .filter(|c| c.column_name == "authored_by_did")
        .collect();
    assert_eq!(
        author_changes.len(),
        1,
        "exactly one authored_by_did change expected, got {author_changes:?}"
    );
    let author_value = author_changes[0].value.as_str().unwrap();
    assert_eq!(
        author_value, "did:key:zMallory",
        "origin must be the UCAN audience (Mallory), not the client claim (Bob)",
    );
}

#[test]
fn injects_one_authored_by_did_per_unique_row() {
    let db = setup_authz_db();
    let changes = vec![
        make_change(
            "haex_peer_shares",
            "row-1",
            "name",
            "1000/abcd",
            json!("share-one"),
        ),
        make_change(
            "haex_peer_shares",
            "row-1",
            "local_path",
            "2000/abcd",
            json!("/path/one"),
        ),
        make_change(
            "haex_peer_shares",
            "row-2",
            "name",
            "3000/abcd",
            json!("share-two"),
        ),
    ];
    let out = expect_accepted(validate_and_attribute(
        &db,
        "space-A",
        "did:key:zAlice",
        changes,
    ));

    let mut author_rows: Vec<&str> = out
        .iter()
        .filter(|c| c.column_name == "authored_by_did")
        .map(|c| c.row_pks.as_str())
        .collect();
    author_rows.sort();
    assert_eq!(
        author_rows,
        vec![r#"{"id":"row-1"}"#, r#"{"id":"row-2"}"#],
        "exactly one authored_by_did per unique row expected",
    );
}

#[test]
fn authored_by_did_uses_max_hlc_within_row_group() {
    // HLC string format is "<ntp_nanos>/<node_id_hex>" — compared
    // numerically by the time component. Pass them out of order to
    // prove the transform picks the real maximum, not the first-seen.
    let db = setup_authz_db();
    let changes = vec![
        make_change("haex_peer_shares", "row-1", "name", "1000/abcd", json!("a")),
        make_change(
            "haex_peer_shares",
            "row-1",
            "local_path",
            "9000/abcd",
            json!("z"),
        ),
        make_change(
            "haex_peer_shares",
            "row-1",
            "endpoint_id",
            "5000/abcd",
            json!("m"),
        ),
    ];
    let out = expect_accepted(validate_and_attribute(
        &db,
        "space-A",
        "did:key:zAlice",
        changes,
    ));

    let author = out
        .iter()
        .find(|c| c.column_name == "authored_by_did")
        .expect("authored_by_did should be injected");
    assert_eq!(
        author.hlc_timestamp, "9000/abcd",
        "authored_by_did HLC should be the max HLC of the row-group",
    );
}

#[test]
fn origin_always_comes_from_audience_never_from_payload() {
    // Even with no client-supplied authored_by_did, the leader sets one
    // from the audience.
    let db = setup_authz_db();
    let changes = vec![make_change(
        "haex_space_members",
        "row-1",
        "role",
        "1000/abcd",
        json!("write"),
    )];
    let out = expect_accepted(validate_and_attribute(
        &db,
        "space-A",
        "did:key:zAlice",
        changes,
    ));

    let author = out
        .iter()
        .find(|c| c.column_name == "authored_by_did")
        .expect("authored_by_did must be injected even without client input");
    assert_eq!(author.value.as_str(), Some("did:key:zAlice"));
}

#[test]
fn empty_batch_stays_empty() {
    let db = setup_authz_db();
    let out = expect_accepted(validate_and_attribute(
        &db,
        "space-A",
        "did:key:zAlice",
        vec![],
    ));
    assert!(out.is_empty());
}

#[test]
fn preserves_non_attribution_changes() {
    // Sanity: the transform must not swallow legitimate changes.
    let db = setup_authz_db();
    let changes = vec![make_change(
        "haex_peer_shares",
        "row-1",
        "name",
        "1000/abcd",
        json!("my-share"),
    )];
    let out = expect_accepted(validate_and_attribute(
        &db,
        "space-A",
        "did:key:zAlice",
        changes,
    ));
    assert!(
        out.iter()
            .any(|c| c.column_name == "name" && c.value.as_str() == Some("my-share")),
        "original 'name' change must be preserved",
    );
}

// -----------------------------------------------------------------------
// Task 3a: registry-driven acceptance for content tables
// -----------------------------------------------------------------------
//
// `haex_shared_space_sync` registers `(table, row_pks, space_id)` triples
// for extension-owned content tables. Those tables are NOT on the static
// `SPACE_SCOPED_CRDT_TABLES` whitelist — the whitelist is intended for the
// membership-system tables. Stage-3 scope must accept a change targeting
// such a registered triple, and reject anything else.

const EXT_TABLE: &str = "ext_notes_v1";

#[test]
fn accepts_registered_content_table() {
    // Given a haex_shared_space_sync row for (space_id, ext_table, row_pks),
    // validate_and_attribute must accept a LocalColumnChange for that same
    // (table, row_pks, space_id) triple — even though ext_table is NOT in
    // SPACE_SCOPED_CRDT_TABLES.
    let db = setup_authz_db();
    let row_pks = r#"{"id":"row-1"}"#;
    insert_registered(&db, "reg-1", EXT_TABLE, row_pks, "space-A");

    let changes = vec![make_change(
        EXT_TABLE,
        "row-1",
        "body",
        "1000/abcd",
        json!("hello"),
    )];

    let out = expect_accepted(validate_and_attribute(
        &db,
        "space-A",
        "did:key:zAlice",
        changes,
    ));
    assert!(
        out.iter().any(|c| c.column_name == "body"),
        "the original change must survive attribution: {out:?}",
    );
}

#[test]
fn rejects_unregistered_content_table() {
    // Same setup minus the registry row.
    let db = setup_authz_db();
    let changes = vec![make_change(
        EXT_TABLE,
        "row-1",
        "body",
        "1000/abcd",
        json!("hello"),
    )];

    let reason = expect_rejected(validate_and_attribute(
        &db,
        "space-A",
        "did:key:zAlice",
        changes,
    ));
    assert!(
        reason.contains(EXT_TABLE),
        "reason should name the unregistered table: {reason}"
    );
}

#[test]
fn rejects_registered_content_row_when_row_pks_differ() {
    // Registry has row X registered; incoming change targets row Y in the
    // same table.
    let db = setup_authz_db();
    let row_x_pks = r#"{"id":"row-x"}"#;
    insert_registered(&db, "reg-x", EXT_TABLE, row_x_pks, "space-A");

    let changes = vec![make_change(
        EXT_TABLE,
        "row-y",
        "body",
        "1000/abcd",
        json!("attack"),
    )];

    let reason = expect_rejected(validate_and_attribute(
        &db,
        "space-A",
        "did:key:zAlice",
        changes,
    ));
    assert!(
        reason.contains(EXT_TABLE),
        "reason should name the table: {reason}"
    );
}

#[test]
fn rejects_registered_content_row_when_space_id_differs() {
    // Registry has (ext_table, row_pks) registered for SPACE_A; incoming
    // change targets same triple but claims SPACE_B.
    let db = setup_authz_db();
    let row_pks = r#"{"id":"row-1"}"#;
    insert_registered(&db, "reg-a", EXT_TABLE, row_pks, "space-A");

    let changes = vec![make_change(
        EXT_TABLE,
        "row-1",
        "body",
        "1000/abcd",
        json!("attack"),
    )];

    let reason = expect_rejected(validate_and_attribute(
        &db,
        "space-B",
        "did:key:zAlice",
        changes,
    ));
    assert!(
        reason.contains(EXT_TABLE),
        "reason should name the table: {reason}"
    );
}

#[test]
fn rejects_when_registry_lookup_fails() {
    // Fail-CLOSED: if `is_registered_for_space` returns `Err` (e.g. the
    // registry table is missing or corrupt), an unwhitelisted-table change
    // MUST reject rather than either accept or silently be treated as
    // unregistered. This guards against a future `.unwrap_or(false)`-style
    // regression.
    let db = setup_authz_db();
    // Drop the registry table so the lookup errors out. Direct SQL (not
    // `core::execute_with_crdt`) — we're setting up an error condition,
    // not exercising the CRDT layer.
    {
        let guard = db.0.lock().unwrap();
        let conn = guard.as_ref().unwrap();
        conn.execute("DROP TABLE haex_shared_space_sync", [])
            .unwrap();
    }

    let changes = vec![make_change(
        EXT_TABLE,
        "row-1",
        "body",
        "1000/abcd",
        json!("hello"),
    )];

    let reason = expect_rejected(validate_and_attribute(
        &db,
        "space-A",
        "did:key:zAlice",
        changes,
    ));
    assert!(
        reason.contains("Registry lookup failed"),
        "reason should surface the underlying lookup failure: {reason}"
    );
}
