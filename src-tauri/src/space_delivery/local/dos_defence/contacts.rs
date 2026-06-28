//! Contact-DID resolver for the DDoS contacts-only escalation.
//!
//! A DID counts as a "contact" if **either**:
//!
//! 1. A row exists in `haex_identities` with `did = ?` (the unified
//!    identities table holds contacts via `source = 'contact'`, but here
//!    we accept any non-null row — own DIDs included — because an attacker
//!    impersonating our own DID is also non-random and would not benefit
//!    from being silent-dropped at L1).
//! 2. The DID shares an active space with us via `haex_space_members`
//!    joined on `haex_identities.did = ?`.
//!
//! Results are cached per resolver instance to avoid hammering SQLite on
//! every pre-auth accept during a DDoS. Callers invalidate the cache when
//! `haex_contacts`/`haex_identities`/`haex_space_members` change (the
//! `notify_contacts_changed` hook fires from the CRDT trigger path).

use std::collections::HashMap;
use std::sync::Mutex;

use serde_json::Value as JsonValue;

use crate::database::core::select_with_crdt;
use crate::database::DbConnection;

/// Cached classification per DID for the lifetime of a Leader session.
/// Only `Contact` results are persisted — see [`ContactResolver`] doc.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ContactClassification {
    Contact,
}

#[derive(Default)]
pub struct ContactResolver {
    /// Positive cache only: maps DID → `Contact` once the DB confirmed it.
    ///
    /// Caching `NonContact` indefinitely would let a transient DB error
    /// poison the cache for the whole leader session (a real contact stays
    /// silent-dropped until restart), and a deliberate attacker spamming
    /// random DIDs could pin negative entries forever and grow this map
    /// unbounded. The pickup brief calls for a CRDT-write-hook cache
    /// invalidation that is NOT wired in this PR — until that lands, the
    /// safer behaviour is to re-query on every miss. See CodeRabbit review
    /// on PR #562.
    cache: Mutex<HashMap<String, ContactClassification>>,
}

impl ContactResolver {
    pub fn new() -> Self {
        Self::default()
    }

    /// Returns whether `did` is a contact, consulting the in-memory cache
    /// first and falling back to a DB lookup. A DB error is treated as a
    /// `NonContact` answer (the call returns `false`) — under uncertainty
    /// the escalation prefers dropping connections to letting unknown peers
    /// through. The error is NOT cached so a transient failure does not
    /// poison this DID's classification for the rest of the session.
    pub fn is_contact(&self, db: &DbConnection, did: &str) -> bool {
        if self.cached_positive(did) {
            return true;
        }
        match lookup_db(db, did) {
            Ok(true) => {
                self.remember_contact(did);
                true
            }
            Ok(false) => false,
            Err(e) => {
                eprintln!("[DosDefence contacts] DB lookup failed for {did}: {e}");
                false
            }
        }
    }

    /// Invalidate the entire cache. Called from the CRDT-write hook for
    /// `haex_identities` or `haex_space_members` so the next lookup
    /// re-reads.
    pub fn invalidate_all(&self) {
        if let Ok(mut cache) = self.cache.lock() {
            cache.clear();
        }
    }

    /// Invalidate a specific DID. Cheaper than `invalidate_all` when the
    /// CRDT trigger can identify the changed row.
    pub fn invalidate(&self, did: &str) {
        if let Ok(mut cache) = self.cache.lock() {
            cache.remove(did);
        }
    }

    fn cached_positive(&self, did: &str) -> bool {
        self.cache
            .lock()
            .ok()
            .and_then(|c| c.get(did).copied())
            .is_some()
    }

    fn remember_contact(&self, did: &str) {
        if let Ok(mut cache) = self.cache.lock() {
            cache.insert(did.to_string(), ContactClassification::Contact);
        }
    }
}

/// Pure DB lookup, factored out so tests can drive it directly without the
/// cache layer. Returns `true` iff **either**:
///
/// 1. `haex_identities` has a row for `did` (the unified identities table
///    holds explicit contacts via `source = 'contact'`, but here we accept
///    any non-null row — own DIDs included — because an attacker
///    impersonating our own DID is also non-random and would not benefit
///    from being silent-dropped at L1), **OR**
/// 2. `did` shares an active space with **one of our own identities** —
///    i.e. there exists a `haex_space_members` row for `did` AND a
///    `haex_space_members` row for some local identity with the SAME
///    `space_id`. A stricter check than "any membership row mentioning the
///    DID" — otherwise a stale or imported membership row pointing at a
///    space we are not in could falsely classify a peer as a contact. See
///    CodeRabbit review on PR #562.
///
/// "Active space" is implicit: we treat any membership row as active. A
/// fully orthogonal "is space deleted" filter would require coordination
/// with the soft-delete log and isn't worth the round trip here — a stale
/// membership row pointing at a removed space is, at worst, a too-generous
/// contact check, which fails open in the safe direction (we accept the
/// connection; L4 still enforces post-auth ACL).
fn lookup_db(db: &DbConnection, did: &str) -> Result<bool, String> {
    let sql = "SELECT EXISTS (\
            SELECT 1 FROM haex_identities WHERE did = ?1 LIMIT 1\
        ) OR EXISTS (\
            SELECT 1 \
            FROM haex_space_members sm_remote \
            JOIN haex_identities i_remote ON i_remote.id = sm_remote.identity_id \
            JOIN haex_space_members sm_own ON sm_own.space_id = sm_remote.space_id \
            JOIN haex_identities i_own ON i_own.id = sm_own.identity_id \
            WHERE i_remote.did = ?1 \
              AND i_own.private_key IS NOT NULL \
            LIMIT 1\
        )"
    .to_string();
    let rows = select_with_crdt(sql, vec![JsonValue::String(did.to_string())], db)
        .map_err(|e| e.to_string())?;
    let Some(first) = rows.into_iter().next() else {
        return Ok(false);
    };
    let Some(val) = first.into_iter().next() else {
        return Ok(false);
    };
    Ok(val.as_i64().is_some_and(|n| n != 0))
}
