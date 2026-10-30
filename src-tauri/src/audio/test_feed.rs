//! A microphone for tests: the test sends blocks, the stream reads them.

use std::sync::mpsc::{channel, Receiver, Sender, TryRecvError};

use super::source::{Read, Source};

pub(crate) struct Feed(Receiver<Vec<f32>>);

/// The sending end speaks into the feed; dropping it ends the feed.
pub(crate) fn feed() -> (Sender<Vec<f32>>, Feed) {
    let (sender, receiver) = channel();
    (sender, Feed(receiver))
}

impl Source for Feed {
    fn read(&mut self, out: &mut Vec<f32>) -> Read {
        match self.0.try_recv() {
            Ok(block) => {
                out.extend(block);
                Read::Data
            }
            Err(TryRecvError::Empty) => Read::Idle,
            Err(TryRecvError::Disconnected) => Read::Ended,
        }
    }
}
