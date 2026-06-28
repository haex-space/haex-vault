use rusqlite::Connection;
use std::sync::{Arc, Mutex};

use super::contacts::ContactResolver;
use crate::crdt::hlc::HlcService;
use crate::database::connection_context::ConnectionContext;
use crate::database::core::{install_tx_hlc_hooks, register_current_hlc_udf};
use crate::database::DbConnection;
use crate::table_names::{TABLE_CRDT_CONFIGS, TABLE_CRDT_DIRTY_TABLES};

const OWN_IDENTITY_ID: &str = "self";
const OWN_DID: &str = "did:key:self";

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

    // Production-shape schema (subset). `private_key NOT NULL` separates
    // own identities from imported contacts — the shared-space check uses
    // it to make sure WE are also in the space the peer claims.
    conn.execute_batch(
        "CREATE TABLE haex_identities (
            id TEXT PRIMARY KEY NOT NULL,
            did TEXT NOT NULL,
            name TEXT NOT NULL,
            source TEXT NOT NULL DEFAULT 'contact',
            private_key TEXT
        );
        CREATE TABLE haex_space_members (
            id TEXT PRIMARY KEY NOT NULL,
            space_id TEXT NOT NULL,
            identity_id TEXT NOT NULL
        );",
    )
    .unwrap();

    let db = DbConnection(Arc::new(Mutex::new(Some(conn))));

    // Pre-insert our own identity so shared-space tests have a "WE" side
    // to join against without each test having to repeat the boilerplate.
    insert_identity(&db, OWN_IDENTITY_ID, OWN_DID, "own", Some("PRIVKEY-OWN"));
    db
}

fn insert_identity(
    db: &DbConnection,
    id: &str,
    did: &str,
    source: &str,
    private_key: Option<&str>,
) {
    let guard = db.0.lock().unwrap();
    let conn = guard.as_ref().unwrap();
    conn.execute(
        "INSERT INTO haex_identities (id, did, name, source, private_key) VALUES (?1, ?2, 'test', ?3, ?4)",
        rusqlite::params![id, did, source, private_key],
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
    insert_identity(&db, "i-1", "did:key:friend", "contact", None);
    let resolver = ContactResolver::new();
    assert!(resolver.is_contact(&db, "did:key:friend"));
}

#[test]
fn own_identity_also_counts_as_contact() {
    // setup_test_db already inserted OWN_DID as an own identity.
    let db = setup_test_db();
    let resolver = ContactResolver::new();
    assert!(resolver.is_contact(&db, OWN_DID));
}

#[test]
fn shared_space_with_own_identity_marks_contact_without_contact_row() {
    let db = setup_test_db();
    insert_identity(&db, "i-2", "did:key:teammate", "remote", None);
    insert_membership(&db, "space-A", OWN_IDENTITY_ID);
    insert_membership(&db, "space-A", "i-2");
    let resolver = ContactResolver::new();
    assert!(resolver.is_contact(&db, "did:key:teammate"));
}

#[test]
fn shared_space_branch_requires_own_identity_in_same_space() {
    // Regression for the over-broad shared-space join: when a peer has
    // ONLY a membership row (no `haex_identities` row at all), the
    // resolver must NOT classify them as a contact through a co-membership
    // unless one of our OWN identities is also a member of that same
    // space. The strict join `i_own.private_key IS NOT NULL` enforces
    // this. CodeRabbit review on PR #562.
    //
    // Two stale-state scenarios merged into one test:
    //   (a) peer's membership references a removed identity (no `i_remote`
    //       row) — shared-space branch can't even join, returns false.
    //   (b) peer has both identity + membership in `space-B`, but WE
    //       have no membership for `space-B` — shared-space branch
    //       traverses but the `i_own` filter rejects it.
    // The identity-row scenario (b) still passes via the first branch in
    // the current schema (any identity row counts as contact); we don't
    // re-test that here because it's covered by `identity_row_marks_contact`.
    let db = setup_test_db();
    // (a) Membership without an identity row for the peer.
    insert_membership(&db, "space-B", "i-orphaned");
    let resolver = ContactResolver::new();
    assert!(
        !resolver.is_contact(&db, "did:key:orphaned"),
        "shared-space branch must not match when the peer has no identity row"
    );
}

#[test]
fn miss_is_not_cached_so_next_lookup_re_queries() {
    // Negative results are intentionally NOT cached (see ContactResolver
    // doc on PR #562) — a DID added to the DB right after a miss must be
    // picked up on the very next call, without needing `invalidate_all`.
    let db = setup_test_db();
    let resolver = ContactResolver::new();
    assert!(!resolver.is_contact(&db, "did:key:added-later"));
    insert_identity(&db, "i-late", "did:key:added-later", "contact", None);
    assert!(
        resolver.is_contact(&db, "did:key:added-later"),
        "negative result must not be cached — next lookup must hit DB",
    );
}

#[test]
fn positive_cache_short_circuits_db() {
    // First call hits DB and remembers Contact. Even if the row is then
    // deleted, the resolver keeps returning true until `invalidate_all` /
    // `invalidate` is called from the CRDT-write hook.
    let db = setup_test_db();
    insert_identity(&db, "i-cached", "did:key:cached", "contact", None);
    let resolver = ContactResolver::new();
    assert!(resolver.is_contact(&db, "did:key:cached"));

    // Wipe the row underneath the cache to prove the cache is the source.
    {
        let guard = db.0.lock().unwrap();
        guard
            .as_ref()
            .unwrap()
            .execute("DELETE FROM haex_identities WHERE id = 'i-cached'", [])
            .unwrap();
    }
    assert!(
        resolver.is_contact(&db, "did:key:cached"),
        "positive cache must short-circuit further DB lookups",
    );

    resolver.invalidate("did:key:cached");
    assert!(
        !resolver.is_contact(&db, "did:key:cached"),
        "invalidate must force the next lookup to re-query the DB",
    );
}

#[test]
fn invalidate_all_clears_positive_cache() {
    let db = setup_test_db();
    insert_identity(&db, "i-x", "did:key:x", "contact", None);
    let resolver = ContactResolver::new();
    assert!(resolver.is_contact(&db, "did:key:x"));
    {
        let guard = db.0.lock().unwrap();
        guard
            .as_ref()
            .unwrap()
            .execute("DELETE FROM haex_identities WHERE id = 'i-x'", [])
            .unwrap();
    }
    resolver.invalidate_all();
    assert!(!resolver.is_contact(&db, "did:key:x"));
}
