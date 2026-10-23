//! The channels of this device in memory (data-model.md), in [`ExtensionHost`] for the one vault
//! of the process; nothing of it is stored (FR-042). Every lookup takes the calling extension and
//! answers an id of another extension exactly like one that does not exist (FR-006).
//!
//! It also holds the open invitations of both sides and the running endpoint of the vault
//! session, which the joining side of a lobby and the inviting side dial from.
//!
//! [`ExtensionHost`]: crate::extensions::host::ExtensionHost

use std::collections::{BTreeSet, HashMap};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError, Weak};
use std::time::Duration;

use serde_json::Value;
use tokio_util::task::TaskTracker;
use uuid::Uuid;

use super::admission::PeerContext;
use super::channel::{Channel, EndReason, Frames};
use super::invite::{OutgoingInvites, CHANNEL_OPENED};
use super::invited::{Answered, Invitations};
use super::limits::{CHANNELS_PER_EXTENSION, INVITE_LIFETIME, PENDING_OPEN_LIMIT};
use super::lobby::{Lobbies, Openness};
use super::PeerError;
use crate::sync::endpoint::SyncNode;
use crate::sync::foreign_auth::{SeenGenerations, Verified};

/// The waits of invitations, which tests shorten instead of waiting them out.
#[derive(Debug, Clone, Copy)]
pub struct Timing {
    /// How long an invitation stays open (FR-019).
    pub invite_lifetime: Duration,
    /// How long an accepted invitation waits for a frame of its extension (R9).
    pub pending_open: Duration,
}

impl Default for Timing {
    fn default() -> Self {
        Self {
            invite_lifetime: INVITE_LIFETIME,
            pending_open: PENDING_OPEN_LIMIT,
        }
    }
}

#[derive(Default)]
pub struct PeerState {
    channels: Mutex<HashMap<Uuid, Arc<Channel>>>,
    lobbies: Mutex<Lobbies>,
    /// Invitations this device sent and has no end of yet.
    outgoing: Mutex<OutgoingInvites>,
    /// Invitations of contacts this device asks its user about.
    incoming: Mutex<Invitations>,
    timing: Mutex<Timing>,
    /// The endpoint of the vault session while it runs. Held weakly: the session owns its node,
    /// and a node held here would keep the session from draining.
    session: Mutex<Option<(Arc<PeerContext>, Weak<SyncNode>)>>,
    /// The highest device-list generation seen per foreign vault in this session.
    seen: SeenGenerations,
}

/// The running endpoint of the vault session and what channels need of this device.
pub struct Session {
    pub ctx: Arc<PeerContext>,
    pub node: Arc<SyncNode>,
}

