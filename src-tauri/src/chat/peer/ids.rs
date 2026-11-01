//! Derived ids of contact and group conversations (spec 072, data-model.md, research R4). The synced
//! chat tables have no UNIQUE constraint (a conflict would halt the sync); where two devices may
//! create the same row (a message reaching both by Nostr, a direct conversation opened on both), its
//! id is a UUIDv5 over the natural key, so both write the same row and merge.
//!
//! Vault identities and device keys are passed as lowercase hex; [`direct_thread_id`] sorts its two
//! vaults, so both sides of a direct conversation derive the same id.

use uuid::Uuid;

/// Namespace of every derived chat id; the kind of row is part of the name.
/// Generated once; **never change** it, or old and new devices would derive different ids.
pub const CHAT_NS: Uuid = Uuid::from_u128(0x1310_b1ae_dcb7_4464_9046_12c0_c2a2_f290);

fn derive(kind: &str, parts: &[&str]) -> Uuid {
    let name = std::iter::once(kind)
        .chain(parts.iter().copied())
        .collect::<Vec<_>>()
        .join(":");
    Uuid::new_v5(&CHAT_NS, name.as_bytes())
}

/// `chat_threads.id` of the direct conversation between two vaults; the same on both sides.
pub fn direct_thread_id(vault_a: &str, vault_b: &str) -> Uuid {
    let (first, second) = if vault_a <= vault_b {
        (vault_a, vault_b)
    } else {
        (vault_b, vault_a)
    };
    derive("direct", &[first, second])
}

/// `chat_members.id`: a vault is a member of a conversation once.
pub fn member_id(thread_id: Uuid, vault: &str) -> Uuid {
    derive("member", &[&thread_id.to_string(), vault])
}

/// `chat_peer_messages.id`: one row per conversation, sender and event id of the rumor.
pub fn message_id(thread_id: Uuid, sender_vault: &str, event_id: &str) -> Uuid {
    derive("message", &[&thread_id.to_string(), sender_vault, event_id])
}

/// `chat_attachments.id`: the n-th attachment of a message.
pub fn attachment_id(message_id: Uuid, index: usize) -> Uuid {
    derive("attachment", &[&message_id.to_string(), &index.to_string()])
}

/// `chat_deliveries.id`: one delivery row per own message and recipient vault.
pub fn delivery_id(message_id: Uuid, recipient_vault: &str) -> Uuid {
    derive("delivery", &[&message_id.to_string(), recipient_vault])
}

#[cfg(test)]
#[path = "ids_tests.rs"]
mod tests;
