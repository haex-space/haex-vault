//! Conversations with contacts and groups (spec 072), beside the agent conversations of
//! `chat::turn`. Both kinds share the one list of `chat_threads`; contact messages live in
//! `chat_peer_messages` (research R15) and travel only over Nostr-Relays and Blossom-Servers
//! (research R7, ADR 0024).
//!
//! Delivery slices (plan.md): this module grows from the list and storage (PR B) to Nostr delivery,
//! requests, files, groups and agent access in later slices.

pub mod commands;
pub mod delivery;
pub mod envelope;
pub mod ids;
pub mod messages;
pub mod notify;
pub mod outbox;
pub mod receive;
pub mod send;
pub mod service;
pub mod session;
pub mod settings;
pub mod store;
pub mod targets;
pub mod verify;
pub mod wrap;

#[cfg(test)]
pub(crate) mod test_support;

#[cfg(test)]
#[path = "two_vaults_tests.rs"]
mod two_vaults_tests;

#[cfg(test)]
#[path = "own_devices_tests.rs"]
mod own_devices_tests;

#[cfg(test)]
#[path = "soak_tests.rs"]
mod soak_tests;
