//! SpaceKeyCache reload/evict helpers for space-lifecycle events.
//!
//! Task C4 of the shared-space Phase 1 column-sig plan wires cache
//! invalidation into the four Rust-side space-mutation entry points:
//!
//! - `local_delivery_start` (owner side, after `createLocalSpace`)
//! - `local_delivery_stop` (owner side, on leader teardown / leave)
//! - `local_delivery_claim_invite` (claimant side, right after the space is
//!   persisted locally)
//! - `handle_claim_invite` (leader side, after the new member is committed)
//!
//! All four sites route through these two thin wrappers so the invariant
//! "cache mutation only after a successful DB commit; failures are
//! swallowed" is enforced in one place rather than duplicated per handler.

use crate::crdt::column_sig::key_cache::SpaceKeyCache;
use crate::database::core::with_connection;
use crate::database::DbConnection;

/// Best-effort SpaceKeyCache warm-up after a space-mutation event.
///
/// A failed reload is silently swallowed — the cache's own `get_or_reload`
/// keeps signing correct via JIT reload on the next call. This is a
/// warm-the-cache-eagerly optimisation, not a correctness gate.
pub(crate) fn warm_column_sig_cache(cache: &SpaceKeyCache, db: &DbConnection, space_id: &str) {
    let _ = with_connection(db, |conn| {
        let _ = cache.get_or_reload(conn, space_id);
        Ok(())
    });
}

/// Drop a `space_id` entry from the SpaceKeyCache.
///
/// Fired on leader-mode stop / space removal. Stale entries are
/// functionally harmless (a subsequent sign would fail on the missing
/// `haex_space_members` row) but bloat the map over long sessions.
pub(crate) fn drop_column_sig_cache(cache: &SpaceKeyCache, space_id: &str) {
    cache.remove(space_id);
}

#[cfg(test)]
#[path = "column_sig_hook_tests.rs"]
mod tests;
