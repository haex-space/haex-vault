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

/// Resolve the DID of the LOCAL identity that joined `space_id`.
///
/// LOCAL identity = a row in `haex_identities` with `source = 'own'` AND a
/// non-null `private_key` (this vault holds the signing key). Per CLAUDE.md
/// ("DID-Auswahl beim Space-Beitritt ist immer explizit — niemals auto-default
/// auf eine bestehende DID"), a user can join a shared space with an identity
/// distinct from the vault owner's, so callers that need to name the viewer
/// for per-member ScopedCred lookups MUST use this resolver — NOT
/// [`resolve_vault_owner_did`], which returns the DID that owns the local
/// vault space and would silently miss non-vault-owner memberships.
///
/// Returns:
/// - `Ok(Some(did))` — the single local DID that joined the space.
/// - `Ok(None)` — this vault has no local identity mapped as a member of
///   `space_id`.
/// - `Err` — SQL failure.
///
/// The schema's `UNIQUE (space_id, identity_id)` index guarantees at most
/// one row per (space, identity) pair, but nothing forbids two DIFFERENT
/// local identities from joining the same space. Two-rows tie-break:
/// `ORDER BY created_at ASC, id ASC`. Deterministic WITHIN a single
/// vault-view (given the same set of local identities), but NOT
/// necessarily identical across devices — a second local identity
/// created on device A may not exist on device B, and `created_at` is a
/// local wall-clock timestamp at insert. If the vault has more than one
/// local identity joined to the same space, this is a pathological
/// configuration; the resolver emits `tracing::warn!` and picks the
/// earliest to keep the system moving.
pub fn resolve_local_member_did_for_space(
    conn: &Connection,
    space_id: &str,
) -> rusqlite::Result<Option<String>> {
    let mut stmt = conn.prepare(
        "SELECT DISTINCT i.did \
         FROM haex_identities i \
         JOIN haex_space_members m ON m.identity_id = i.id \
         WHERE m.space_id = ?1 \
           AND i.source = 'own' \
           AND i.private_key IS NOT NULL \
         ORDER BY i.created_at ASC, i.id ASC \
         LIMIT 2",
    )?;
    let dids: Vec<String> = stmt
        .query_map([space_id], |row| row.get::<_, String>(0))?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    match dids.len() {
        0 => Ok(None),
        1 => Ok(dids.into_iter().next()),
        _ => {
            tracing::warn!(
                space_id = %space_id,
                "ambiguous local membership: multiple local identities joined this space; \
                 picking earliest-created deterministically"
            );
            Ok(dids.into_iter().next())
        }
    }
}

/// Enumerate the iroh `endpoint_id`s of the owner's OTHER devices.
///
/// Selects every `haex_devices` row owned by `owner_did` except the one whose
/// `endpoint_id` is `own_endpoint_id`, so the caller can connect to the owner's
/// remaining devices for owner-vault sync. The `IS NOT NULL` guard is defensive
/// against pre-`endpoint_id` rows.
pub fn resolve_owner_device_endpoints(
    conn: &Connection,
    owner_did: &str,
    own_endpoint_id: &str,
) -> rusqlite::Result<Vec<String>> {
    let mut stmt = conn.prepare(
        "SELECT endpoint_id FROM haex_devices \
         WHERE owner_did = ?1 AND endpoint_id != ?2 AND endpoint_id IS NOT NULL",
    )?;
    let rows = stmt.query_map([owner_did, own_endpoint_id], |row| row.get::<_, String>(0))?;
    rows.collect()
}

/// Resolve the id of the VAULT-type space.
///
/// The VAULT space id doubles as the owner-sync cursor namespace. Returns
/// `Ok(None)` when there is no vault space.
pub fn resolve_vault_space_id(conn: &Connection) -> rusqlite::Result<Option<String>> {
    conn.query_row(
        "SELECT id FROM haex_spaces WHERE type = ?1 LIMIT 1",
        [VAULT_SPACE_TYPE],
        |row| row.get(0),
    )
    .optional()
}

