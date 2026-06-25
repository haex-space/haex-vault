//! `validate_and_attribute` — pure transform.

use serde_json::json;

use crate::crdt::scanner::LocalColumnChange;
use crate::space_delivery::local::inbound_sync::validate_and_attribute;

use super::helpers::{expect_accepted, expect_rejected, make_change};

#[test]
fn rejects_non_whitelisted_table() {
    let changes = vec![make_change(
        "haex_identities",
        "row-1",
        "private_key",
        "1000/abcd",
        json!("leaked-key"),
    )];
    let reason = expect_rejected(validate_and_attribute("space-A", "did:key:zAlice", changes));
    assert!(
        reason.contains("haex_identities"),
        "reason should name the bad table: {reason}"
    );
}

#[test]
fn rejects_foreign_space_id_column_value() {
    let changes = vec![make_change(
        "haex_peer_shares",
        "row-1",
        "space_id",
        "1000/abcd",
        json!("space-B"),
    )];
    let reason = expect_rejected(validate_and_attribute("space-A", "did:key:zAlice", changes));
    assert!(
        reason.contains("space-A") || reason.contains("space-B"),
        "reason should mention the space_id mismatch: {reason}"
    );
}

#[test]
fn accepts_matching_space_id_column_value() {
    let changes = vec![make_change(
        "haex_peer_shares",
        "row-1",
        "space_id",
        "1000/abcd",
        json!("space-A"),
    )];
    let out = expect_accepted(validate_and_attribute("space-A", "did:key:zAlice", changes));
    assert!(out.iter().any(|c| c.column_name == "space_id"));
}

#[test]
fn strips_client_supplied_authored_by_did() {
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
    let out = expect_accepted(validate_and_attribute("space-A", "did:key:zAlice", changes));

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
    let out = expect_accepted(validate_and_attribute("space-A", "did:key:zAlice", changes));

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
    let changes = vec![make_change(
        "haex_space_members",
        "row-1",
        "role",
        "1000/abcd",
        json!("write"),
    )];
    let out = expect_accepted(validate_and_attribute("space-A", "did:key:zAlice", changes));

    let author = out
        .iter()
        .find(|c| c.column_name == "authored_by_did")
        .expect("authored_by_did must be injected even without client input");
    assert_eq!(author.value.as_str(), Some("did:key:zAlice"));
}

#[test]
fn empty_batch_stays_empty() {
    let out = expect_accepted(validate_and_attribute("space-A", "did:key:zAlice", vec![]));
    assert!(out.is_empty());
}

#[test]
fn preserves_non_attribution_changes() {
    // Sanity: the transform must not swallow legitimate changes.
    let changes = vec![make_change(
        "haex_peer_shares",
        "row-1",
        "name",
        "1000/abcd",
        json!("my-share"),
    )];
    let out = expect_accepted(validate_and_attribute("space-A", "did:key:zAlice", changes));
    assert!(
        out.iter()
            .any(|c| c.column_name == "name" && c.value.as_str() == Some("my-share")),
        "original 'name' change must be preserved",
    );
}
