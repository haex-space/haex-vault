//! The tasks of one invitation on the inviting side (spec 057 US1, research R10): one per device
//! of the card, from the dial to the device's answer, and one that ends the invitation when its
//! time is up. [`super::invite`] holds what they share and what the extension calls.

use std::sync::Arc;
use std::time::Duration;

use iroh::endpoint::{Connection, RecvStream, SendStream};
use iroh::EndpointAddr;
use tokio::time::timeout;
use tokio_util::sync::CancellationToken;
use tokio_util::task::TaskTracker;
use uuid::Uuid;

use super::admission::{refuse, PeerContext, PURPOSE};
use super::channel::{self, Channel, Opening, Origin};
use super::invite::{Device, Final, INVITE_STATUS};
use super::limits::INVITE_DIAL_LIMIT;
use super::lobby::write_reply;
use super::wire::{self, InviteStatus, Message, ADMISSION_FRAME_LIMIT, VERSION};
use super::ExtensionRef;
use crate::sync::endpoint::SyncNode;
use crate::sync::foreign_auth::{self, Trust, Verified};
use crate::sync::wire::EXT_PEER_ALPN;
use serde_json::json;

/// How long the other side gets to hear a `Withdraw` before the connection closes.
const WITHDRAW_GRACE: Duration = Duration::from_secs(1);

/// Everything the tasks of one invitation share.
pub(super) struct Run {
    pub(super) ctx: Arc<PeerContext>,
    pub(super) tracker: TaskTracker,
    pub(super) id: Uuid,
    pub(super) extension_id: Uuid,
    pub(super) extension: ExtensionRef,
    pub(super) contact_id: String,
    pub(super) payload: Vec<u8>,
    pub(super) expires_at: u64,
    pub(super) stop: CancellationToken,
}

/// A device of the card that was reached and proved itself.
struct Link {
    connection: Connection,
    send: SendStream,
    recv: RecvStream,
    verified: Verified,
}

/// Starts the tasks of the invitation: its expiry and one task for each of `addrs`, which the
/// card names.
pub(super) fn start(run: Run, node: &Arc<SyncNode>, addrs: Vec<EndpointAddr>, lifetime: Duration) {
    let run = Arc::new(run);
    let tracker = run.tracker.clone();
    tracker.spawn(expire(Arc::clone(&run), lifetime));
    if addrs.is_empty() {
        // A card without a device: nothing to wait for.
        tracker.spawn(settle_empty(Arc::clone(&run)));
    }
    for (index, addr) in addrs.into_iter().enumerate() {
        tracker.spawn(device(Arc::clone(&run), Arc::clone(node), index, addr));
    }
}

impl Run {
    fn frames(&self) -> &channel::Frames {
        &self.ctx.frames
    }

    /// Notes what device `index` did; when that settles the invitation, its status goes out.
    fn device_done(&self, index: usize, device: Device) {
        let Some(host) = self.frames().host() else {
            return;
        };
        let ended = host.peers.outgoing().mark(self.id, index, device);
        if let Some((outgoing, ended)) = ended {
            outgoing.stop.cancel();
            self.frames()
                .emit(self.extension_id, INVITE_STATUS, &ended.event(self.id));
        }
    }

    /// Ends the invitation with `ended` if nobody did yet.
    fn end_with(&self, ended: Final) {
        let Some(host) = self.frames().host() else {
            return;
        };
        let taken = host.peers.outgoing().take(self.id);
        if let Some(outgoing) = taken {
            outgoing.stop.cancel();
            self.frames()
                .emit(self.extension_id, INVITE_STATUS, &ended.event(self.id));
        }
    }
}

/// An invitation to a card without devices is over at once.
async fn settle_empty(run: Arc<Run>) {
    run.end_with(Final::Unreachable);
}

/// The invitation expires after `lifetime` (FR-019): its devices hear `Withdraw`.
async fn expire(run: Arc<Run>, lifetime: Duration) {
    tokio::select! {
        () = run.stop.cancelled() => {}
        () = tokio::time::sleep(lifetime) => run.end_with(Final::Expired),
    }
}

