//! Single-source flood notifier: tracks which DIDs have already been
//! announced to the user so we emit the banner exactly once per
//! `(leader-session, DID)`. Without this, every reject above the
//! warn threshold would re-emit, drowning the user in identical
//! notifications during a flood.
//!
//! Lifetime = leader-lifetime. A leader restart resets the set, which
//! is intentional: the operator may have already addressed the
//! offending peer between restarts.

#![allow(clippy::expect_used)]

use std::collections::HashSet;
use std::sync::Mutex;

pub struct SingleSourceNotifier {
    notified: Mutex<HashSet<String>>,
}

impl Default for SingleSourceNotifier {
    fn default() -> Self {
        Self::new()
    }
}

impl SingleSourceNotifier {
    pub fn new() -> Self {
        Self {
            notified: Mutex::new(HashSet::new()),
        }
    }

    /// Returns `true` the first time it is called for `did`, marking the
    /// DID as notified. Returns `false` on every subsequent call for the
    /// same DID in this leader session.
    pub fn should_notify(&self, did: &str) -> bool {
        let mut notified = self
            .notified
            .lock()
            .expect("SingleSourceNotifier mutex poisoned");
        notified.insert(did.to_string())
    }
}
