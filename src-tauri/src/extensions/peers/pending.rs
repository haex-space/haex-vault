//! An accepted invitation whose extension has no frame yet (spec 057 T041, FR-016, research R9):
//! holzi opens the extension and keeps what the frames would hear until one is there, so the
//! extension learns the channel (`peers:channel-opened`) before its first message.
//!
//! What waits is bounded like the frames' own queue: beyond [`MAX_HELD`] events the oldest fast
//! message goes first, and when only reliable ones are held a reliable one cannot be dropped
//! without breaking the promise it carries, so the channel ends with `lost` (FR-038).

use std::collections::HashMap;

use serde_json::Value;
use tokio_util::sync::CancellationToken;
use uuid::Uuid;

use super::channel::Frames;

/// Events held for one channel at most, as many as the frames' own queue holds.
pub const MAX_HELD: usize = 256;

/// An event for the frames of the channel's extension that waits for one.
struct Held {
    event: &'static str,
    data: Value,
    /// A reliable message or the channel's own announcement: never dropped quietly.
    kept: bool,
}

struct Waiting {
    extension_id: Uuid,
    frames: Frames,
    held: Vec<Held>,
    /// Ends the wait of 60 s when a frame came or the channel ended.
    cancel: CancellationToken,
}

/// What holding a message came to.
#[derive(Debug, PartialEq, Eq)]
pub(super) enum Hold {
    /// The channel waits for no frame: the message is handed back, to go to the frames.
    Pass(Value),
    /// It waits with the channel's other events, or was a fast message that found no room.
    Held,
    /// A reliable message found no room: the channel cannot go on.
    Overflow,
}

/// The accepted channels that wait for a frame of their extension, by channel.
#[derive(Default)]
pub(super) struct PendingOpens(HashMap<Uuid, Waiting>);

impl PendingOpens {
    /// Starts the wait of `channel`, whose first event is `opened`; the token ends it.
    pub(super) fn insert(
        &mut self,
        channel: Uuid,
        extension_id: Uuid,
        frames: Frames,
        event: &'static str,
        opened: Value,
        cancel: CancellationToken,
    ) {
        self.0.insert(
            channel,
            Waiting {
                extension_id,
                frames,
                held: vec![Held {
                    event,
                    data: opened,
                    kept: true,
                }],
                cancel,
            },
        );
    }

    pub(super) fn hold(
        &mut self,
        channel: Uuid,
        event: &'static str,
        data: Value,
        reliable: bool,
    ) -> Hold {
        let Some(waiting) = self.0.get_mut(&channel) else {
            return Hold::Pass(data);
        };
        if waiting.held.len() >= MAX_HELD {
            match waiting.held.iter().position(|held| !held.kept) {
                Some(oldest) => {
                    waiting.held.remove(oldest);
                }
                None if reliable => return Hold::Overflow,
                None => return Hold::Held,
            }
        }
        waiting.held.push(Held {
            event,
            data,
            kept: reliable,
        });
        Hold::Held
    }

    /// The wait of `channel` is over, whether a frame came or not.
    pub(super) fn remove(&mut self, channel: Uuid) -> bool {
        self.0.remove(&channel).map(|w| w.cancel.cancel()).is_some()
    }

    /// Hands what waits for a frame of `extension_id` to `emit`, channel by channel and in order,
    /// now that one is there. Called under the lock of the caller, so a message that arrives
    /// meanwhile finds the wait over only after the last held event went out.
    pub(super) fn release(
        &mut self,
        extension_id: Uuid,
        mut emit: impl FnMut(&Frames, &'static str, &Value),
    ) {
        let ids: Vec<Uuid> = self
            .0
            .iter()
            .filter(|(_, waiting)| waiting.extension_id == extension_id)
            .map(|(id, _)| *id)
            .collect();
        for waiting in ids.into_iter().filter_map(|id| self.0.remove(&id)) {
            waiting.cancel.cancel();
            for held in &waiting.held {
                emit(&waiting.frames, held.event, &held.data);
            }
        }
    }

    /// How many events wait for `channel`.
    #[cfg(test)]
    pub(super) fn held(&self, channel: Uuid) -> usize {
        self.0.get(&channel).map_or(0, |waiting| waiting.held.len())
    }

    pub(super) fn clear(&mut self) {
        for (_, waiting) in self.0.drain() {
            waiting.cancel.cancel();
        }
    }
}

#[cfg(test)]
#[path = "pending_tests.rs"]
mod tests;
