use std::sync::{Arc, Mutex};

use rusqlite::Connection;
use uuid::Uuid;

use crate::database::DbConnection;
use crate::owner_sync::scope::{
    classify_peer, owner_route_decision, resolve_local_member_did_for_space,
    resolve_owner_device_endpoints, resolve_vault_owner_did, resolve_vault_space_id, PeerClass,
};

/// Create the minimal subset of `haex_identities` + `haex_spaces` the
/// resolver joins over. Columns mirror the production Drizzle schema
/// (`src/database/schemas/identity.ts`, `src/database/schemas/spaces.ts`)
/// for the `NOT NULL` constraints the JOIN depends on; purely cosmetic
/// columns are omitted.
fn setup_db() -> Connection {
    let conn = Connection::open_in_memory().expect("in-memory DB");
    conn.execute_batch(
        "CREATE TABLE haex_identities (
            id TEXT PRIMARY KEY NOT NULL,
            did TEXT NOT NULL,
            name TEXT NOT NULL,
            source TEXT NOT NULL DEFAULT 'contact',
            private_key TEXT
        );
        CREATE TABLE haex_spaces (
            id TEXT PRIMARY KEY NOT NULL,
            type TEXT NOT NULL DEFAULT 'online',
            status TEXT NOT NULL DEFAULT 'active',
            name TEXT NOT NULL,
            owner_identity_id TEXT NOT NULL
        );",
    )
    .expect("create tables");
    conn
}

#[test]
fn returns_owner_did_of_vault_space() {
    let conn = setup_db();
    let identity_id = Uuid::new_v4().to_string();
    let owner_did = format!("did:key:{}", Uuid::new_v4());

    conn.execute(
        "INSERT INTO haex_identities (id, did, name, source) VALUES (?1, ?2, 'Me', 'own')",
        rusqlite::params![identity_id, owner_did],
    )
    .unwrap();
    conn.execute(
        "INSERT INTO haex_spaces (id, type, name, owner_identity_id) \
         VALUES (?1, 'vault', 'My Vault', ?2)",
        rusqlite::params![Uuid::new_v4().to_string(), identity_id],
    )
    .unwrap();

    let resolved = resolve_vault_owner_did(&conn);
    assert_eq!(resolved, Ok(Some(owner_did)));
}

#[test]
fn returns_none_when_no_vault_space() {
    let conn = setup_db();
    let identity_id = Uuid::new_v4().to_string();
    let some_did = format!("did:key:{}", Uuid::new_v4());

    conn.execute(
        "INSERT INTO haex_identities (id, did, name, source) VALUES (?1, ?2, 'Me', 'own')",
        rusqlite::params![identity_id, some_did],
    )
    .unwrap();
    // A non-vault space owned by the same identity must not match.
    conn.execute(
        "INSERT INTO haex_spaces (id, type, name, owner_identity_id) \
         VALUES (?1, 'online', 'A Shared Space', ?2)",
        rusqlite::params![Uuid::new_v4().to_string(), identity_id],
    )
    .unwrap();

    let resolved = resolve_vault_owner_did(&conn);
    assert_eq!(resolved, Ok(None));
}

#[test]
fn classifies_matching_did_as_owner_device() {
    let owner_did = format!("did:key:{}", Uuid::new_v4());
    assert_eq!(
        classify_peer(&owner_did, &owner_did),
        PeerClass::OwnerDevice
    );
}

#[test]
fn classifies_different_did_as_foreign() {
    let owner_did = format!("did:key:{}", Uuid::new_v4());
    let peer_did = format!("did:key:{}", Uuid::new_v4());
    assert_eq!(classify_peer(&peer_did, &owner_did), PeerClass::Foreign);
}

