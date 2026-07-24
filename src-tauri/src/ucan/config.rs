//! Vault-global settings for UCAN chain verification.
//!
//! `max_ucan_chain_depth` is the maximum number of **tokens** (chain
//! nodes — leaf + every ancestor up to and including the self-signed
//! root) the chain walker in [`crate::ucan::verify::validate_token`]
//! will visit before rejecting. A limit of `1` accepts only a self-signed
//! root; a one-edge delegation (root → leaf) needs `2`. Higher values
//! allow richer sub-delegation trees (sub-admins, chat-owner roles) at
//! negligible cost — verification is `O(N)` ed25519 signature checks
//! per token. Bounded to protect against pathological chains that
//! would let a malicious peer waste CPU. See [`walk_prf_chain`] for
//! the authoritative counting rule.
//!
//! [`walk_prf_chain`]: crate::ucan::verify
//!
//! ## Storage & threat model
//!
//! Persisted in `haex_vault_settings` with a single row per vault
//! (`device_id = NULL`). This table is owner-only synced — CRDT sync of
//! `haex_*` tables goes owner-to-owner via the owner-sync-server pathway
//! only; a foreign peer cannot inject a row that would weaken the depth
//! cap on someone else's device.
//!
//! Reads go **directly through rusqlite** (not `select_with_crdt`) so the
//! verifier's depth cap is not exposed to CRDT-level tombstone or shadow
//! semantics. This is a plain SELECT — `sql-must-use-crdt-helpers` applies
//! to writes on synced tables, not reads.

use rusqlite::Connection;

/// Default depth if no row is present or the stored value is invalid.
///
/// Counts **tokens** — five tokens is a five-node chain, i.e. root plus
/// up to four delegation hops. Chosen empirically: covers
/// space-root → admin → sub-admin → member → device-holder chains with
/// room for one intermediate layer, which is deeper than any current
/// Phase-2 use case actually needs.
pub const MAX_UCAN_CHAIN_DEPTH_DEFAULT: u32 = 5;

/// Minimum permitted value. Must be `>= 1` — a depth of 0 would admit no
/// tokens at all, breaking every authorised operation.
pub const MAX_UCAN_CHAIN_DEPTH_MIN: u32 = 1;

/// Maximum permitted value. Bounds worst-case walk cost at `O(N)`
/// ed25519-verify operations. 20 comfortably exceeds any realistic
/// delegation topology while keeping a hard DoS ceiling.
pub const MAX_UCAN_CHAIN_DEPTH_MAX: u32 = 20;

/// Settings-table key. Kept in this module so renames stay grep-able and
/// so callers cannot mix up prefixes.
pub const MAX_UCAN_CHAIN_DEPTH_KEY: &str = "max_ucan_chain_depth";

/// Read the configured `max_ucan_chain_depth` from `haex_vault_settings`.
///
/// Row selection is `WHERE key = 'max_ucan_chain_depth' AND device_id IS
/// NULL LIMIT 1` — this is the single vault-global row. Per-device rows
/// (`device_id = <did>`) are intentionally **not** matched here; the depth
/// cap is a vault-wide security setting.
///
/// Returns [`MAX_UCAN_CHAIN_DEPTH_DEFAULT`] when:
/// - the row is missing (first-run vault, never configured),
/// - the value column is `NULL`,
/// - the stored string does not parse as `u32`, or
/// - the parsed value falls outside `[MIN, MAX]` inclusive.
///
/// A DB error also collapses to the default rather than propagating —
/// the verifier must fail *closed* on token validation, but the depth
/// cap itself defaulting to a safe value is the right recovery.
pub fn read_max_ucan_chain_depth(conn: &Connection) -> u32 {
    conn.query_row(
        "SELECT value FROM haex_vault_settings \
         WHERE key = ?1 AND device_id IS NULL \
         ORDER BY id LIMIT 1",
        [MAX_UCAN_CHAIN_DEPTH_KEY],
        |row| row.get::<_, Option<String>>(0),
    )
    .ok()
    .flatten()
    .and_then(|s| s.parse::<u32>().ok())
    .filter(|&n| (MAX_UCAN_CHAIN_DEPTH_MIN..=MAX_UCAN_CHAIN_DEPTH_MAX).contains(&n))
    .unwrap_or(MAX_UCAN_CHAIN_DEPTH_DEFAULT)
}

#[cfg(test)]
mod tests;