/// Is `space_id` the current vault's owner-space (the `type='vault'` row)?
///
/// Used by the CRDT apply-gate to decide whether per-column signature
/// enforcement applies to an inbound batch:
///
/// - **Owner-space (`true`)** → sig enforcement OFF. Owner-vault sync between
///   two devices of the same identity is deliberately unsigned on the write
///   side ([`crate::crdt::column_sig::write::sign_column_for_spaces`] only
///   signs rows the register maps into a space; owner-private rows carry
///   `haex_column_sigs = {}`). Peer legitimacy is already established by
///   QUIC-level DID auth plus the peer's row in `haex_space_devices`, so
///   per-column sig enforcement adds nothing on top and would drop every
///   unsigned owner-private CRDT change on the receiver.
/// - **Non-owner space (`false`)** → sig enforcement ON. Shared-space apply
///   keeps the strict "unsigned changes are dropped" behavior from ADR 0002
///   Phase 1.
///
/// A vault with no `type='vault'` row (e.g. a fresh install in the middle of
/// setup, or a minimal test schema that never creates `haex_spaces`) returns
/// `false`, so the strict shared-space semantics remain the safe default
/// when in doubt.
pub fn is_owner_space(conn: &Connection, space_id: &str) -> rusqlite::Result<bool> {
    // Defensive against very early setup / minimal test schemas: if
    // `haex_spaces` does not exist yet, treat this space as NOT the owner-
    // space (the strict shared-space gate remains active).
    let has_spaces_table: i64 = conn.query_row(
        "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name='haex_spaces'",
        [],
        |r| r.get(0),
    )?;
    if has_spaces_table == 0 {
        return Ok(false);
    }
    Ok(resolve_vault_space_id(conn)?.as_deref() == Some(space_id))
}

/// Resolve the vault-owner DID and vault space id in a single atomic query.
///
/// This is the serving-gate inputs combined: the routing decision must compare
/// the verified peer DID against the OWNER of the same vault row whose id is
/// the target space id — so both values must come from one row. Reading them
/// via two separate `LIMIT 1` queries would, if the schema invariant of a
/// single vault row were ever violated, allow the gate to combine
/// `owner_did` from row A with `space_id` from row B. Returns `Ok(None)` when
/// no vault space exists.
pub fn resolve_vault_owner_route_context(
    conn: &Connection,
) -> rusqlite::Result<Option<(String, String)>> {
    conn.query_row(
        "SELECT i.did, s.id \
         FROM haex_spaces s \
         JOIN haex_identities i ON i.id = s.owner_identity_id \
         WHERE s.type = ?1 \
         LIMIT 1",
        [VAULT_SPACE_TYPE],
        |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?)),
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

/// The serving-side routing gate: should a verified peer be served the FULL
/// vault instead of the narrow space-scoped path?
///
/// Returns `true` only when BOTH hold:
///
/// 1. the cryptographically `verified_did` is the SAME as `vault_owner_did`
///    (so the peer is another device of this vault's owner), **and**
/// 2. the request targets `vault_space_id` (the owner-sync namespace).
///
/// Every other combination returns `false`, so the connection falls through
/// to the existing space-scoped dispatch where UCAN + membership are enforced.
///
/// # Security
///
/// This is THE gate that decides full-vault exposure. A `false`-negative just
/// downgrades an owner device to the space path (no leak); a `false`-positive
/// would hand the entire vault — passwords, identities, vault settings — to a
/// non-owner peer. It is therefore deliberately a single, isolated, exact
/// equality over already-verified inputs: `verified_did` MUST come from the
/// `quic_did_auth` handshake (never the request payload), and `vault_owner_did`
/// MUST be the DID resolved from the local `haex_spaces`/`haex_identities`
/// join. No trimming, no normalization, no substring matching.
pub fn owner_route_decision(
    verified_did: &str,
    target_space_id: &str,
    vault_owner_did: &str,
    vault_space_id: &str,
) -> bool {
    classify_peer(verified_did, vault_owner_did) == PeerClass::OwnerDevice
        && target_space_id == vault_space_id
}
