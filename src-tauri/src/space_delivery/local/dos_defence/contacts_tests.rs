use rusqlite::Connection;
use std::sync::{Arc, Mutex};

use super::contacts::ContactResolver;
use crate::crdt::hlc::HlcService;
use crate::database::connection_context::ConnectionContext;
use crate::database::core::{install_tx_hlc_hooks, register_current_hlc_udf};
use crate::database::DbConnection;
use crate::table_names::{TABLE_CRDT_CONFIGS, TABLE_CRDT_DIRTY_TABLES};

fn setup_test_db() -> DbConnection {
    let conn = Connection::open_in_memory().expect("in-memory DB");

    let hlc = HlcService::new_for_testing("test-device-contacts");
    let ctx = ConnectionContext::new();
    register_current_hlc_udf(&conn, hlc, ctx.clone()).unwrap();
    install_tx_hlc_hooks(&conn, ctx).unwrap();

    conn.execute_batch(&format!(
        "CREATE TABLE {} (key TEXT PRIMARY KEY, type TEXT NOT NULL, value TEXT NOT NULL)",
        TABLE_CRDT_CONFIGS
    ))
    .unwrap();
    conn.execute_batch(&format!(
        "CREATE TABLE {} (table_name TEXT PRIMARY KEY, last_modified TEXT)",
        TABLE_CRDT_DIRTY_TABLES
    ))
    .unwrap();

    // Minimal production-shape schema for the two tables the resolver hits.
    // Joining via identity_id matches the actual schema (see
    // src/database/schemas/spaces.ts and identity.ts).
    conn.execute_batch(
        "CREATE TABLE haex_identities (
            id TEXT PRIMARY KEY NOT NULL,
            did TEXT NOT NULL,
            name TEXT NOT NULL,
            source TEXT NOT NULL DEFAULT 'contact'
        );
        CREATE TABLE haex_space_members (
            id TEXT PRIMARY KEY NOT NULL,
            space_id TEXT NOT NULL,
            identity_id TEXT NOT NULL
        );",
    )
    .unwrap();

    DbConnection(Arc::new(Mutex::new(Some(conn))))
}

fn insert_identity(db: &DbConnection, id: &str, did: &str, source: &str) {
    let guard = db.0.lock().unwrap();
    let conn = guard.as_ref().unwrap();
    conn.execute(
        "INSERT INTO haex_identities (id, did, name, source) VALUES (?1, ?2, 'test', ?3)",
        rusqlite::params![id, did, source],
    )
    .unwrap();
}

fn insert_membership(db: &DbConnection, space_id: &str, identity_id: &str) {
    let guard = db.0.lock().unwrap();
    let conn = guard.as_ref().unwrap();
    conn.execute(
        "INSERT INTO haex_space_members (id, space_id, identity_id) VALUES (?1, ?2, ?3)",
        rusqlite::params![format!("m-{space_id}-{identity_id}"), space_id, identity_id],
    )
    .unwrap();
}

#[test]
fn unknown_did_is_not_contact() {
    let db = setup_test_db();
    let resolver = ContactResolver::new();
    assert!(!resolver.is_contact(&db, "did:key:stranger"));
}

#[test]
fn identity_row_marks_contact() {
    let db = setup_test_db();
    insert_identity(&db, "i-1", "did:key:friend", "contact");
    let resolver = ContactResolver::new();
    assert!(resolver.is_contact(&db, "did:key:friend"));
}

#[test]
fn own_identity_also_counts_as_contact() {
    let db = setup_test_db();
    insert_identity(&db, "i-1", "did:key:self", "own");
    let resolver = ContactResolver::new();
    assert!(resolver.is_contact(&db, "did:key:self"));
}

#[test]
fn shared_space_membership_marks_contact_without_contact_row() {
    let db = setup_test_db();
    insert_identity(&db, "i-2", "did:key:teammate", "remote");
    insert_membership(&db, "space-A", "i-2");
    let resolver = ContactResolver::new();
    assert!(resolver.is_contact(&db, "did:key:teammate"));
}

#[test]
fn cache_invalidate_all_clears_classification() {
    let db = setup_test_db();
    let resolver = ContactResolver::new();
    // Empty DB → NonContact, cached.
    assert!(!resolver.is_contact(&db, "did:key:added-later"));
    // Add row, but cache still holds NonContact until invalidated.
    insert_identity(&db, "i-3", "did:key:added-later", "contact");
    assert!(
        !resolver.is_contact(&db, "did:key:added-later"),
        "stale cache still reports NonContact"
    );
    resolver.invalidate_all();
    assert!(resolver.is_contact(&db, "did:key:added-later"));
}

#[test]
fn cache_invalidate_single_did_targets_only_that_did() {
    let db = setup_test_db();
    let resolver = ContactResolver::new();
    assert!(!resolver.is_contact(&db, "did:key:a"));
    assert!(!resolver.is_contact(&db, "did:key:b"));
    insert_identity(&db, "i-a", "did:key:a", "contact");
    insert_identity(&db, "i-b", "did:key:b", "contact");
    resolver.invalidate("did:key:a");
    assert!(resolver.is_contact(&db, "did:key:a"));
    assert!(
        !resolver.is_contact(&db, "did:key:b"),
        "did:key:b's NonContact cache still applies"
    );
}
