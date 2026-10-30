use super::{block_queue, Block};
use std::sync::mpsc::TryRecvError;

#[test]
fn bounded_blocks_return_to_the_preallocated_pool() {
    let (sender, receiver) = block_queue(2, 4);
    let mut first = sender.take().unwrap();
    let mut second = sender.take().unwrap();
    assert!(matches!(sender.take(), Err(TryRecvError::Empty)));

    first.samples.extend_from_slice(&[1.0, 2.0]);
    second.samples.extend_from_slice(&[3.0, 4.0]);
    assert!(sender.send(first));
    assert!(sender.send(second));

    let first = receiver.try_recv().unwrap();
    let second = receiver.try_recv().unwrap();
    assert_eq!(first.samples, [1.0, 2.0]);
    assert_eq!(second.samples, [3.0, 4.0]);
    receiver.recycle(first);
    receiver.recycle(second);

    let recycled = sender.take().unwrap();
    assert!(recycled.samples.is_empty());
    assert!(recycled.samples.capacity() >= 4);
}

#[test]
fn a_full_queue_rejects_an_extra_block() {
    let (sender, receiver) = block_queue(1, 1);
    let block = sender.take().unwrap();
    let overflow = Block {
        samples: Vec::new(),
        index: 0,
    };
    assert!(sender.send(block));
    assert!(!sender.send(overflow));
    assert!(receiver.try_recv().is_ok());
}
