//! Fixed blocks of samples handed between an audio callback and a worker thread (spec 066,
//! research R2).
//!
//! The blocks are allocated once and travel in a circle: the sending side takes an empty one from
//! the pool, fills it and passes it on; the receiving side hands it back when it is done. Both
//! directions are bounded `std::sync::mpsc` channels, whose `try_send` and `try_recv` neither
//! allocate nor lock as long as no thread waits on the channel. So nobody ever blocks on these
//! channels: both sides poll, and the realtime side never touches a lock.

use std::sync::mpsc::{sync_channel, Receiver, SyncSender, TryRecvError};

/// One block of mono or interleaved samples. `index` counts the blocks a sender has sent, so the
/// receiver can tell which of them a later stop request covers.
pub struct Block {
    pub samples: Vec<f32>,
    pub index: u64,
}

/// The sending side: takes empty blocks from the pool and sends filled ones.
pub struct BlockSender {
    free: Receiver<Block>,
    full: SyncSender<Block>,
}

/// The receiving side: receives filled blocks and returns them to the pool.
pub struct BlockReceiver {
    full: Receiver<Block>,
    free: SyncSender<Block>,
}

/// A pool of `blocks` blocks of `block_len` samples each.
pub fn block_queue(blocks: usize, block_len: usize) -> (BlockSender, BlockReceiver) {
    let (free_tx, free_rx) = sync_channel(blocks);
    let (full_tx, full_rx) = sync_channel(blocks);
    for _ in 0..blocks {
        // The channel holds exactly `blocks` entries, so every send fits.
        let _ = free_tx.try_send(Block {
            samples: Vec::with_capacity(block_len),
            index: 0,
        });
    }
    (
        BlockSender {
            free: free_rx,
            full: full_tx,
        },
        BlockReceiver {
            full: full_rx,
            free: free_tx,
        },
    )
}

impl BlockSender {
    /// An empty block from the pool. `Empty` when all blocks are on their way (the receiver is
    /// behind), `Disconnected` when the receiver is gone.
    pub fn take(&self) -> Result<Block, TryRecvError> {
        let mut block = self.free.try_recv()?;
        block.samples.clear();
        Ok(block)
    }

    /// Passes a filled block on. It always fits: there are never more blocks than the channel
    /// holds. Returns `false` when the receiver is gone; the block is then dropped, which frees
    /// memory, but that only happens while a stream ends.
    pub fn send(&self, block: Block) -> bool {
        self.full.try_send(block).is_ok()
    }
}

impl BlockReceiver {
    /// The next filled block, if one is waiting.
    pub fn try_recv(&self) -> Result<Block, TryRecvError> {
        self.full.try_recv()
    }

    /// Returns a block to the pool. A block that cannot go back (the sender is gone) is dropped,
    /// which frees memory; that only happens when the stream ends.
    pub fn recycle(&self, block: Block) {
        let _ = self.free.try_send(block);
    }
}
