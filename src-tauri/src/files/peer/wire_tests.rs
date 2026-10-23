//! The heads of `haex-vault-files/1` (contracts/files-protocol.md): a round trip, the size limit, and
//! a stream that ends inside a head.

use super::{read_head, write_head, Caller, Op, Reply, Request, WireEntry, WireKind, VERSION};
use crate::sync::wire::WireError;

#[tokio::test]
async fn a_request_and_an_answer_survive_the_wire() {
    let request = Request {
        v: VERSION,
        caller: Caller::User,
        op: Op::Read {
            share: Some("share-1".to_owned()),
            path: "/Fotos/a.jpg".to_owned(),
            start: 5,
            len: 10,
        },
    };
    let reply = Reply::Entries(vec![WireEntry {
        name: "a.jpg".to_owned(),
        kind: WireKind::File,
        size: 42,
        modified_ms: Some(1_700_000_000_000),
        etag: "\"2a-1\"".to_owned(),
    }]);
    let mut bytes = Vec::new();
    write_head(&mut bytes, &request).await.expect("write");
    write_head(&mut bytes, &reply).await.expect("write");
    let mut read = bytes.as_slice();
    let back: Request = read_head(&mut read).await.expect("read");
    let answer: Reply = read_head(&mut read).await.expect("read");
    assert_eq!(back, request);
    assert_eq!(answer, reply);
}

#[tokio::test]
async fn a_head_over_the_limit_is_refused_both_ways() {
    let huge = Reply::Entries(vec![
        WireEntry {
            name: "x".repeat(1024),
            kind: WireKind::File,
            size: 0,
            modified_ms: None,
            etag: String::new(),
        };
        80
    ]);
    let mut bytes = Vec::new();
    assert!(matches!(
        write_head(&mut bytes, &huge).await,
        Err(WireError::FrameTooLarge { .. })
    ));
    let mut forged = (70_000u32).to_be_bytes().to_vec();
    forged.extend(vec![0; 16]);
    assert!(matches!(
        read_head::<_, Reply>(&mut forged.as_slice()).await,
        Err(WireError::FrameTooLarge { .. })
    ));
}

#[tokio::test]
async fn a_stream_that_ends_inside_a_head_is_an_error() {
    let mut bytes = Vec::new();
    write_head(&mut bytes, &Reply::Shares(Vec::new()))
        .await
        .expect("write");
    bytes.truncate(bytes.len() - 1);
    bytes.push(0xff);
    let mut short = (bytes.len() as u32 + 4).to_be_bytes().to_vec();
    short.extend(&bytes[4..]);
    assert!(read_head::<_, Reply>(&mut short.as_slice()).await.is_err());
    assert!(matches!(
        read_head::<_, Reply>(&mut [].as_slice()).await,
        Err(WireError::Closed)
    ));
}
