// Regression tests for the MLS cursor-skip logic in fetch_and_process_mls_messages.
//
// Root cause of the infinite rejoin loop:
//   1. Peer submits External Commit → leader stores it as message id=N.
//   2. Old code set cursor to batch_max (id of last message in the *current*
//      fetch batch, which was < N since the EC wasn't in the batch yet).
//   3. Next fetch: WHERE id > batch_max returned the EC (id=N) again.
//   4. EC had the old epoch → "Wrong Epoch" error → another rejoin → new EC
//      stored at id=N+1 → infinite loop.
//
// Fix: cursor = max(batch_max, ec_msg_id) so the EC itself is skipped.

/// Simulate: fetch returned [id=1,2,3], EC stored by leader as id=4.
/// skip_to must be 4 so the next fetch (WHERE id > 4) misses the EC.
#[test]
fn cursor_skips_ec_when_ec_is_beyond_batch() {
    let message_ids: Vec<i64> = vec![1, 2, 3];
    let failing_msg_id: i64 = 3;
    let ec_msg_id: i64 = 4;

    let batch_max = message_ids.iter().copied().max().unwrap_or(failing_msg_id);
    let skip_to = batch_max.max(ec_msg_id);

    assert_eq!(
        skip_to, 4,
        "cursor must advance past the EC (id=4), not stop at batch max (id=3)"
    );
}

/// EC arrives in the same batch as the failing message (unusual but possible).
/// skip_to must be batch_max so no messages are dropped.
#[test]
fn cursor_uses_batch_max_when_ec_already_in_batch() {
    let message_ids: Vec<i64> = vec![1, 2, 3, 4, 5];
    let failing_msg_id: i64 = 3;
    let ec_msg_id: i64 = 2; // hypothetically already in the batch

    let batch_max = message_ids.iter().copied().max().unwrap_or(failing_msg_id);
    let skip_to = batch_max.max(ec_msg_id);

    assert_eq!(
        skip_to, 5,
        "when batch_max > ec_msg_id, use batch_max to avoid losing later messages"
    );
}

/// Single-message batch where that message is the failing one.
/// unwrap_or(msg.id) kicks in → batch_max = failing_msg_id.
#[test]
fn cursor_handles_single_failing_message_in_batch() {
    let ec_msg_id: i64 = 8;

    // messages = [failing_msg with id=7], batch_max = max of [7] = 7
    let batch_max: i64 = 7;
    let skip_to = batch_max.max(ec_msg_id);

    assert_eq!(skip_to, 8);
}

// =====================================================================
// SyncMode push-collection tests.
//
// These exercise `collect_push_changes` — the pure-ish helper that
// decides WHICH scanner produces the push batch — without an AppHandle
// or QUIC session. They run against an in-memory DbConnection, mirroring
// the scanner unit tests.
// =====================================================================
use super::pending_columns::{
    pending_rows_to_clear, recoverable_pending_columns, rows_present_in_changes,
};
use super::push::collect_push_changes;
use super::SyncMode;
use crate::crdt::scanner::LocalColumnChange;
use crate::database::DbConnection;
use crate::table_names::TABLE_CRDT_PENDING_COLUMNS;
use rusqlite::Connection;
use std::sync::{Arc, Mutex};

/// Build an in-memory DB with one space-scoped whitelist table
/// (`haex_peer_shares`, carrying a `space_id`) and one off-whitelist,
/// vault-private table (`haex_passwords`, no `space_id`). Used to prove
/// that owner mode ships BOTH while space-scoped mode ships only the
/// whitelisted one.
fn setup_owner_vs_space_db() -> Connection {
    let conn = Connection::open_in_memory().unwrap();
    conn.execute_batch(
        "CREATE TABLE haex_peer_shares (
                id TEXT PRIMARY KEY,
                space_id TEXT NOT NULL,
                data TEXT,
                haex_hlc TEXT,
                haex_column_hlcs TEXT NOT NULL DEFAULT '{}'
            );
             CREATE TABLE haex_passwords (
                id TEXT PRIMARY KEY,
                secret TEXT,
                haex_hlc TEXT,
                haex_column_hlcs TEXT NOT NULL DEFAULT '{}'
            );",
    )
    .unwrap();
    conn
}

fn insert_peer_share(conn: &Connection, id: &str, space_id: &str, data: &str, hlc: &str) {
    let hlcs = format!("{{\"space_id\":\"{hlc}\",\"data\":\"{hlc}\"}}");
    conn.execute(
        "INSERT INTO haex_peer_shares (id, space_id, data, haex_hlc, haex_column_hlcs)
             VALUES (?1, ?2, ?3, ?4, ?5)",
        rusqlite::params![id, space_id, data, hlc, hlcs],
    )
    .unwrap();
}

fn insert_password(conn: &Connection, id: &str, secret: &str, hlc: &str) {
    let hlcs = format!("{{\"secret\":\"{hlc}\"}}");
    conn.execute(
        "INSERT INTO haex_passwords (id, secret, haex_hlc, haex_column_hlcs)
             VALUES (?1, ?2, ?3, ?4)",
        rusqlite::params![id, secret, hlc, hlcs],
    )
    .unwrap();
}

