//! Device sessions of the chat with contacts (spec 095, ADR 0026): one Double Ratchet session
//! (Olm, `vodozemac`) per pair of devices inside the gift wraps of spec 072. A device key exposed
//! later then opens the archived wraps and seals but no message content (forward secrecy), and a
//! copied key state stops decrypting once both devices contributed fresh ratchet keys.
//!
//! This module owns the ratchet account of this installation, its sessions with peer devices, what
//! it remembers about those devices and the chat key records it accepted (data-model.md). It reads
//! and writes only the rows of this installation (`installation_uuid`); rows of another
//! installation in a copied vault file are never read, changed or deleted (research R9). No secret
//! (pickle, message key, plaintext) reaches a log line or an error message.

use nostr::event::Kind;

pub mod account;
pub mod session;
pub mod store;
pub mod wire;

const DAY_MS: i64 = 24 * 60 * 60 * 1000;
const HOUR_MS: i64 = 60 * 60 * 1000;

/// Rumor kind of a ratchet message (`chat.ratchet`).
pub const RATCHET_KIND: Kind = Kind::Custom(24122);
/// Rumor kind of a request to open a new session (`chat.reset`).
pub const RESET_KIND: Kind = Kind::Custom(24123);
/// Event kind of a chat key record.
pub const KEY_RECORD_KIND: Kind = Kind::Custom(10120);
/// The largest plaintext of a ratchet message: an opening message with it still wraps within
/// [`super::wrap::MAX_WRAP_BYTES`] (measured: 21,215 bytes, research "Checks done for tasks").
pub const MAX_PLAINTEXT_BYTES: usize = 20 * 1024;
/// A new start key after this long.
pub const START_KEY_RENEW_MS: i64 = 30 * DAY_MS;
/// A session this device sends on gets an event at least this often while the vault is open.
pub const KEEP_ALIVE_MS: i64 = DAY_MS;
/// A renewal older than this is shown as outstanding.
pub const RENEWAL_OVERDUE_MS: i64 = 7 * DAY_MS;
/// A reset is sent to, and accepted from, one device at most this often.
pub const RESET_EVERY_MS: i64 = HOUR_MS;
/// A chat event without a delivery receipt is sent again after these intervals; the last repeats.
pub const RESEND_AFTER_MS: [i64; 3] = [DAY_MS, 3 * DAY_MS, 7 * DAY_MS];
/// A message stored longer ago than this that arrives again is a resend and is acknowledged again.
pub const REACK_AFTER_MS: i64 = 12 * HOUR_MS;
/// Sessions kept per peer device.
pub const MAX_SESSIONS: usize = 4;

#[cfg(test)]
#[path = "erasure_tests.rs"]
mod erasure_tests;
