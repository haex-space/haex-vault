//! Tests for the owner-serve handler.
//!
//! Two layers live here:
//! 1. The pure request-variant classifier (`owner_request_action`): an
//!    owner-classified connection only ever acts on SyncPull / SyncPush /
//!    SyncPullColumns, never falling through to other request logic.
//! 2. AppHandle-free behavioral coverage of `handle_owner_pull_columns` (the
//!    SyncPullColumns serving path) against an in-memory DB.
//!
//! The push path and the "foreign peer gets ZERO vault-private rows" negative
//! need a live DB + AppHandle / real QUIC, so that coverage is the endpoint
//! capstone in `owner_sync_integration_tests.rs`.

use std::sync::{Arc, Mutex};

use rusqlite::Connection;

use super::{handle_owner_pull_columns, owner_request_action, OwnerRequestAction};
use crate::crdt::scanner::LocalColumnChange;
use crate::database::DbConnection;
use crate::space_delivery::local::protocol::{Request, Response};

#[test]
fn sync_pull_routes_to_pull() {
    let req = Request::SyncPull {
        space_id: "vault-space".to_string(),
        after_timestamp: None,
        ucan_token: None,
    };
    assert_eq!(owner_request_action(&req), OwnerRequestAction::Pull);
}

#[test]
fn sync_push_routes_to_push() {
    let req = Request::SyncPush {
        space_id: "vault-space".to_string(),
        changes: serde_json::json!([]),
        ucan_token: None,
    };
    assert_eq!(owner_request_action(&req), OwnerRequestAction::Push);
}

/// Any non-sync request reaching the owner handler must be rejected, NOT
/// silently treated as a sync op or passed to space logic.
#[test]
fn non_sync_requests_are_rejected() {
    let welcomes = Request::MlsFetchWelcomes {
        space_id: "vault-space".to_string(),
    };
    assert_eq!(owner_request_action(&welcomes), OwnerRequestAction::Reject);

    let rejoin = Request::RequestRejoin {
        space_id: "vault-space".to_string(),
        ucan_token: None,
    };
    assert_eq!(owner_request_action(&rejoin), OwnerRequestAction::Reject);

    let kp_count = Request::MlsKeyPackageCount {
        space_id: "vault-space".to_string(),
    };
    assert_eq!(owner_request_action(&kp_count), OwnerRequestAction::Reject);
}

#[test]
fn sync_pull_columns_routes_to_pull_columns() {
    let req = Request::SyncPullColumns {
        space_id: "vault-space".to_string(),
        columns: vec![("haex_passwords".to_string(), "title".to_string())],
        after_row_pks: None,
        ucan_token: None,
    };
    assert_eq!(owner_request_action(&req), OwnerRequestAction::PullColumns);
}

// ---- Behavioral coverage for the serving handler (AppHandle-free) ----

/// Build an in-memory `DbConnection` with two CRDT-shaped tables seeded with
/// rows. Each table has `id` PK, data columns, and the CRDT metadata columns
/// `haex_hlc` + `haex_column_hlcs`.
fn setup_two_table_db() -> DbConnection {
    let conn = Connection::open_in_memory().unwrap();
    conn.execute_batch(
        "CREATE TABLE notes (
                id TEXT PRIMARY KEY,
                title TEXT,
                body TEXT,
                haex_hlc TEXT,
                haex_column_hlcs TEXT NOT NULL DEFAULT '{}'
            );
         CREATE TABLE tags (
                id TEXT PRIMARY KEY,
                label TEXT,
                haex_hlc TEXT,
                haex_column_hlcs TEXT NOT NULL DEFAULT '{}'
            );",
    )
    .unwrap();

    // HLC strings: "<u64>/<node_hex>" (see scanner_tests.rs convention).
    insert_note(
        &conn,
        "n1",
        "first",
        "alpha",
        "1000000000000000000/aabbccdd",
    );
    insert_note(
        &conn,
        "n2",
        "second",
        "beta",
        "2000000000000000000/aabbccdd",
    );
    insert_note(
        &conn,
        "n3",
        "third",
        "gamma",
        "3000000000000000000/aabbccdd",
    );

    insert_tag(&conn, "t1", "red", "1000000000000000000/eeff0011");
    insert_tag(&conn, "t2", "blue", "2000000000000000000/eeff0011");

    DbConnection(Arc::new(Mutex::new(Some(conn))))
}