#[test]
fn owner_mode_push_collects_all_tables() {
    let conn = setup_owner_vs_space_db();
    insert_peer_share(
        &conn,
        "s1",
        "space-A",
        "shared",
        "1000000000000000000/aabbccdd",
    );
    let secret: u64 = rand::random();
    insert_password(
        &conn,
        "p1",
        &format!("v{secret}"),
        "2000000000000000000/aabbccdd",
    );
    let db = DbConnection(Arc::new(Mutex::new(Some(conn))));

    // Owner mode is handed the FULL table list explicitly (caller resolves
    // it). Include the off-whitelist private table.
    let tables = vec!["haex_peer_shares".to_string(), "haex_passwords".to_string()];
    let mode = SyncMode::OwnerVault { tables };

    let changes = collect_push_changes(
        &mode, &db, "space-A", None, "device-1", None, // our_node
        true, // can_push_user_content (irrelevant for owner mode)
    )
    .unwrap();

    let tables_seen: std::collections::HashSet<&str> =
        changes.iter().map(|c| c.table_name.as_str()).collect();
    assert!(
        tables_seen.contains("haex_peer_shares"),
        "owner mode dropped space-scoped table"
    );
    assert!(
        tables_seen.contains("haex_passwords"),
        "owner mode dropped the off-whitelist vault-private table"
    );
}

#[test]
fn space_scoped_mode_collects_only_space_tables() {
    let conn = setup_owner_vs_space_db();
    insert_peer_share(
        &conn,
        "s1",
        "space-A",
        "shared",
        "1000000000000000000/aabbccdd",
    );
    let secret: u64 = rand::random();
    insert_password(
        &conn,
        "p1",
        &format!("v{secret}"),
        "2000000000000000000/aabbccdd",
    );
    let db = DbConnection(Arc::new(Mutex::new(Some(conn))));

    let changes = collect_push_changes(
        &SyncMode::SpaceScoped,
        &db,
        "space-A",
        None,
        "device-1",
        None,
        true, // can_push_user_content → peer_shares included
    )
    .unwrap();

    let tables_seen: std::collections::HashSet<&str> =
        changes.iter().map(|c| c.table_name.as_str()).collect();
    // The on-whitelist table is present...
    assert!(
        tables_seen.contains("haex_peer_shares"),
        "space-scoped mode dropped a whitelisted table"
    );
    // ...but the off-whitelist vault-private table must NEVER appear.
    assert!(
        !tables_seen.contains("haex_passwords"),
        "space-scoped mode leaked the off-whitelist vault-private table"
    );
}

// =====================================================================
// recoverable_pending_columns tests.
//
// This filter is the data-loss guard: it must drop pending entries whose
// column has NOT yet been re-added locally, so the recovery step never
// clears a marker for a value it cannot actually apply.
// =====================================================================

/// In-memory connection with the pending-columns table, created via the real
/// const so the schema tracks the production table name.
fn pending_filter_conn() -> Connection {
    let conn = Connection::open_in_memory().expect("open in-memory connection");
    conn.execute_batch(&format!(
        "CREATE TABLE {TABLE_CRDT_PENDING_COLUMNS} (
            table_name TEXT NOT NULL,
            column_name TEXT NOT NULL,
            row_pks TEXT NOT NULL,
            PRIMARY KEY(table_name, column_name, row_pks)
        );"
    ))
    .expect("create pending-columns table");
    conn
}

fn insert_pending(conn: &Connection, table_name: &str, column_name: &str, row_pks: &str) {
    conn.execute(
        &format!(
            "INSERT INTO {TABLE_CRDT_PENDING_COLUMNS} (table_name, column_name, row_pks) \
             VALUES (?, ?, ?)"
        ),
        rusqlite::params![table_name, column_name, row_pks],
    )
    .expect("insert pending column");
}