#[test]
fn classification_is_case_sensitive_and_untrimmed() {
    let owner_did = "did:key:zABC".to_string();
    // Case difference must not match — DIDs are case-sensitive identifiers.
    assert_eq!(
        classify_peer("did:key:zabc", &owner_did),
        PeerClass::Foreign
    );
    // Surrounding whitespace must not be normalized away.
    assert_eq!(
        classify_peer(" did:key:zABC", &owner_did),
        PeerClass::Foreign
    );
}

/// Create the minimal subset of `haex_devices` the endpoint resolver scans.
/// Columns mirror the production Drizzle schema (`src/database/schemas/devices.ts`)
/// for the `NOT NULL` constraints; purely cosmetic columns are omitted.
fn setup_devices_db() -> Connection {
    let conn = Connection::open_in_memory().expect("in-memory DB");
    conn.execute_batch(
        "CREATE TABLE haex_devices (
            id TEXT PRIMARY KEY NOT NULL,
            owner_did TEXT NOT NULL,
            endpoint_id TEXT NOT NULL,
            name TEXT NOT NULL,
            platform TEXT NOT NULL
        );",
    )
    .expect("create table");
    conn
}

fn insert_device(conn: &Connection, owner_did: &str, endpoint_id: &str) {
    conn.execute(
        "INSERT INTO haex_devices (id, owner_did, endpoint_id, name, platform) \
         VALUES (?1, ?2, ?3, 'Device', 'desktop')",
        rusqlite::params![Uuid::new_v4().to_string(), owner_did, endpoint_id],
    )
    .unwrap();
}

#[test]
fn enumerates_other_owner_devices_excluding_self() {
    let conn = setup_devices_db();
    let owner_did = format!("did:key:{}", Uuid::new_v4());
    let other_owner_did = format!("did:key:{}", Uuid::new_v4());
    let self_ep = format!("ep-{}", Uuid::new_v4());
    let ep_b = format!("ep-{}", Uuid::new_v4());
    let ep_c = format!("ep-{}", Uuid::new_v4());

    // (a) own device, (b) owner's other device, (c) a foreign owner's device.
    insert_device(&conn, &owner_did, &self_ep);
    insert_device(&conn, &owner_did, &ep_b);
    insert_device(&conn, &other_owner_did, &ep_c);

    let resolved = resolve_owner_device_endpoints(&conn, &owner_did, &self_ep);
    assert_eq!(resolved, Ok(vec![ep_b]));
}

#[test]
fn returns_empty_when_only_self() {
    let conn = setup_devices_db();
    let owner_did = format!("did:key:{}", Uuid::new_v4());
    let self_ep = format!("ep-{}", Uuid::new_v4());

    insert_device(&conn, &owner_did, &self_ep);

    let resolved = resolve_owner_device_endpoints(&conn, &owner_did, &self_ep);
    assert_eq!(resolved, Ok(Vec::<String>::new()));
}

#[test]
fn resolve_vault_space_id_returns_vault_row() {
    let conn = setup_db();
    let identity_id = Uuid::new_v4().to_string();
    let some_did = format!("did:key:{}", Uuid::new_v4());
    let space_id = Uuid::new_v4().to_string();

    conn.execute(
        "INSERT INTO haex_identities (id, did, name, source) VALUES (?1, ?2, 'Me', 'own')",
        rusqlite::params![identity_id, some_did],
    )
    .unwrap();
    conn.execute(
        "INSERT INTO haex_spaces (id, type, name, owner_identity_id) \
         VALUES (?1, 'vault', 'My Vault', ?2)",
        rusqlite::params![space_id, identity_id],
    )
    .unwrap();

    let resolved = resolve_vault_space_id(&conn);
    assert_eq!(resolved, Ok(Some(space_id)));
}

