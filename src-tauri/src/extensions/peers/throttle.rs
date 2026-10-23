//! Throttling the admission on `haex-vault-ext-peer/1` (research R7, FR-033): trying codes and
//! overloading the device fail, valid joins of contacts are not locked out.
//!
//! Every new connection needs a handshake slot (a bounded number run at once) and stays within
//! [`ATTEMPTS_PER_ENDPOINT`] per [`WINDOW`] for its endpoint id. The exchange tells verified from
//! not verified; only a device that is not verified then needs a pending slot ([`PENDING`] at
//! once) and a place among the [`UNVERIFIED_PER_WINDOW`] new ones of the window, so contacts and
//! own devices have their own capacity. [`FAILED_JOINS`] joins answered `Closed` ban the endpoint
//! for [`BAN`], during which every join of it is answered `Closed` too, whatever it proves.
//!
//! The clock is the caller's: every method takes `now`, so tests run without waiting.
//!
//! ponytail: fixed windows per endpoint id in a map cleaned on access, at most [`MAX_ENDPOINTS`]
//! entries with the least recently seen falling out, bans last (an O(n) scan when full), and one
//! bound on handshakes for everyone. Endpoint ids are free to make, so a flood of fresh ids
//! resets the per-endpoint counts of the ids it pushes out and can fill the handshake slots for
//! a while, delaying contacts too. Upgrade: a token bucket per network address prefix, before
//! the handshake.

use std::collections::HashMap;
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::time::{Duration, Instant};

/// The fixed window of the counts below.
pub const WINDOW: Duration = Duration::from_secs(10);
/// Connections per endpoint id and window, verified or not.
pub const ATTEMPTS_PER_ENDPOINT: u32 = 5;
/// Joins answered `Closed` before the endpoint id is banned.
pub const FAILED_JOINS: u32 = 3;
/// How long a ban lasts.
pub const BAN: Duration = Duration::from_secs(60);
/// New connections of devices that are not verified, per window.
pub const UNVERIFIED_PER_WINDOW: u32 = 20;
/// Devices that are not verified and not yet admitted, at once.
pub const PENDING: usize = 16;
/// Exchanges running at once, before anyone is known.
pub const HANDSHAKES: usize = 32;
/// Endpoint ids remembered at most.
pub const MAX_ENDPOINTS: usize = 1024;

/// The counters of one endpoint id.
struct Endpoint {
    window_start: Instant,
    attempts: u32,
    /// Joins answered `Closed` since the last ban.
    failed_joins: u32,
    banned_until: Option<Instant>,
    last_seen: Instant,
}

impl Endpoint {
    fn new(now: Instant) -> Self {
        Self {
            window_start: now,
            attempts: 0,
            failed_joins: 0,
            banned_until: None,
            last_seen: now,
        }
    }

    fn banned(&self, now: Instant) -> bool {
        self.banned_until.is_some_and(|until| now < until)
    }

    /// Nothing about it matters any more: no ban, and quiet for longer than a ban lasts.
    fn stale(&self, now: Instant) -> bool {
        !self.banned(now) && now.saturating_duration_since(self.last_seen) >= BAN
    }
}

#[derive(Default)]
struct Counters {
    endpoints: HashMap<[u8; 32], Endpoint>,
    handshakes: usize,
    pending: usize,
    unverified_window: Option<(Instant, u32)>,
}

impl Counters {
    /// The record of `endpoint`, made room for when the map is full.
    fn endpoint(&mut self, endpoint: [u8; 32], now: Instant) -> &mut Endpoint {
        if !self.endpoints.contains_key(&endpoint) && self.endpoints.len() >= MAX_ENDPOINTS {
            self.endpoints.retain(|_, record| !record.stale(now));
            if self.endpoints.len() >= MAX_ENDPOINTS {
                // Bans go last.
                let oldest = self
                    .endpoints
                    .iter()
                    .min_by_key(|(_, record)| (record.banned(now), record.last_seen))
                    .map(|(id, _)| *id);
                if let Some(oldest) = oldest {
                    self.endpoints.remove(&oldest);
                }
            }
        }
        let record = self
            .endpoints
            .entry(endpoint)
            .or_insert_with(|| Endpoint::new(now));
        record.last_seen = now;
        record
    }
}

