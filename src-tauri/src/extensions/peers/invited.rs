//! The invited side (spec 057 US1, research R10; FR-013–FR-017): a device of a contact sent
//! `Invite`. The prompt of [`super::prompt`] asks the user, and only the user's acceptance there
//! lets the connection become a channel, after the inviting side confirms it (`InviteConfirm`).
//! Until then no extension of this device hears of the invitation (FR-016, SC-006).
//!
//! The connection stays open while the user is asked, up to [`INVITE_LIFETIME`]. The prompt goes
//! when the inviting side withdraws, the connection ends, the time runs out or the extension
//! stops; an answer that comes after is told the invitation is gone.
//!
//! [`INVITE_LIFETIME`]: super::limits::INVITE_LIFETIME

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use iroh::endpoint::{Connection, RecvStream, SendStream};
use serde_json::Value;
use tokio::sync::oneshot;
use tokio::time::timeout;
use tokio_util::sync::CancellationToken;
use tokio_util::task::TaskTracker;
use uuid::Uuid;

use super::admission::{refuse, PeerContext};
use super::channel::{self, Channel, Opening, Origin};
use super::invite::channel_opened;
use super::limits::{INCOMING_PER_VAULT, INCOMING_TOTAL, PAYLOAD_BYTES};
use super::lobby::write_reply;
use super::prompt::{self, Answer, ExtensionPeerInvitationGoneReason, PromptInfo};
use super::wire::{self, InviteStatus, Message, ADMISSION_FRAME_LIMIT, VERSION};
use super::{contact_in_scope, ExtensionRef, PeerState, FEATURE};
use crate::contacts::service::{Caller, ContactsService};
use crate::extensions::host::ExtensionHost;
use crate::extensions::permissions::store::candidates;
use crate::extensions::permissions::{
    evaluate, Action, Decision, Permission, PermissionKind, PermissionRequest, RequestTarget,
};
use crate::storage::query::Query;
use crate::sync::foreign_auth::{Trust, Verified};

/// How long a side that answered has to hear the other close before the connection closes.
const REPLY_GRACE: Duration = Duration::from_secs(2);

/// What the user's answer found.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Answered {
    /// The answer was taken; the invitation goes on from here.
    Open,
    /// The invitation is no longer open: withdrawn, expired, taken by another device or answered.
    Gone,
}

/// An invitation this device asks about (data-model.md `IncomingInvite`).
struct Incoming {
    extension_id: Uuid,
    remote: Verified,
    /// The user's answer, once.
    answer: Option<oneshot::Sender<bool>>,
    /// Ends the invitation here: its extension stopped or its inviter lost the ground.
    end: CancellationToken,
}

/// The invitations this device asks about, by the inviter's id.
#[derive(Default)]
pub(super) struct Invitations(HashMap<Uuid, Incoming>);

impl Invitations {
    /// Takes `invite` in unless it is known already or a limit holds: four from one vault, sixteen
    /// in all (research R10).
    fn register(&mut self, invite: Uuid, incoming: Incoming) -> bool {
        let from_vault = self
            .0
            .values()
            .filter(|i| i.remote.vault == incoming.remote.vault)
            .count();
        if self.0.contains_key(&invite)
            || from_vault >= INCOMING_PER_VAULT
            || self.0.len() >= INCOMING_TOTAL
        {
            return false;
        }
        self.0.insert(invite, incoming);
        true
    }

    pub(super) fn answer(&mut self, invite: Uuid, accept: bool) -> Answered {
        let sent = self
            .0
            .get_mut(&invite)
            .and_then(|incoming| incoming.answer.take())
            .is_some_and(|answer| answer.send(accept).is_ok());
        if sent {
            Answered::Open
        } else {
            Answered::Gone
        }
    }

    fn remove(&mut self, invite: Uuid) {
        self.0.remove(&invite);
    }

    /// Ends the invitations `ended` picks: their tasks stop and the prompts go.
    fn end(&mut self, ended: impl Fn(&Uuid, &Incoming) -> bool) {
        self.0.retain(|id, incoming| {
            let keep = !ended(id, incoming);
            if !keep {
                incoming.end.cancel();
            }
            keep
        });
    }

    pub(super) fn end_all(&mut self, extension_id: Uuid) {
        self.end(|_, incoming| incoming.extension_id == extension_id);
    }

    /// Ends the invitation `invite`, whose inviter lost the ground.
    pub(super) fn end_one(&mut self, invite: Uuid) {
        self.end(|id, _| *id == invite);
    }

    pub(super) fn clear(&mut self) {
        self.end(|_, _| true);
    }

    pub(super) fn remotes(&self) -> Vec<(Uuid, Verified)> {
        self.0
            .iter()
            .map(|(id, incoming)| (*id, incoming.remote.clone()))
            .collect()
    }

