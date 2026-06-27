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
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ContactClassification {
    Contact,
    NonContact,
}

#[derive(Default)]
pub struct ContactResolver {
    cache: Mutex<HashMap<String, ContactClassification>>,
}

impl ContactResolver {
    pub fn new() -> Self {
        Self::default()
    }

    /// Returns whether `did` is a contact, consulting the in-memory cache
    /// first and falling back to a DB lookup. A DB error is treated as a
    /// `NonContact` answer — under uncertainty the escalation prefers
    /// dropping connections to letting unknown peers through. Callers that
    /// need a strict-fail-open policy should bypass this resolver
    /// entirely (e.g. when `EscalationPolicy::Off`).
    pub fn is_contact(&self, db: &DbConnection, did: &str) -> bool {
        if let Some(cached) = self.cached(did) {
            return cached == ContactClassification::Contact;
        }
        let classification = match lookup_db(db, did) {
            Ok(true) => ContactClassification::Contact,
            Ok(false) => ContactClassification::NonContact,
            Err(e) => {
                eprintln!("[DosDefence contacts] DB lookup failed for {did}: {e}");
                ContactClassification::NonContact
            }
        };
        self.remember(did, classification);
        classification == ContactClassification::Contact
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

    fn cached(&self, did: &str) -> Option<ContactClassification> {
        self.cache.lock().ok().and_then(|c| c.get(did).copied())
    }

    fn remember(&self, did: &str, classification: ContactClassification) {
        if let Ok(mut cache) = self.cache.lock() {
            cache.insert(did.to_string(), classification);
        }
    }
}

/// Pure DB lookup, factored out so tests can drive it directly without the
/// cache layer. Returns `true` iff `haex_identities` has a row for `did`,
/// **or** `did` appears as a `haex_space_members` row via a joined
/// `haex_identities` row.
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
            SELECT 1 FROM haex_space_members sm \
            JOIN haex_identities i ON i.id = sm.identity_id \
            WHERE i.did = ?1 LIMIT 1\
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
