//! Inviting a contact (spec 057 US1, research R5, R10; FR-009–FR-011, FR-017–FR-019): holzi dials
//! every device of the contact's stored card at once, proves itself ([`foreign_auth::dial`]) and
//! sends `Invite` on each connection. The first `Accepted` wins and gets `InviteConfirm`; every
//! other device gets `Withdraw`. The invited side is [`super::invited`].
//!
//! The inviting extension hears exactly one status per invitation (`peers:invite-status`): the one
//! who takes the invitation out of [`OutgoingInvites`] sends it, and a taken invitation cannot be
//! taken again. A withdrawal by the extension itself, or by [`PeerState::end_all`], sends none.
//!
//! No address is looked up anywhere but on the card: a device that is not reached by what the card
//! names is unreachable, and the extension is told the contact may need to share a current card
//! (research R5). Knocking (spec 056) comes later.
//!
//! [`PeerState::end_all`]: super::PeerState::end_all

use std::collections::HashMap;
use std::sync::Arc;

use iroh::EndpointAddr;
use serde_json::{json, Value};
use tokio_util::sync::CancellationToken;
use uuid::Uuid;

use super::channel::Channel;
use super::invite_run::{self, Run};
use super::limits::{OUTGOING_INVITES_PER_EXTENSION, PAYLOAD_BYTES};
use super::{contact_in_scope, ExtensionRef, PeerError, PeerState};
use crate::contacts::card::received;
use crate::sync::presence::{endpoint_addr_of, now_ms};

/// The end of an invitation, for the frames of the inviting extension.
pub const INVITE_STATUS: &str = "peers:invite-status";
/// An accepted invitation became a channel, for the frames of the invited extension.
pub const CHANNEL_OPENED: &str = "peers:channel-opened";
/// The hint that goes with `unreachable`.
pub const SHARE_CURRENT_CARD: &str = "share_current_card";

/// What one device of the card did.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Device {
    Dialing,
    Waiting,
    Unreachable,
    Declined,
    NotInstalled,
}

/// How an invitation ended for the extension that sent it (FR-018).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Final {
    Declined,
    NotInstalled,
    Unreachable,
    Expired,
}

impl Final {
    pub(super) fn event(self, invite: Uuid) -> Value {
        let (status, hint) = match self {
            Self::Declined => ("declined", None),
            Self::NotInstalled => ("not-installed", None),
            Self::Unreachable => ("unreachable", Some(SHARE_CURRENT_CARD)),
            Self::Expired => ("expired", None),
        };
        let mut event = json!({ "inviteId": invite.to_string(), "status": status });
        if let Some(hint) = hint {
            event["hint"] = Value::from(hint);
        }
        event
    }
}

/// An open invitation of this device (data-model.md `OutgoingInvite`).
pub(super) struct Outgoing {
    extension_id: Uuid,
    devices: Vec<Device>,
    /// Ends every task of the invitation: the devices that wait send `Withdraw`.
    pub(super) stop: CancellationToken,
}

impl Outgoing {
    /// The end of the invitation once no device dials or waits any more: `declined` and
    /// `not-installed` only when every device reached said so, `unreachable` when none was
    /// reached (FR-018).
    fn verdict(&self) -> Option<Final> {
        if self
            .devices
            .iter()
            .any(|d| matches!(d, Device::Dialing | Device::Waiting))
        {
            return None;
        }
        let reached: Vec<Device> = self
            .devices
            .iter()
            .copied()
            .filter(|d| *d != Device::Unreachable)
            .collect();
        Some(if reached.is_empty() {
            Final::Unreachable
        } else if reached.iter().all(|d| *d == Device::NotInstalled) {
            Final::NotInstalled
        } else {
            Final::Declined
        })
    }
}

/// The open invitations of this device.
#[derive(Default)]
pub(super) struct OutgoingInvites(HashMap<Uuid, Outgoing>);

impl OutgoingInvites {
    fn add(&mut self, id: Uuid, outgoing: Outgoing) -> Result<(), PeerError> {
        let own = self
            .0
            .values()
            .filter(|o| o.extension_id == outgoing.extension_id)
            .count();
        if own >= OUTGOING_INVITES_PER_EXTENSION {
            return Err(PeerError::Limit {
                limit: OUTGOING_INVITES_PER_EXTENSION,
            });
        }
        self.0.insert(id, outgoing);
        Ok(())
    }

    /// Takes the invitation out: whoever gets it ends it.
    pub(super) fn take(&mut self, id: Uuid) -> Option<Outgoing> {
        self.0.remove(&id)
    }

    fn take_own(&mut self, extension_id: Uuid, id: Uuid) -> Option<Outgoing> {
        match self.0.get(&id) {
            Some(o) if o.extension_id == extension_id => self.0.remove(&id),
            _ => None,
        }
    }