    pub(super) fn extensions(&self) -> impl Iterator<Item = Uuid> + '_ {
        self.0.values().map(|incoming| incoming.extension_id)
    }
}

/// Takes the invitation out of the list however its task ends.
struct Registration<'a> {
    peers: &'a PeerState,
    invite: Uuid,
}

impl Drop for Registration<'_> {
    fn drop(&mut self) {
        self.peers.invitations().remove(self.invite);
    }
}

/// An `Invite` as it came over the wire.
pub(super) struct InviteRequest {
    pub invite: [u8; 16],
    pub extension: ExtensionRef,
    pub payload: Vec<u8>,
}

/// Everything the invited side needs besides the request.
pub(super) struct Inviting<'a> {
    pub ctx: &'a PeerContext,
    pub host: &'a Arc<ExtensionHost>,
    pub tracker: &'a TaskTracker,
    pub connection: Connection,
    pub remote: Verified,
    pub send: SendStream,
    pub recv: RecvStream,
}

/// The extension an invitation names, as this device has it.
struct Known {
    id: Uuid,
    name: String,
}

/// How the exchange with the inviting side ended.
enum Ended {
    Confirmed,
    Declined,
    Gone(ExtensionPeerInvitationGoneReason),
}

/// Asks the user about an invitation of a contact and, once the user accepted and the inviting
/// side confirmed, makes the connection a channel. An invitation of a device that is no contact
/// is dropped without a word; one for an extension this device does not run or may not use is
/// answered `NotInstalled`.
pub(super) async fn admit_invite(inviting: Inviting<'_>, request: InviteRequest) {
    let Inviting {
        ctx,
        host,
        tracker,
        connection,
        remote,
        mut send,
        mut recv,
    } = inviting;
    let Trust::Contact {
        contact_id,
        card_name,
    } = remote.trust.clone()
    else {
        return refuse(&connection);
    };
    let invite = Uuid::from_bytes(request.invite);
    let Some(payload) = parse_payload(&request.payload) else {
        return refuse(&connection);
    };
    let known = match lookup(ctx, host, &request.extension).await {
        Ok(Some(known)) => known,
        Ok(None) => {
            return reply(
                &mut send,
                &connection,
                request.invite,
                InviteStatus::NotInstalled,
            )
            .await;
        }
        Err(error) => {
            log::warn!("peers: an invitation could not be checked: {error}");
            return refuse(&connection);
        }
    };
    if host.peers.channels_full(known.id) {
        return refuse(&connection);
    }
    let contact_name = contact_name(ctx, &contact_id)
        .await
        .unwrap_or_else(|| card_name.clone());
    let info = Arc::new(PromptInfo {
        invite,
        extension_id: known.id,
        extension_name: known.name,
        contact_name,
    });
    let (answer, answered) = oneshot::channel();
    let end = CancellationToken::new();
    let incoming = Incoming {
        extension_id: known.id,
        remote: remote.clone(),
        answer: Some(answer),
        end: end.clone(),
    };
    if !host.peers.invitations().register(invite, incoming) {
        return refuse(&connection);
    }
    let _registration = Registration {
        peers: &host.peers,
        invite,
    };
    let weak = Arc::downgrade(host);
    let on_answer: Answer = Arc::new(move |accept| {
        if let Some(host) = weak.upgrade() {
            host.peers.answer_invitation(invite, accept);
        }
    });
    let mut prompt = prompt::present(host, &ctx.frames, Arc::clone(&info), on_answer).await;
    let lifetime = host.peers.timing().invite_lifetime;
    match converse(&mut send, &mut recv, invite, answered, &end, lifetime).await {
        Ended::Gone(reason) => {
            prompt.gone(reason);
            refuse(&connection);
        }
        Ended::Declined => {
            drop(prompt);
            let _ = send.finish();
            let _ = timeout(REPLY_GRACE, connection.closed()).await;
            refuse(&connection);
        }
        Ended::Confirmed => {
            drop(prompt);
            let contact = contact_in_scope(ctx, host, known.id, &contact_id)
                .await
                .then_some(contact_id);
            let opening = Opening {
                extension_id: known.id,
                remote,
                origin: Origin::Invitation { invite },
                connection,
                send,
                recv,
            };
            let announce = |channel: &Channel| {
                let event = channel_opened(channel, invite, contact, &card_name, payload);
                host.peers.opened(channel, &ctx.frames, event, tracker);
            };
            if let Err(error) = channel::open_announced(opening, &ctx.frames, tracker, announce) {
                log::debug!("peers: an accepted invitation did not become a channel: {error}");
            }
        }
    }
}