/// One device of the card, from the dial to its answer.
async fn device(run: Arc<Run>, node: Arc<SyncNode>, index: usize, addr: EndpointAddr) {
    let reached = reach(&run, &node, addr).await;
    drop(node);
    let Some(mut link) = reached else {
        return run.device_done(index, Device::Unreachable);
    };
    if run.stop.is_cancelled() {
        return refuse(&link.connection);
    }
    let invite = Message::Invite {
        v: VERSION,
        invite: *run.id.as_bytes(),
        extension: run.extension.clone(),
        payload: run.payload.clone(),
        expires_at: run.expires_at,
    };
    if !write_reply(&mut link.send, &invite).await {
        refuse(&link.connection);
        return run.device_done(index, Device::Unreachable);
    }
    run.device_done(index, Device::Waiting);
    let reply = tokio::select! {
        biased;
        () = run.stop.cancelled() => return withdraw(link, run.id).await,
        reply = wire::read(&mut link.recv, ADMISSION_FRAME_LIMIT) => reply,
    };
    match reply {
        Ok(Some(Message::InviteReply { invite, status, .. })) if invite == *run.id.as_bytes() => {
            match status {
                InviteStatus::Accepted => win(&run, link).await,
                InviteStatus::Declined => {
                    refuse(&link.connection);
                    run.device_done(index, Device::Declined);
                }
                InviteStatus::NotInstalled => {
                    refuse(&link.connection);
                    run.device_done(index, Device::NotInstalled);
                }
            }
        }
        // A cut connection, another message or no answer: this device is gone for the invitation.
        _ => {
            refuse(&link.connection);
            run.device_done(index, Device::Unreachable);
        }
    }
}

/// Dials `addr` and runs the exchange within [`INVITE_DIAL_LIMIT`]; only a device of the invited
/// contact's vault counts (`None` for any other, as for one that is not reached).
async fn reach(run: &Run, node: &SyncNode, addr: EndpointAddr) -> Option<Link> {
    let dial = async {
        let connection = node
            .dial_alpn(addr, EXT_PEER_ALPN)
            .await
            .map_err(|error| log::debug!("peers: a device of a card was not reached: {error}"))
            .ok()?;
        let reached = prove(run, &connection).await;
        if reached.is_none() {
            refuse(&connection);
        }
        let (send, recv, verified) = reached?;
        Some(Link {
            connection,
            send,
            recv,
            verified,
        })
    };
    timeout(INVITE_DIAL_LIMIT, dial).await.ok().flatten()
}

async fn prove(run: &Run, connection: &Connection) -> Option<(SendStream, RecvStream, Verified)> {
    let host = run.frames().host()?;
    let (mut send, mut recv) = connection.open_bi().await.ok()?;
    let remote = *connection.remote_id().as_bytes();
    let local = run.ctx.local(host.peers.seen());
    let verified = foreign_auth::dial(&mut send, &mut recv, &local, PURPOSE, remote)
        .await
        .map_err(|error| log::debug!("peers: the exchange with a device of a card ended: {error}"))
        .ok()?;
    matches!(&verified.trust, Trust::Contact { contact_id, .. } if *contact_id == run.contact_id)
        .then_some((send, recv, verified))
}

/// The first acceptance: the invitation is taken, the device gets `InviteConfirm` and the
/// connection becomes the channel; everything else of the invitation stops.
async fn win(run: &Run, mut link: Link) {
    let Some(host) = run.frames().host() else {
        return refuse(&link.connection);
    };
    let taken = host.peers.outgoing().take(run.id);
    let Some(taken) = taken else {
        // Another device was first, or the invitation ended: this one is withdrawn.
        return withdraw(link, run.id).await;
    };
    taken.stop.cancel();
    let confirm = Message::InviteConfirm {
        v: VERSION,
        invite: *run.id.as_bytes(),
    };
    if !write_reply(&mut link.send, &confirm).await {
        refuse(&link.connection);
        return run.frames().emit(
            run.extension_id,
            INVITE_STATUS,
            &Final::Unreachable.event(run.id),
        );
    }
    let opening = Opening {
        extension_id: run.extension_id,
        remote: link.verified,
        origin: Origin::Invitation { invite: run.id },
        connection: link.connection,
        send: link.send,
        recv: link.recv,
    };
    let accepted = |channel: &Channel| {
        let event = json!({
            "inviteId": run.id.to_string(),
            "status": "accepted",
            "channelId": channel.id.to_string(),
        });
        run.frames().emit(run.extension_id, INVITE_STATUS, &event);
    };
    // ponytail: a channel that cannot open here (the extension reached its limit between the
    // check of `invite` and now) ends the invitation as `unreachable`, which is not quite what
    // happened. Upgrade: a status of its own, with the contract.
    if let Err(error) = channel::open_announced(opening, run.frames(), &run.tracker, accepted) {
        log::debug!("peers: an accepted invitation did not become a channel: {error}");
        run.frames().emit(
            run.extension_id,
            INVITE_STATUS,
            &Final::Unreachable.event(run.id),
        );
    }
}

/// Tells the device the invitation is gone and closes the connection.
async fn withdraw(mut link: Link, invite: Uuid) {
    let message = Message::Withdraw {
        v: VERSION,
        invite: *invite.as_bytes(),
    };
    if timeout(
        WITHDRAW_GRACE,
        wire::write(&mut link.send, &message, ADMISSION_FRAME_LIMIT),
    )
    .await
    .is_ok_and(|written| written.is_ok())
    {
        // The other side closes on hearing it; if it does not, the grace ends the wait.
        let _ = link.send.finish();
        let _ = timeout(WITHDRAW_GRACE, link.connection.closed()).await;
    }
    refuse(&link.connection);
}