    /// Notes what a device did; the invitation leaves when that settles it.
    pub(super) fn mark(
        &mut self,
        id: Uuid,
        index: usize,
        device: Device,
    ) -> Option<(Outgoing, Final)> {
        let outgoing = self.0.get_mut(&id)?;
        *outgoing.devices.get_mut(index)? = device;
        let ended = outgoing.verdict()?;
        self.0.remove(&id).map(|outgoing| (outgoing, ended))
    }

    pub(super) fn take_all(&mut self, extension_id: Uuid) -> Vec<CancellationToken> {
        let ids: Vec<Uuid> = self
            .0
            .iter()
            .filter(|(_, o)| o.extension_id == extension_id)
            .map(|(id, _)| *id)
            .collect();
        ids.into_iter()
            .filter_map(|id| self.0.remove(&id))
            .map(|o| o.stop)
            .collect()
    }

    pub(super) fn clear(&mut self) -> Vec<CancellationToken> {
        self.0.drain().map(|(_, o)| o.stop).collect()
    }

    pub(super) fn extensions(&self) -> impl Iterator<Item = Uuid> + '_ {
        self.0.values().map(|o| o.extension_id)
    }
}

/// Invites `contact_id` for `extension_id` (`extension_peers_invite`): the invitation's id, at
/// once. The status follows as `peers:invite-status`, which can reach the frames before the
/// caller has the id.
///
/// `contact_unknown` for a contact the extension's `contacts` permission does not cover, one that
/// does not exist and one without a card alike (051 FR-031).
pub async fn invite(
    peers: &PeerState,
    extension_id: Uuid,
    extension: ExtensionRef,
    contact_id: &str,
    payload: &Value,
) -> Result<Uuid, PeerError> {
    let payload = serde_json::to_vec(payload).map_err(|_| PeerError::InvalidInput)?;
    if payload.len() > PAYLOAD_BYTES {
        return Err(PeerError::TooLarge {
            limit: PAYLOAD_BYTES,
        });
    }
    let session = peers.session().ok_or(PeerError::NetworkUnavailable)?;
    let host = session
        .ctx
        .frames
        .host()
        .ok_or(PeerError::NetworkUnavailable)?;
    if !contact_in_scope(&session.ctx, &host, extension_id, contact_id).await {
        return Err(PeerError::ContactUnknown);
    }
    let contact = contact_id.to_owned();
    let card = session
        .ctx
        .db
        .read(move |q| received::stored_of_contact(q, &contact).map_err(Into::into))
        .await
        .map_err(|error| {
            log::warn!("peers: the card of a contact could not be read: {error}");
            PeerError::ContactUnknown
        })?
        .ok_or(PeerError::ContactUnknown)?;
    if peers.channels_full(extension_id) {
        return Err(PeerError::Limit {
            limit: super::limits::CHANNELS_PER_EXTENSION,
        });
    }
    let addrs: Vec<EndpointAddr> = card
        .card
        .devices
        .iter()
        .filter_map(|d| endpoint_addr_of(&d.endpoint_id, d.iroh_relay.as_deref(), &[]).ok())
        .collect();
    let lifetime = peers.timing().invite_lifetime;
    let id = Uuid::new_v4();
    let stop = CancellationToken::new();
    peers.outgoing().add(
        id,
        Outgoing {
            extension_id,
            devices: vec![Device::Dialing; addrs.len()],
            stop: stop.clone(),
        },
    )?;
    let run = Run {
        ctx: Arc::clone(&session.ctx),
        tracker: session.node.tasks().tracker,
        id,
        extension_id,
        extension,
        contact_id: contact_id.to_owned(),
        payload,
        expires_at: now_ms().saturating_add(u64::try_from(lifetime.as_millis()).unwrap_or(0)),
        stop,
    };
    invite_run::start(run, &session.node, addrs, lifetime);
    Ok(id)
}

/// Withdraws an open invitation of `extension_id` from every device that has it
/// (`extension_peers_invite_cancel`). No status follows. One that ended already, or that belongs
/// to another extension, is `invite_not_open`.
pub fn cancel(peers: &PeerState, extension_id: Uuid, invite: Uuid) -> Result<(), PeerError> {
    let taken = peers.outgoing().take_own(extension_id, invite);
    taken
        .map(|outgoing| outgoing.stop.cancel())
        .ok_or(PeerError::InviteNotOpen)
}

/// The event an invited extension hears when its invitation became a channel (contracts/bridge.md).
pub(super) fn channel_opened(
    channel: &Channel,
    invite: Uuid,
    contact_id: Option<String>,
    name: &str,
    payload: Value,
) -> Value {
    let mut from = json!({ "name": name });
    if let Some(contact_id) = contact_id {
        from["contactId"] = Value::String(contact_id);
    }
    json!({
        "channelId": channel.id.to_string(),
        "origin": "invitation",
        "inviteId": invite.to_string(),
        "from": from,
        "payload": payload,
    })
}

#[cfg(test)]
#[path = "invite_tests.rs"]
mod tests;