impl PeerState {
    fn channels(&self) -> MutexGuard<'_, HashMap<Uuid, Arc<Channel>>> {
        self.channels.lock().unwrap_or_else(PoisonError::into_inner)
    }

    pub(super) fn lobbies(&self) -> MutexGuard<'_, Lobbies> {
        self.lobbies.lock().unwrap_or_else(PoisonError::into_inner)
    }

    pub(super) fn outgoing(&self) -> MutexGuard<'_, OutgoingInvites> {
        self.outgoing.lock().unwrap_or_else(PoisonError::into_inner)
    }

    pub(super) fn invitations(&self) -> MutexGuard<'_, Invitations> {
        self.incoming.lock().unwrap_or_else(PoisonError::into_inner)
    }

    pub fn timing(&self) -> Timing {
        *self.timing.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// Replaces the waits, so a test need not wait them out.
    #[cfg(test)]
    pub(crate) fn set_timing(&self, timing: Timing) {
        *self.timing.lock().unwrap_or_else(PoisonError::into_inner) = timing;
    }

    fn session_slot(&self) -> MutexGuard<'_, Option<(Arc<PeerContext>, Weak<SyncNode>)>> {
        self.session.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// Makes `node` the endpoint channels of this vault session dial from; the sync service
    /// calls it once its endpoint is bound, and [`Self::clear`] forgets it.
    pub fn attach(&self, ctx: Arc<PeerContext>, node: &Arc<SyncNode>) {
        *self.session_slot() = Some((ctx, Arc::downgrade(node)));
    }

    /// The endpoint of the running vault session; `None` while there is none (FR-044).
    pub fn session(&self) -> Option<Session> {
        let slot = self.session_slot();
        let (ctx, node) = slot.as_ref()?;
        Some(Session {
            ctx: Arc::clone(ctx),
            node: node.upgrade()?,
        })
    }

    /// Which lobbies are open, for the admission of devices that are not verified (FR-031).
    pub fn openness(&self) -> Openness {
        self.lobbies().openness()
    }

    /// Whether `extension_id` has [`CHANNELS_PER_EXTENSION`] channels already.
    pub fn channels_full(&self, extension_id: Uuid) -> bool {
        full(&self.channels(), extension_id)
    }

    pub fn seen(&self) -> &SeenGenerations {
        &self.seen
    }

    /// Registers an opened channel; refused with `limit` once its extension has
    /// [`CHANNELS_PER_EXTENSION`].
    pub(super) fn add(&self, channel: Arc<Channel>) -> Result<(), PeerError> {
        let mut channels = self.channels();
        if full(&channels, channel.extension_id) {
            return Err(PeerError::Limit {
                limit: CHANNELS_PER_EXTENSION,
            });
        }
        channels.insert(channel.id, channel);
        Ok(())
    }

    /// The open channel `id` of `extension_id`.
    pub fn channel(&self, extension_id: Uuid, id: Uuid) -> Result<Arc<Channel>, PeerError> {
        self.channels()
            .get(&id)
            .filter(|channel| channel.extension_id == extension_id)
            .cloned()
            .ok_or(PeerError::ChannelUnknown)
    }

    /// Closes the channel `id` of `extension_id` on both sides; both hear `closed`.
    pub fn close(&self, extension_id: Uuid, id: Uuid) -> Result<(), PeerError> {
        self.channel(extension_id, id)?;
        self.finish(id, EndReason::Closed);
        Ok(())
    }

    /// Ends the channel `id` with `reason`: forgets it, stops it (`Close`, then the connection
    /// closes) and tells the frames of its extension. Whoever comes first does it; a channel that
    /// already ended is left alone, so each side hears of the end exactly once (FR-040).
    pub fn finish(&self, id: Uuid, reason: EndReason) {
        let Some(channel) = self.channels().remove(&id) else {
            return;
        };
        channel.shut();
        channel.tell_closed(reason);
    }

    /// Ends everything of `extension_id` on this device (FR-008, FR-034): its permission went,
    /// it was disabled or removed, or its last frame closed. Its lobbies close, its channels end,
    /// the invitations it sent are withdrawn (no status follows) and the ones it was invited by
    /// go from the prompt.
    pub fn end_all(&self, extension_id: Uuid) {
        self.lobbies().close_all(extension_id);
        let withdrawn = self.outgoing().take_all(extension_id);
        withdrawn.iter().for_each(|stop| stop.cancel());
        self.invitations().end_all(extension_id);
        self.end_where(|channel| channel.extension_id == extension_id);
    }

    /// Takes the answer of the user to the invitation `invite` (the prompt's buttons or the
    /// window's command): `Open` when it was still waiting for one, else `Gone`.
    pub fn answer_invitation(&self, invite: Uuid, accept: bool) -> Answered {
        self.invitations().answer(invite, accept)
    }

    /// The other side of every invitation waiting for an answer, by invitation.
    pub fn invitation_remotes(&self) -> Vec<(Uuid, Verified)> {
        self.invitations().remotes()
    }

    /// Ends the invitation `invite` waiting for an answer: its inviter lost the ground.
    pub fn end_invitation(&self, invite: Uuid) {
        self.invitations().end_one(invite);
    }

    /// Tells the frames of `channel`'s extension that an invitation became a channel
    /// (`peers:channel-opened`, FR-016).
    pub(super) fn opened(
        &self,
        channel: &Channel,
        frames: &Frames,
        event: Value,
        _tracker: &TaskTracker,
    ) {
        frames.emit(channel.extension_id, CHANNEL_OPENED, &event);
    }

    /// Ends every channel `ended` picks, with `closed`.
    pub fn end_where(&self, ended: impl Fn(&Channel) -> bool) {
        let ids: Vec<Uuid> = self
            .channels()
            .values()
            .filter(|channel| ended(channel))
            .map(|channel| channel.id)
            .collect();
        for id in ids {
            self.finish(id, EndReason::Closed);
        }
    }

    /// The extensions with open channels, lobbies or invitations.
    pub fn extensions(&self) -> BTreeSet<Uuid> {
        let mut extensions: BTreeSet<Uuid> =
            self.channels().values().map(|c| c.extension_id).collect();
        extensions.extend(self.lobbies().extensions());
        extensions.extend(self.outgoing().extensions());
        extensions.extend(self.invitations().extensions());
        extensions
    }

    /// The other side of every open channel, by channel.
    pub fn remotes(&self) -> Vec<(Uuid, Verified)> {
        self.channels()
            .values()
            .map(|c| (c.id, c.remote.clone()))
            .collect()
    }

    /// Ends every channel, closes every lobby, withdraws every invitation and forgets the
    /// session's endpoint and list generations: the vault session ended.
    pub fn clear(&self) {
        *self.session_slot() = None;
        self.lobbies().clear();
        let withdrawn = self.outgoing().clear();
        withdrawn.iter().for_each(|stop| stop.cancel());
        self.invitations().clear();
        self.end_where(|_| true);
        self.seen.clear();
    }
}

fn full(channels: &HashMap<Uuid, Arc<Channel>>, extension_id: Uuid) -> bool {
    channels
        .values()
        .filter(|c| c.extension_id == extension_id)
        .count()
        >= CHANNELS_PER_EXTENSION
}

#[cfg(test)]
#[path = "state_tests.rs"]
mod tests;
