//! The channels of this device in memory (data-model.md), in [`ExtensionHost`] for the one vault
//! of the process; nothing of it is stored (FR-042). Every lookup takes the calling extension and
//! answers an id of another extension exactly like one that does not exist (FR-006).
//!
//! Lobbies and invitations join this state with the deliveries that bring them (PR 2 and 3).
//!
//! [`ExtensionHost`]: crate::extensions::host::ExtensionHost

use std::collections::{BTreeSet, HashMap};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};

use uuid::Uuid;

use super::channel::{Channel, EndReason};
use super::limits::CHANNELS_PER_EXTENSION;
use super::PeerError;
use crate::sync::foreign_auth::{SeenGenerations, Verified};

#[derive(Default)]
pub struct PeerState {
    channels: Mutex<HashMap<Uuid, Arc<Channel>>>,
    /// The highest device-list generation seen per foreign vault in this session.
    seen: SeenGenerations,
}

impl PeerState {
    fn channels(&self) -> MutexGuard<'_, HashMap<Uuid, Arc<Channel>>> {
        self.channels.lock().unwrap_or_else(PoisonError::into_inner)
    }

    pub fn seen(&self) -> &SeenGenerations {
        &self.seen
    }

    /// Registers an opened channel; refused with `limit` once its extension has
    /// [`CHANNELS_PER_EXTENSION`].
    pub(super) fn add(&self, channel: Arc<Channel>) -> Result<(), PeerError> {
        let mut channels = self.channels();
        let open = channels
            .values()
            .filter(|c| c.extension_id == channel.extension_id)
            .count();
        if open >= CHANNELS_PER_EXTENSION {
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

    /// The extensions with open channels.
    pub fn extensions(&self) -> BTreeSet<Uuid> {
        self.channels().values().map(|c| c.extension_id).collect()
    }

    /// The other side of every open channel, by channel.
    pub fn remotes(&self) -> Vec<(Uuid, Verified)> {
        self.channels()
            .values()
            .map(|c| (c.id, c.remote.clone()))
            .collect()
    }

    /// Ends every channel and forgets the session's list generations: the vault session ended.
    pub fn clear(&self) {
        self.end_where(|_| true);
        self.seen.clear();
    }
}

#[cfg(test)]
#[path = "state_tests.rs"]
mod tests;
