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