#[test]
fn resolve_vault_space_id_returns_none_without_vault() {
    let conn = setup_db();
    let identity_id = Uuid::new_v4().to_string();
    let some_did = format!("did:key:{}", Uuid::new_v4());

    conn.execute(
        "INSERT INTO haex_identities (id, did, name, source) VALUES (?1, ?2, 'Me', 'own')",
        rusqlite::params![identity_id, some_did],
    )
    .unwrap();
    // Only a non-vault space present.
    conn.execute(
        "INSERT INTO haex_spaces (id, type, name, owner_identity_id) \
         VALUES (?1, 'online', 'A Shared Space', ?2)",
        rusqlite::params![Uuid::new_v4().to_string(), identity_id],
    )
    .unwrap();

    let resolved = resolve_vault_space_id(&conn);
    assert_eq!(resolved, Ok(None));
}

// ------------------------------------------------------------------
// owner_route_decision — THE serving-side security gate.
//
// Returns true (→ serve full vault) only when the verified DID equals
// the vault-owner DID AND the targeted space is the vault space.
// Any other combination must return false so the connection falls
// through to the existing space-scoped path (UCAN/membership-gated).
// ------------------------------------------------------------------

/// Same owner DID + the vault space id → serve the full vault.
#[test]
fn owner_route_decision_same_did_vault_sid_is_true() {
    let owner_did = format!("did:key:{}", Uuid::new_v4());
    let vault_sid = Uuid::new_v4().to_string();
    assert!(owner_route_decision(
        &owner_did, &vault_sid, &owner_did, &vault_sid
    ));
}

/// CRITICAL LEAK GUARD: a *different* (foreign) verified DID targeting the
/// vault space must NOT route to the full-vault path, even though the sid
/// matches the vault space. A true here would hand the entire vault
/// (passwords, identities, …) to a non-owner peer.
#[test]
fn owner_route_decision_foreign_did_vault_sid_is_false() {
    let owner_did = format!("did:key:{}", Uuid::new_v4());
    let foreign_did = format!("did:key:{}", Uuid::new_v4());
    let vault_sid = Uuid::new_v4().to_string();
    assert!(!owner_route_decision(
        &foreign_did,
        &vault_sid,
        &owner_did,
        &vault_sid
    ));
}

/// Same owner DID but targeting a NON-vault space → false. The owner path
/// is only for the vault space; other spaces of the owner still use the
/// normal space dispatch.
#[test]
fn owner_route_decision_same_did_non_vault_sid_is_false() {
    let owner_did = format!("did:key:{}", Uuid::new_v4());
    let vault_sid = Uuid::new_v4().to_string();
    let other_sid = Uuid::new_v4().to_string();
    assert!(!owner_route_decision(
        &owner_did, &other_sid, &owner_did, &vault_sid
    ));
}

/// Empty / edge DIDs must never accidentally match. An empty verified DID
/// (e.g. a malformed handshake result) against an empty owner DID would be
/// string-equal — but the vault-owner DID is never empty in production, so
/// the realistic edge is "empty verified DID, real owner DID" → false.
#[test]
fn owner_route_decision_empty_verified_did_is_false() {
    let owner_did = format!("did:key:{}", Uuid::new_v4());
    let vault_sid = Uuid::new_v4().to_string();
    assert!(!owner_route_decision(
        "", &vault_sid, &owner_did, &vault_sid
    ));
}

/// Case difference in the DID must not match — DIDs are case-sensitive.
#[test]
fn owner_route_decision_case_mismatch_is_false() {
    let vault_sid = Uuid::new_v4().to_string();
    assert!(!owner_route_decision(
        "did:key:zabc",
        &vault_sid,
        "did:key:zABC",
        &vault_sid
    ));
}

// ------------------------------------------------------------------
// resolve_local_member_did_for_space — per-space local-identity lookup.
//
// Predicate is `source='own' AND private_key IS NOT NULL` on
// `haex_identities`, joined via `haex_space_members`. Zero rows →
// Ok(None); multiple qualifying local identities → error, because the
// resolver must never choose a DID implicitly.
// ------------------------------------------------------------------

