use std::sync::{Arc, Mutex};

use rusqlite::Connection;
use serde_json::{json, Value as JsonValue};

use crate::crdt::hlc::HlcService;
use crate::crdt::scanner::LocalColumnChange;
use crate::database::connection_context::ConnectionContext;
use crate::database::core::{self, install_tx_hlc_hooks, register_current_hlc_udf};
use crate::database::DbConnection;
use crate::space_delivery::local::inbound_sync::InboundSyncPushOutcome;
use crate::table_names::{TABLE_CRDT_CONFIGS, TABLE_CRDT_DIRTY_TABLES};

pub(super) fn make_change(
    table: &str,
    row_id: &str,
    column: &str,
    hlc: &str,
    value: JsonValue,
) -> LocalColumnChange {
    LocalColumnChange {
        table_name: table.to_string(),
        row_pks: format!(r#"{{"id":"{row_id}"}}"#),
        column_name: column.to_string(),
        hlc_timestamp: hlc.to_string(),
        value,
        device_id: "device-under-test".to_string(),
    }
}

pub(super) fn change(
    table: &str,
    row_id: &str,
    column: &str,
    hlc: &str,
    value: JsonValue,
) -> LocalColumnChange {
    LocalColumnChange {
        table_name: table.to_string(),
        row_pks: format!(r#"{{"id":"{row_id}"}}"#),
        column_name: column.to_string(),
        hlc_timestamp: hlc.to_string(),
        value,
        device_id: "wire-device-id".to_string(),
    }
}

pub(super) fn expect_accepted(outcome: InboundSyncPushOutcome) -> Vec<LocalColumnChange> {
    match outcome {
        InboundSyncPushOutcome::Accepted { changes } => changes,
        InboundSyncPushOutcome::Rejected { reason } => {
            panic!("expected Accepted, got Rejected: {reason}")
        }
    }
}

pub(super) fn expect_rejected(outcome: InboundSyncPushOutcome) -> String {
    match outcome {
        InboundSyncPushOutcome::Rejected { reason } => reason,
        InboundSyncPushOutcome::Accepted { .. } => panic!("expected Rejected, got Accepted"),
    }
}

/// In-memory DB with all schemas the authorisation pipeline reads from.
/// Schemas mirror production but skip CRDT triggers — these tests do not
/// exercise the CRDT merge layer, only authorisation decisions.
pub(super) fn setup_authz_db() -> DbConnection {
    let conn = Connection::open_in_memory().unwrap();
    let hlc = HlcService::new_for_testing("test-device");
    let ctx = ConnectionContext::new();
    register_current_hlc_udf(&conn, hlc, ctx.clone()).unwrap();
    install_tx_hlc_hooks(&conn, ctx).unwrap();

    conn.execute_batch(&format!(
        "CREATE TABLE {} (key TEXT PRIMARY KEY, type TEXT NOT NULL, value TEXT NOT NULL);",
        TABLE_CRDT_CONFIGS
    ))
    .unwrap();
    conn.execute_batch(&format!(
        "CREATE TABLE {} (table_name TEXT PRIMARY KEY, last_modified TEXT);",
        TABLE_CRDT_DIRTY_TABLES
    ))
    .unwrap();

    // `haex_identities` schema mirrored from production so the shared
    // `test_support::insert_identity` helper (which writes the
    // `name NOT NULL` / `source NOT NULL DEFAULT 'contact'` columns
    // present in `0000_slippery_black_tom.sql`) succeeds against this DB.
    // Keeping the schema at full production parity also catches drift
    // earlier — a future migration that adds another NOT NULL column on
    // identities will fail loudly in both test fixtures at once.
    conn.execute_batch(
        "CREATE TABLE haex_identities (
            id TEXT PRIMARY KEY,
            did TEXT NOT NULL UNIQUE,
            name TEXT NOT NULL,
            source TEXT NOT NULL DEFAULT 'contact',
            private_key TEXT,
            created_at TEXT
        );

        CREATE TABLE haex_spaces (
            id TEXT PRIMARY KEY,
            type TEXT NOT NULL DEFAULT 'local',
            status TEXT NOT NULL DEFAULT 'active',
            name TEXT NOT NULL
        );

        CREATE TABLE haex_space_members (
            id TEXT PRIMARY KEY,
            space_id TEXT NOT NULL,
            identity_id TEXT NOT NULL,
            role TEXT NOT NULL DEFAULT 'read',
            authored_by_did TEXT,
            joined_at TEXT
        );

        CREATE TABLE haex_space_devices (
            id TEXT PRIMARY KEY,
            space_id TEXT NOT NULL,
            identity_id TEXT,
            endpoint_id TEXT NOT NULL,
            name TEXT NOT NULL,
            relay_url TEXT,
            authored_by_did TEXT
        );

        CREATE TABLE haex_device_mls_enrollments (
            id TEXT PRIMARY KEY,
            space_id TEXT NOT NULL,
            device_id TEXT NOT NULL,
            key_package TEXT NOT NULL,
            welcome TEXT,
            status TEXT NOT NULL DEFAULT 'pending',
            authored_by_did TEXT
        );

        CREATE TABLE haex_mls_sync_keys (
            id TEXT PRIMARY KEY,
            space_id TEXT NOT NULL,
            epoch INTEGER NOT NULL,
            key_data TEXT NOT NULL,
            authored_by_did TEXT
        );

        CREATE TABLE haex_peer_shares (
            id TEXT PRIMARY KEY,
            space_id TEXT NOT NULL,
            endpoint_id TEXT NOT NULL,
            name TEXT NOT NULL,
            local_path TEXT NOT NULL,
            authored_by_did TEXT
        );",
    )
    .unwrap();

    DbConnection(Arc::new(Mutex::new(Some(conn))))
}

pub(super) fn insert_device(
    db: &DbConnection,
    device_row_id: &str,
    space_id: &str,
    identity_id: Option<&str>,
    endpoint_id: &str,
    name: &str,
) {
    core::execute(
        "INSERT INTO haex_space_devices \
         (id, space_id, identity_id, endpoint_id, name) \
         VALUES (?1, ?2, ?3, ?4, ?5)"
            .to_string(),
        vec![
            json!(device_row_id),
            json!(space_id),
            identity_id.map(JsonValue::from).unwrap_or(JsonValue::Null),
            json!(endpoint_id),
            json!(name),
        ],
        db,
    )
    .unwrap();
}

pub(super) fn insert_share(
    db: &DbConnection,
    share_row_id: &str,
    space_id: &str,
    endpoint_id: &str,
    name: &str,
) {
    core::execute(
        "INSERT INTO haex_peer_shares \
         (id, space_id, endpoint_id, name, local_path) \
         VALUES (?1, ?2, ?3, ?4, ?5)"
            .to_string(),
        vec![
            json!(share_row_id),
            json!(space_id),
            json!(endpoint_id),
            json!(name),
            json!("/x"),
        ],
        db,
    )
    .unwrap();
}