fn insert_note(conn: &Connection, id: &str, title: &str, body: &str, hlc: &str) {
    let hlcs = format!("{{\"title\":\"{hlc}\",\"body\":\"{hlc}\"}}");
    conn.execute(
        "INSERT INTO notes (id, title, body, haex_hlc, haex_column_hlcs)
             VALUES (?1, ?2, ?3, ?4, ?5)",
        rusqlite::params![id, title, body, hlc, hlcs],
    )
    .unwrap();
}

fn insert_tag(conn: &Connection, id: &str, label: &str, hlc: &str) {
    let hlcs = format!("{{\"label\":\"{hlc}\"}}");
    conn.execute(
        "INSERT INTO tags (id, label, haex_hlc, haex_column_hlcs)
             VALUES (?1, ?2, ?3, ?4)",
        rusqlite::params![id, label, hlc, hlcs],
    )
    .unwrap();
}

fn changes_from_response(resp: Response) -> Vec<LocalColumnChange> {
    match resp {
        Response::SyncChanges { changes } => serde_json::from_value(changes).unwrap(),
        other => panic!("expected SyncChanges, got {other:?}"),
    }
}

/// A single requested column is dumped for EVERY row, and no other column of
/// the same table leaks into the response.
#[test]
fn pull_columns_dumps_full_column_and_only_that_column() {
    let db = setup_two_table_db();

    let resp = handle_owner_pull_columns(&[("notes".to_string(), "title".to_string())], &db);
    let changes = changes_from_response(resp);

    // Exactly the three `title` values across all rows — full dump.
    assert_eq!(changes.len(), 3);
    assert!(changes.iter().all(|c| c.table_name == "notes"));
    assert!(changes.iter().all(|c| c.column_name == "title"));

    let mut titles: Vec<&str> = changes.iter().map(|c| c.value.as_str().unwrap()).collect();
    titles.sort();
    assert_eq!(titles, vec!["first", "second", "third"]);

    // The other column of `notes` (`body`) must NOT be present.
    assert!(changes.iter().all(|c| c.column_name != "body"));
}

/// Requesting two `(table, column)` pairs returns both columns' full dumps.
#[test]
fn pull_columns_serves_multiple_pairs() {
    let db = setup_two_table_db();

    let resp = handle_owner_pull_columns(
        &[
            ("notes".to_string(), "body".to_string()),
            ("tags".to_string(), "label".to_string()),
        ],
        &db,
    );
    let changes = changes_from_response(resp);

    let bodies: Vec<&LocalColumnChange> = changes
        .iter()
        .filter(|c| c.table_name == "notes" && c.column_name == "body")
        .collect();
    let labels: Vec<&LocalColumnChange> = changes
        .iter()
        .filter(|c| c.table_name == "tags" && c.column_name == "label")
        .collect();

    assert_eq!(bodies.len(), 3, "all notes bodies present");
    assert_eq!(labels.len(), 2, "all tags labels present");

    // Nothing else leaked (only the two requested columns).
    assert_eq!(changes.len(), 5);
    assert!(changes
        .iter()
        .all(|c| c.column_name == "body" || c.column_name == "label"));
}

/// A requested column that does not exist yields an empty/those-only set,
/// never an error response. (`scan_single_column_for_owner` filters by name,
/// so a missing column simply contributes nothing.)
#[test]
fn pull_columns_missing_column_is_empty_not_error() {
    let db = setup_two_table_db();

    let resp =
        handle_owner_pull_columns(&[("notes".to_string(), "does_not_exist".to_string())], &db);
    let changes = changes_from_response(resp);
    assert!(
        changes.is_empty(),
        "missing column must produce an empty dump, not an error"
    );
}

/// A requested table that does not exist also yields an empty set, never an
/// error. The scanner treats an unknown table as an empty schema (Ok(empty)),
/// so a stale `(table, column)` pair from the wire cannot fail the whole
/// response.
#[test]
fn pull_columns_missing_table_is_empty_not_error() {
    let db = setup_two_table_db();

    let resp =
        handle_owner_pull_columns(&[("no_such_table".to_string(), "title".to_string())], &db);
    let changes = changes_from_response(resp);
    assert!(
        changes.is_empty(),
        "missing table must produce an empty dump, not an error"
    );
}