/// The throttle of one endpoint, shared by all its connections.
#[derive(Clone, Default)]
pub struct Throttle(Arc<Mutex<Counters>>);

impl Throttle {
    fn counters(&self) -> MutexGuard<'_, Counters> {
        self.0.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// A new connection from `endpoint`: its handshake slot, or `None` when the endpoint tried
    /// too often in this window or too many exchanges run.
    pub fn handshake(&self, endpoint: [u8; 32], now: Instant) -> Option<Slot> {
        let mut counters = self.counters();
        if counters.handshakes >= HANDSHAKES {
            return None;
        }
        let record = counters.endpoint(endpoint, now);
        if now.saturating_duration_since(record.window_start) >= WINDOW {
            record.window_start = now;
            record.attempts = 0;
        }
        if record.attempts >= ATTEMPTS_PER_ENDPOINT {
            return None;
        }
        record.attempts += 1;
        counters.handshakes += 1;
        Some(self.slot(Kind::Handshake))
    }

    /// A device that is not verified waits for admission: its pending slot, or `None` when
    /// [`PENDING`] wait already or [`UNVERIFIED_PER_WINDOW`] came in this window.
    pub fn unverified(&self, now: Instant) -> Option<Slot> {
        let mut counters = self.counters();
        if counters.pending >= PENDING {
            return None;
        }
        let window = match counters.unverified_window {
            Some((start, count)) if now.saturating_duration_since(start) < WINDOW => (start, count),
            _ => (now, 0),
        };
        if window.1 >= UNVERIFIED_PER_WINDOW {
            return None;
        }
        counters.unverified_window = Some((window.0, window.1 + 1));
        counters.pending += 1;
        Some(self.slot(Kind::Pending))
    }

    /// A join of `endpoint` was answered `Closed`; the [`FAILED_JOINS`]th bans it for [`BAN`].
    pub fn join_failed(&self, endpoint: [u8; 32], now: Instant) {
        let mut counters = self.counters();
        let record = counters.endpoint(endpoint, now);
        record.failed_joins += 1;
        if record.failed_joins >= FAILED_JOINS {
            record.failed_joins = 0;
            record.banned_until = Some(now + BAN);
        }
    }

    /// Whether `endpoint` is banned at `now`.
    pub fn banned(&self, endpoint: [u8; 32], now: Instant) -> bool {
        self.counters()
            .endpoints
            .get(&endpoint)
            .is_some_and(|record| record.banned(now))
    }

    /// Endpoint ids remembered now.
    #[cfg(test)]
    pub(crate) fn endpoints(&self) -> usize {
        self.counters().endpoints.len()
    }

    fn slot(&self, kind: Kind) -> Slot {
        Slot {
            counters: Arc::clone(&self.0),
            kind,
        }
    }
}

#[derive(Debug, Clone, Copy)]
enum Kind {
    Handshake,
    Pending,
}

/// A held place of the throttle; dropping it gives the place back, on every way out.
pub struct Slot {
    counters: Arc<Mutex<Counters>>,
    kind: Kind,
}

impl Drop for Slot {
    fn drop(&mut self) {
        let mut counters = self.counters.lock().unwrap_or_else(PoisonError::into_inner);
        match self.kind {
            Kind::Handshake => counters.handshakes = counters.handshakes.saturating_sub(1),
            Kind::Pending => counters.pending = counters.pending.saturating_sub(1),
        }
    }
}

#[cfg(test)]
#[path = "throttle_tests.rs"]
mod tests;
