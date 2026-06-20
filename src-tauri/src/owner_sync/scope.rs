//! Scope resolution for owner-vault sync.

use rusqlite::{Connection, OptionalExtension};

/// The `haex_spaces.type` value identifying the local vault space.
/// Mirrors `SpaceType.VAULT` in `src/database/constants.ts`.
const VAULT_SPACE_TYPE: &str = "vault";

/// Resolve the DID of the identity that owns the VAULT-type space.
///
/// Joins `haex_spaces` (the row whose `type` is the vault type) to
/// `haex_identities` via `haex_spaces.owner_identity_id = haex_identities.id`
/// and returns that identity's `did`. Returns `Ok(None)` when there is no
/// vault space.
pub fn resolve_vault_owner_did(conn: &Connection) -> rusqlite::Result<Option<String>> {
    conn.query_row(
        "SELECT i.did FROM haex_spaces s \
         JOIN haex_identities i ON i.id = s.owner_identity_id \
         WHERE s.type = ?1 \
         LIMIT 1",
        [VAULT_SPACE_TYPE],
        |row| row.get(0),
    )
    .optional()
}

/// How a verified peer relates to this vault's owner.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PeerClass {
    /// The peer proved (via DID-auth) it holds the same vault-owner DID, so it
    /// is another device of the owner and may receive the full vault.
    OwnerDevice,
    /// Any other peer; it may only receive the narrow space-scoped table set.
    Foreign,
}

/// Classify a peer by comparing its verified DID against the vault-owner DID.
///
/// Exact, case-sensitive string equality — DIDs are case-sensitive identifiers,
/// so no trimming or normalization is applied.
pub fn classify_peer(verified_did: &str, vault_owner_did: &str) -> PeerClass {
    if verified_did == vault_owner_did {
        PeerClass::OwnerDevice
    } else {
        PeerClass::Foreign
    }
}

/// Select which CRDT tables a peer of the given class may receive.
///
/// A [`PeerClass::OwnerDevice`] receives the full `all_crdt_tables` set; a
/// [`PeerClass::Foreign`] peer receives only `space_scoped_tables`. This is a
/// pure decision over the two provided lists — the caller supplies the real
/// table sets in a later phase.
pub fn select_sync_scope(
    class: PeerClass,
    all_crdt_tables: &[String],
    space_scoped_tables: &[String],
) -> Vec<String> {
    match class {
        PeerClass::OwnerDevice => all_crdt_tables.to_vec(),
        PeerClass::Foreign => space_scoped_tables.to_vec(),
    }
}
