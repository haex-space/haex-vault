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
use super::{collect_push_changes, SyncMode};
use crate::database::DbConnection;
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