/// Waits for the user's answer, answers the inviting side and waits for its confirmation. The
/// inviting side's `Withdraw`, the end of the connection, the end of `lifetime` and `end` all
/// end it as gone.
async fn converse(
    send: &mut SendStream,
    recv: &mut RecvStream,
    invite: Uuid,
    mut answered: oneshot::Receiver<bool>,
    end: &CancellationToken,
    lifetime: Duration,
) -> Ended {
    let deadline = tokio::time::sleep(lifetime);
    tokio::pin!(deadline);
    let read = wire::read(recv, ADMISSION_FRAME_LIMIT);
    tokio::pin!(read);
    let mut accepted = false;
    loop {
        tokio::select! {
            biased;
            () = end.cancelled() => return Ended::Gone(ExtensionPeerInvitationGoneReason::Withdrawn),
            () = &mut deadline => return Ended::Gone(ExtensionPeerInvitationGoneReason::Expired),
            message = &mut read => {
                return match message {
                    Ok(Some(Message::InviteConfirm { invite: confirmed, .. }))
                        if accepted && confirmed == *invite.as_bytes() => Ended::Confirmed,
                    _ => Ended::Gone(ExtensionPeerInvitationGoneReason::Withdrawn),
                };
            }
            choice = &mut answered, if !accepted => match choice {
                Ok(accept) => {
                    let status = if accept { InviteStatus::Accepted } else { InviteStatus::Declined };
                    let message = Message::InviteReply { v: VERSION, invite: *invite.as_bytes(), status };
                    if !write_reply(send, &message).await {
                        return Ended::Gone(ExtensionPeerInvitationGoneReason::Withdrawn);
                    }
                    if !accept {
                        return Ended::Declined;
                    }
                    accepted = true;
                }
                Err(_) => return Ended::Gone(ExtensionPeerInvitationGoneReason::Withdrawn),
            },
        }
    }
}

/// Answers `status` without asking anyone and closes.
async fn reply(
    send: &mut SendStream,
    connection: &Connection,
    invite: [u8; 16],
    status: InviteStatus,
) {
    let message = Message::InviteReply {
        v: VERSION,
        invite,
        status,
    };
    if write_reply(send, &message).await {
        let _ = send.finish();
        let _ = timeout(REPLY_GRACE, connection.closed()).await;
    }
    refuse(connection);
}

/// The JSON of an invitation's payload, at most [`PAYLOAD_BYTES`].
fn parse_payload(payload: &[u8]) -> Option<Value> {
    if payload.len() > PAYLOAD_BYTES {
        return None;
    }
    serde_json::from_slice(payload).ok()
}

/// The extension `extension` names on this device and its name for the user, or `None` when it is
/// missing, disabled or its `peers` permission is denied (FR-013).
async fn lookup(
    ctx: &PeerContext,
    host: &ExtensionHost,
    extension: &ExtensionRef,
) -> crate::error::Result<Option<Known>> {
    let device = ctx.db.device_id();
    let extension = extension.clone();
    let found = ctx
        .db
        .read(move |q| {
            let Some(id) = extension
                .resolve(q, device)
                .map_err(haex_crdt::Error::from)?
            else {
                return Ok(None);
            };
            let grants =
                candidates(q, id, PermissionKind::Peers, device).map_err(haex_crdt::Error::from)?;
            let name = q.query_row(
                "SELECT coalesce(display_name, name) FROM extensions WHERE id = ?1 \
                 UNION ALL SELECT coalesce(display_name, name) FROM dev_extensions_no_sync \
                 WHERE id = ?1",
                &[&id.to_string()],
                |r| r.get::<_, String>(0),
            )?;
            Ok(Some((id, grants, name.unwrap_or_default())))
        })
        .await?;
    let Some((id, mut grants, name)) = found else {
        return Ok(None);
    };
    grants.extend(host.permissions.temporary(id, PermissionKind::Peers));
    Ok((!denied(&grants, device)).then_some(Known { id, name }))
}

/// Whether `peers` is denied for the extension. A question not yet answered does not deny
/// (FR-013); the bridge asks it when the extension first uses the channel.
fn denied(grants: &[Permission], device: Uuid) -> bool {
    let request = PermissionRequest {
        kind: PermissionKind::Peers,
        action: Action::Connect,
        target: RequestTarget::Any,
    };
    evaluate(grants, &request, device) == Decision::Deny
}

/// The name the address book of this vault has for the contact.
async fn contact_name(ctx: &PeerContext, contact_id: &str) -> Option<String> {
    ContactsService::new(ctx.db.clone())
        .get(&Caller::Internal { feature: FEATURE }, &[], contact_id)
        .await
        .ok()
        .map(|contact| contact.name)
}

#[cfg(test)]
#[path = "invited_tests.rs"]
mod tests;