/// Minimal schema for the resolver: `haex_identities` plus
/// `haex_space_members`. Columns mirror the production Drizzle schema
/// (`src/database/schemas/identity.ts`,
/// `src/database/schemas/space_members.ts`) for the fields the query
/// touches; purely cosmetic columns are omitted.
fn setup_members_db() -> DbConnection {
    let conn = Connection::open_in_memory().expect("in-memory DB");
    conn.execute_batch(
        "CREATE TABLE haex_identities (
            id TEXT PRIMARY KEY NOT NULL,
            did TEXT NOT NULL,
            name TEXT NOT NULL,
            source TEXT NOT NULL DEFAULT 'contact',
            private_key TEXT,
            created_at TEXT NOT NULL
        );
        CREATE TABLE haex_space_members (
            id TEXT PRIMARY KEY NOT NULL,
            space_id TEXT NOT NULL,
            identity_id TEXT NOT NULL,
            UNIQUE (space_id, identity_id)
        );",
    )
    .expect("create tables");
    DbConnection(Arc::new(Mutex::new(Some(conn))))
}

fn insert_identity(
    db: &DbConnection,
    id: &str,
    did: &str,
    source: &str,
    private_key: Option<&str>,
    created_at: &str,
) {
    let guard = db.0.lock().expect("db lock");
    let conn = guard.as_ref().expect("db open");
    conn.execute(
        "INSERT INTO haex_identities (id, did, name, source, private_key, created_at) \
         VALUES (?1, ?2, 'Person', ?3, ?4, ?5)",
        rusqlite::params![id, did, source, private_key, created_at],
    )
    .unwrap();
}

fn insert_membership(db: &DbConnection, space_id: &str, identity_id: &str) {
    let guard = db.0.lock().expect("db lock");
    let conn = guard.as_ref().expect("db open");
    conn.execute(
        "INSERT INTO haex_space_members (id, space_id, identity_id) VALUES (?1, ?2, ?3)",
        rusqlite::params![Uuid::new_v4().to_string(), space_id, identity_id],
    )
    .unwrap();
}

/// A REMOTE identity (source='contact', private_key IS NULL) that is a
/// member of the space must not qualify as a local member — the
/// predicate `source='own' AND private_key IS NOT NULL` excludes it —
/// so the resolver returns `None`, not a stale remote DID.
#[test]
fn resolve_local_member_did_for_space_returns_none_when_no_local_identity_is_member() {
    let conn = setup_members_db();
    let space_id = Uuid::new_v4().to_string();
    let remote_id = Uuid::new_v4().to_string();
    let remote_did = format!("did:key:{}", Uuid::new_v4());

    insert_identity(
        &conn,
        &remote_id,
        &remote_did,
        "contact",
        None,
        "2020-01-01",
    );
    insert_membership(&conn, &space_id, &remote_id);

    let resolved = resolve_local_member_did_for_space(&conn, &space_id).expect("query ok");
    assert!(resolved.is_none(), "no local identity → None");
}

/// Two local identities joined to the same space are ambiguous. The caller
/// must obtain an explicitly selected DID instead of silently choosing one.
#[test]
fn resolve_local_member_did_for_space_rejects_ambiguous_local_identities() {
    let conn = setup_members_db();
    let space_id = Uuid::new_v4().to_string();

    let id_a = "id-A".to_string();
    let did_a = "did:key:zAlpha".to_string();
    let id_b = "id-B".to_string();
    let did_b = "did:key:zBeta".to_string();

    insert_identity(&conn, &id_b, &did_b, "own", Some("privkey-B"), "2020-01-02");
    insert_identity(&conn, &id_a, &did_a, "own", Some("privkey-A"), "2020-01-01");
    insert_membership(&conn, &space_id, &id_b);
    insert_membership(&conn, &space_id, &id_a);

    let err = resolve_local_member_did_for_space(&conn, &space_id)
        .expect_err("ambiguous membership must require explicit DID selection");
    assert!(err.to_string().contains("ambiguous local membership"));
}