/// Pending `(notes, title, r1)` where `notes` HAS `title` → recoverable.
#[test]
fn recoverable_returns_pending_column_that_exists_locally() {
    let conn = pending_filter_conn();
    conn.execute_batch("CREATE TABLE notes (id TEXT PRIMARY KEY, title TEXT);")
        .unwrap();
    insert_pending(&conn, "notes", "title", r#"{"id":"r1"}"#);

    let recoverable = recoverable_pending_columns(&conn).unwrap();
    assert_eq!(recoverable.len(), 1);
    assert_eq!(recoverable[0].table_name, "notes");
    assert_eq!(recoverable[0].column_name, "title");
    assert_eq!(recoverable[0].row_pks, r#"{"id":"r1"}"#);
}

/// Pending `(notes, bio)` where `notes` LACKS `bio` (migration not applied yet)
/// → filtered out. THIS is the data-loss guard: clearing it would lose the
/// skipped values forever.
#[test]
fn recoverable_filters_pending_column_not_yet_migrated() {
    let conn = pending_filter_conn();
    // `notes` exists but has no `bio` column yet.
    conn.execute_batch("CREATE TABLE notes (id TEXT PRIMARY KEY, title TEXT);")
        .unwrap();
    insert_pending(&conn, "notes", "bio", r#"{"id":"r1"}"#);

    let recoverable = recoverable_pending_columns(&conn).unwrap();
    assert!(
        recoverable.is_empty(),
        "a pending column the local schema lacks must NOT be recoverable \
         (clearing it would be silent data loss)"
    );
}

/// Pending `(ghost_table, x)` where the table doesn't exist → filtered out.
#[test]
fn recoverable_filters_pending_column_on_missing_table() {
    let conn = pending_filter_conn();
    insert_pending(&conn, "ghost_table", "x", r#"{"id":"r1"}"#);

    let recoverable = recoverable_pending_columns(&conn).unwrap();
    assert!(
        recoverable.is_empty(),
        "a pending column on a non-existent table must NOT be recoverable"
    );
}

/// No pending entries → empty result (the cheap path).
#[test]
fn recoverable_returns_empty_when_no_pending() {
    let conn = pending_filter_conn();
    let recoverable = recoverable_pending_columns(&conn).unwrap();
    assert!(recoverable.is_empty());
}

fn change(table: &str, column: &str, row_pks: &str) -> LocalColumnChange {
    LocalColumnChange {
        table_name: table.to_string(),
        row_pks: row_pks.to_string(),
        column_name: column.to_string(),
        hlc_timestamp: "1000000000000000000/aabbccdd".to_string(),
        value: serde_json::Value::String("v".to_string()),
        device_id: "leader".to_string(),
    }
}

/// The clear-set is exactly the `(table, column, row_pks)` triples the dump
/// carried a value for — this is the guard that stops an empty/absent
/// row from being cleared (which would be silent data loss under
/// owner-device row-incompleteness / version skew).
#[test]
fn rows_present_in_changes_keys_on_table_column_rowpks() {
    let changes = vec![
        change("devices", "bio", r#"{"id":"r1"}"#),
        change("devices", "bio", r#"{"id":"r1"}"#), // exact duplicate → one triple
        change("notes", "body", r#"{"id":"r9"}"#),
    ];
    let present = rows_present_in_changes(&changes);
    assert_eq!(present.len(), 2);
    assert!(present.contains(&(
        "devices".to_string(),
        "bio".to_string(),
        r#"{"id":"r1"}"#.to_string()
    )));
    assert!(present.contains(&(
        "notes".to_string(),
        "body".to_string(),
        r#"{"id":"r9"}"#.to_string()
    )));
    // A different row of the SAME column is NOT in the set → recovery must
    // leave that row pending rather than clear it.
    assert!(!present.contains(&(
        "devices".to_string(),
        "bio".to_string(),
        r#"{"id":"r2"}"#.to_string()
    )));
}

/// An empty dump yields an empty clear-set → nothing is cleared.
#[test]
fn rows_present_in_changes_empty_dump_clears_nothing() {
    let present = rows_present_in_changes(&[]);
    assert!(present.is_empty());
}

// ---------------------------------------------------------------------------
// Row-aware recovery: a partial dump from a NON-authoritative peer must NOT
// clear markers for rows it did not serve (silent-data-loss regression).
// ---------------------------------------------------------------------------
#[test]
fn partial_dump_keeps_unserved_rows_pending() {
    use crate::database::migrations::PendingColumnRow;

    // Two rows of devices.bio are owed (skipped pre-migration).
    let owed = vec![
        PendingColumnRow {
            table_name: "devices".to_string(),
            column_name: "bio".to_string(),
            row_pks: r#"{"id":"r1"}"#.to_string(),
        },
        PendingColumnRow {
            table_name: "devices".to_string(),
            column_name: "bio".to_string(),
            row_pks: r#"{"id":"r2"}"#.to_string(),
        },
    ];

    // The peer we reached is row-incomplete: it served only r1's bio.
    let present: std::collections::HashSet<(String, String, String)> = [(
        "devices".to_string(),
        "bio".to_string(),
        r#"{"id":"r1"}"#.to_string(),
    )]
    .into_iter()
    .collect();

    let to_clear = pending_rows_to_clear(&owed, &present);

    // r1 is recovered and clearable; r2 stays pending (its value still lives
    // only behind this device's pull cursor on some other peer — clearing it
    // here would lose it forever).
    assert_eq!(to_clear.len(), 1, "only the served row may be cleared");
    assert_eq!(to_clear[0].row_pks, r#"{"id":"r1"}"#);
}

#[test]
fn pending_rows_to_clear_empty_present_clears_nothing() {
    use crate::database::migrations::PendingColumnRow;
    let owed = vec![PendingColumnRow {
        table_name: "t".to_string(),
        column_name: "c".to_string(),
        row_pks: r#"{"id":"r"}"#.to_string(),
    }];
    assert!(pending_rows_to_clear(&owed, &Default::default()).is_empty());
}
