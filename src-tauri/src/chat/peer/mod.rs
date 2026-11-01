//! Conversations with contacts and groups (spec 072), beside the agent conversations of
//! `chat::turn`. Both kinds share the one list of `chat_threads`; contact messages live in
//! `chat_peer_messages` (research R15) and travel only over Nostr-Relays and Blossom-Servers
//! (research R7, ADR 0024).
//!
//! Delivery slices (plan.md): this module grows from the list and storage (PR B) to Nostr delivery,
//! requests, files, groups and agent access in later slices.

pub mod commands;
pub mod ids;
pub mod store;
