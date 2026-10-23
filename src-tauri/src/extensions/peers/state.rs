//! The channels of this device in memory (data-model.md), in [`ExtensionHost`] for the one vault
//! of the process; nothing of it is stored (FR-042). Every lookup takes the calling extension and
//! answers an id of another extension exactly like one that does not exist (FR-006).
//!
//! Invitations join this state with the delivery that brings them (PR 3). It also holds the
//! running endpoint of the vault session, which the joining side of a lobby dials from.
//!
//! [`ExtensionHost`]: crate::extensions::host::ExtensionHost

use std::collections::{BTreeSet, HashMap};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError, Weak};

use uuid::Uuid;

use super::admission::PeerContext;
use super::channel::{Channel, EndReason};
use super::limits::CHANNELS_PER_EXTENSION;
use super::lobby::{Lobbies, Openness};
use super::PeerError;
use crate::sync::endpoint::SyncNode;
use crate::sync::foreign_auth::{SeenGenerations, Verified};

#[derive(Default)]
pub struct PeerState {
    channels: Mutex<HashMap<Uuid, Arc<Channel>>>,
    lobbies: Mutex<Lobbies>,
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
    /// it was disabled or removed, or its last frame closed.
    pub fn end_all(&self, extension_id: Uuid) {
        self.lobbies().close_all(extension_id);
        self.end_where(|channel| channel.extension_id == extension_id);
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

    /// The extensions with open channels or lobbies.
    pub fn extensions(&self) -> BTreeSet<Uuid> {
        let mut extensions: BTreeSet<Uuid> =
            self.channels().values().map(|c| c.extension_id).collect();
        extensions.extend(self.lobbies().extensions());
        extensions
    }

    /// The other side of every open channel, by channel.
    pub fn remotes(&self) -> Vec<(Uuid, Verified)> {
        self.channels()
            .values()
            .map(|c| (c.id, c.remote.clone()))
            .collect()
    }

    /// Ends every channel, closes every lobby and forgets the session's endpoint and list
    /// generations: the vault session ended.
    pub fn clear(&self) {
        *self.session_slot() = None;
        self.lobbies().clear();
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
