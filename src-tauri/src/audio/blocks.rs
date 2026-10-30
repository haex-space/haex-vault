//! Fixed blocks of samples handed between an audio callback and a worker thread (spec 066,
//! research R2). Blocks travel through bounded lock-free queues and are allocated once, so the
//! realtime callbacks neither wait for another thread nor allocate.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::TryRecvError;
use std::sync::Arc;

use crossbeam_queue::ArrayQueue;

/// One block of mono or interleaved samples. `index` counts the blocks a sender has sent, so the
/// receiver can tell which of them a later stop request covers.
pub struct Block {
    pub samples: Vec<f32>,
    pub index: u64,
}

/// The sending side: takes empty blocks from the pool and sends filled ones.
pub struct BlockSender {
    free: Arc<ArrayQueue<Block>>,
    full: Arc<ArrayQueue<Block>>,
    sender_open: Arc<AtomicBool>,
    receiver_open: Arc<AtomicBool>,
}

/// The receiving side: receives filled blocks and returns them to the pool.
pub struct BlockReceiver {
    full: Arc<ArrayQueue<Block>>,
    free: Arc<ArrayQueue<Block>>,
    sender_open: Arc<AtomicBool>,
    receiver_open: Arc<AtomicBool>,
}

/// A pool of `blocks` blocks of `block_len` samples each.
pub fn block_queue(blocks: usize, block_len: usize) -> (BlockSender, BlockReceiver) {
    let free = Arc::new(ArrayQueue::new(blocks));
    let full = Arc::new(ArrayQueue::new(blocks));
    let sender_open = Arc::new(AtomicBool::new(true));
    let receiver_open = Arc::new(AtomicBool::new(true));
    for _ in 0..blocks {
        // The queue holds exactly `blocks` entries, so every push fits.
        let _ = free.push(Block {
            samples: Vec::with_capacity(block_len),
            index: 0,
        });
    }
    (
        BlockSender {
            free: Arc::clone(&free),
            full: Arc::clone(&full),
            sender_open: Arc::clone(&sender_open),
            receiver_open: Arc::clone(&receiver_open),
        },
        BlockReceiver {
            full,
            free,
            sender_open,
            receiver_open,
        },
    )
}

impl BlockSender {
    /// An empty block from the pool. `Empty` when all blocks are on their way (the receiver is
    /// behind).
    pub fn take(&self) -> Result<Block, TryRecvError> {
        let mut block = self.free.pop().ok_or_else(|| {
            if self.receiver_open.load(Ordering::Acquire) {
                TryRecvError::Empty
            } else {
                TryRecvError::Disconnected
            }
        })?;
        block.samples.clear();
        Ok(block)
    }

    /// Passes a filled block on. It always fits: there are never more blocks than the queue holds.
    /// Returns `false` if the receiver is behind; the block is then dropped and its memory freed.
    pub fn send(&self, block: Block) -> bool {
        self.receiver_open.load(Ordering::Acquire) && self.full.push(block).is_ok()
    }
}

impl BlockReceiver {
    /// The next filled block, if one is waiting.
    pub fn try_recv(&self) -> Result<Block, TryRecvError> {
        self.full.pop().ok_or_else(|| {
            if self.sender_open.load(Ordering::Acquire) {
                TryRecvError::Empty
            } else {
                TryRecvError::Disconnected
            }
        })
    }

    /// Returns a block to the pool. A block that cannot go back (the sender is behind) is dropped.
    pub fn recycle(&self, block: Block) {
        if self.sender_open.load(Ordering::Acquire) {
            let _ = self.free.push(block);
        }
    }
}

impl Drop for BlockSender {
    fn drop(&mut self) {
        self.sender_open.store(false, Ordering::Release);
    }
}

impl Drop for BlockReceiver {
    fn drop(&mut self) {
        self.receiver_open.store(false, Ordering::Release);
    }
}

#[cfg(test)]
#[path = "blocks_tests.rs"]
mod tests;
