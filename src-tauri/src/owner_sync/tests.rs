use rusqlite::Connection;
use uuid::Uuid;

use crate::owner_sync::scope::{
    classify_peer, owner_route_decision, resolve_owner_device_endpoints, resolve_vault_owner_did,
    resolve_vault_space_id, PeerClass,
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
