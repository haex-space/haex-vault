//! Tests for the sources the media server reads from: the ranges of a file held in memory
//! (spec 058 R3).

use std::sync::Arc;

use tokio::io::AsyncReadExt;

use super::{BytesSource, StreamingSource};

fn source() -> BytesSource {
    BytesSource::new(Arc::from(&b"0123456789"[..]), "application/pdf")
}

async fn read(source: &BytesSource, start: u64, len: u64) -> Vec<u8> {
    let mut reader = source.open_range(start, len).await.unwrap();
    let mut bytes = Vec::new();
    reader.read_to_end(&mut bytes).await.unwrap();
    bytes
}

#[tokio::test]
async fn bytes_report_their_size_and_type() {
    let source = source();
    assert_eq!(source.size().await.unwrap(), 10);
    assert_eq!(source.content_type(), "application/pdf");
}

#[tokio::test]
async fn the_whole_range_reads_every_byte() {
    assert_eq!(read(&source(), 0, 10).await, b"0123456789");
}

#[tokio::test]
async fn a_range_in_the_middle_reads_exactly_that_slice() {
    assert_eq!(read(&source(), 3, 4).await, b"3456");
}

#[tokio::test]
async fn a_range_past_the_end_stops_at_the_end() {
    assert_eq!(read(&source(), 8, 100).await, b"89");
    assert_eq!(read(&source(), 0, u64::MAX).await, b"0123456789");
}

#[tokio::test]
async fn a_range_at_or_beyond_the_end_reads_nothing() {
    assert!(read(&source(), 10, 5).await.is_empty());
    assert!(read(&source(), u64::MAX, 5).await.is_empty());
}
