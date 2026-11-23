//! The protection state of a conversation (spec 095 US4, FR-013, data-model.md "Protection
//! state"): derived from this installation's ratchet rows with the devices of the conversation's
//! vaults, never stored.
//!
//! Per device, from strongest to weakest:
//! - `protected`: a session exists and its last renewal (`renewed_at`, else `created_at`) is at
//!   most [`RENEWAL_OVERDUE_MS`] ago;
//! - `renewalOutstanding`: the last renewal is older;
//! - `recovering`: a reset with the device was sent or received and the new session has not
//!   carried an event yet (`recovering_since`);
//! - `waitingForKeys`: no session, and an event of the conversation still waits for the device's
//!   vault in this device's outbox. Nothing goes to the device until its record arrives.
//!
//! A device without a session that nothing waits for has had nothing sent to it, so it does not
//! weaken the state. The conversation shows the weakest state over its devices: the devices on the
//! current cards of its other vaults and the own other devices, which get a copy of every message.
//! Notes and conversations with a model have none.

use serde::Serialize;
use ts_rs::TS;
use uuid::Uuid;

use super::{store, RENEWAL_OVERDUE_MS};
use crate::chat::peer::session::Me;
use crate::chat::peer::store::{kind_of, ConversationKind};
use crate::chat::peer::targets;
use crate::error::{HaexError, Result};
use crate::storage::query::Query;
use crate::sync::keys::hex;

/// A protection state, strongest first; a later one is weaker.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, TS)]
#[ts(
    export,
    export_to = "../../src/types/bindings/",
    rename = "ChatPeerProtectionState"
)]
#[serde(rename_all = "camelCase")]
pub enum ProtectionState {
    Protected,
    RenewalOutstanding,
    Recovering,
    WaitingForKeys,
}

/// What the conversation view shows of a conversation's protection.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, TS)]
#[ts(
    export,
    export_to = "../../src/types/bindings/",
    rename = "ChatPeerProtection"
)]
#[serde(rename_all = "camelCase")]
pub struct Protection {
    /// The weakest state over the conversation's devices.
    pub state: ProtectionState,
    /// The oldest renewal of the session used to send on, across devices; `None` without one.
    #[ts(type = "number | null")]
    pub renewed_at: Option<i64>,
}

/// The protection of conversation `thread_id` at `now`; `None` for notes, conversations with a
/// model and an unknown id.
pub fn of(q: &mut impl Query, me: &Me, thread_id: Uuid, now: i64) -> Result<Option<Protection>> {
    match kind_of(q, thread_id)? {
        Some(ConversationKind::Direct | ConversationKind::Group) => {}
        _ => return Ok(None),
    }
    let mut vaults: Vec<String> = q.query_map(
        "SELECT vault FROM chat_members WHERE thread_id = ?1 AND vault <> ?2",
        &[&thread_id.to_string(), &me.vault_hex],
        |row| row.get(0),
    )?;
    vaults.push(me.vault_hex.clone());
    let waiting = waiting_vaults(q, thread_id)?;
    let mut state = ProtectionState::Protected;
    let mut renewed_at: Option<i64> = None;
    for vault in &vaults {
        for device in targets::of(q, me, vault)?.devices {
            let device = hex(&device);
            let sessions = store::sessions(q, me.installation, &device)?;
            // The smallest session id is the one `session::encrypt` sends on. A newer session
            // must not hide that sending session's overdue renewal.
            let last_renewal = sessions
                .first()
                .map(|row| row.renewed_at.unwrap_or(row.created_at));
            let of_device = if store::peer(q, me.installation, &device)?
                .recovering_since
                .is_some()
            {
                ProtectionState::Recovering
            } else if let Some(last_renewal) = last_renewal {
                if now - last_renewal > RENEWAL_OVERDUE_MS {
                    ProtectionState::RenewalOutstanding
                } else {
                    ProtectionState::Protected
                }
            } else if waiting.contains(vault) {
                ProtectionState::WaitingForKeys
            } else {
                ProtectionState::Protected
            };
            state = state.max(of_device);
            if let Some(last_renewal) = last_renewal {
                renewed_at = Some(renewed_at.map_or(last_renewal, |at| at.min(last_renewal)));
            }
        }
    }
    Ok(Some(Protection { state, renewed_at }))
}

/// The vaults an event of `thread_id` still waits for in this device's outbox.
fn waiting_vaults(q: &mut impl Query, thread_id: Uuid) -> Result<Vec<String>> {
    let rows: Vec<(String, String)> = q.query_map(
        "SELECT pending, awaiting FROM chat_outbox_no_sync WHERE thread_id = ?1",
        &[&thread_id.to_string()],
        |row| Ok((row.get(0)?, row.get(1)?)),
    )?;
    let mut vaults = Vec::new();
    for (pending, awaiting) in rows {
        let pending: Vec<String> =
            serde_json::from_str(&pending).map_err(|e| HaexError::InvalidInput {
                reason: format!("outbox pending: {e}"),
            })?;
        let awaiting: Vec<String> =
            serde_json::from_str(&awaiting).map_err(|e| HaexError::InvalidInput {
                reason: format!("outbox awaiting: {e}"),
            })?;
        for vault in pending.into_iter().chain(awaiting) {
            if !vaults.contains(&vault) {
                vaults.push(vault);
            }
        }
    }
    Ok(vaults)
}

#[cfg(test)]
#[path = "protection_tests.rs"]
mod tests;
