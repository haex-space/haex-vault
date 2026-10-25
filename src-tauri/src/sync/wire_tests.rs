use tokio::io::{duplex, AsyncWriteExt};
use uuid::Uuid;

use super::*;
use crate::sync::change::Change;

fn samples() -> Vec<Message> {
    let list = ListRef {
        generation: 3,
        list_hash: [9; 32],
    };
    let schema = SchemaVersion {
        protocol: 1,
        holzi_migration: 21,
        crdt_trigger: 13,
    };
    vec![
        Message::Challenge {
            v: PROTOCOL_VERSION,
            nonce_a: [1; 32],
            endpoint_a: [2; 32],
            list,
            schema,
        },
        Message::Response {
            v: PROTOCOL_VERSION,
            device_d: [3; 32],
            vault: [4; 32],
            nonce_d: [5; 32],
            schema,
            list,
            sig_d: Signature([6; 64]),
        },
        Message::Accept {
            device_a: [7; 32],
            schema,
            list,
            sig_a: Signature([8; 64]),
        },
        Message::Reject {
            code: RejectCode::Removed,
        },
        Message::DeviceListPush {
            payload: vec![1, 2, 3],
            signature: Signature([0; 64]),
        },
        Message::Progress {
            vector: Vector::from([(Uuid::new_v4(), "7/1".to_string())]),
            last_seen: vec![([1; 32], 42)],
        },
        Message::Pull {
            vector: Vector::new(),
            replace: false,
        },
        Message::Page(Page {
            changes: vec![Change {
                table: "chat_threads".to_string(),
                row_pks: r#"{"id":"t"}"#.to_string(),
                column: "title".to_string(),
                hlc: "7/1".to_string(),
                value: r#""x""#.to_string(),
                continues: false,
            }],
            group_continues: false,
            more: false,
            served: Vector::new(),
        }),
        Message::Resync {
            reason: ResyncReason::TombstonesExpired,
        },
    ]
}

#[tokio::test]
async fn every_message_survives_a_frame() {
    let (mut a, mut b) = duplex(1 << 20);
    for message in samples() {
        write_frame(&mut a, &message, FRAME_LIMIT)
            .await
            .expect("write");
        let read = expect_frame(&mut b, FRAME_LIMIT).await.expect("read");
        assert_eq!(read, message);
    }
}

#[tokio::test]
async fn an_oversized_length_is_refused_before_reading_the_body() {
    let (mut a, mut b) = duplex(64);
    // Only the length prefix is sent: the reader must refuse without waiting
    // for, or allocating, the announced body.
    let announced = (HANDSHAKE_FRAME_LIMIT as u32 + 1).to_be_bytes();
    a.write_all(&announced).await.expect("write length");

    let error = read_frame(&mut b, HANDSHAKE_FRAME_LIMIT)
        .await
        .expect_err("too large");
    assert!(matches!(error, WireError::FrameTooLarge { .. }));
    assert_eq!(error.code(), ErrorCode::FrameTooLarge);
}

#[tokio::test]
async fn a_message_over_the_limit_is_not_sent() {
    let (mut a, _b) = duplex(64);
    let message = Message::DeviceListPush {
        payload: vec![0; 100],
        signature: Signature([0; 64]),
    };
    let error = write_frame(&mut a, &message, 64)
        .await
        .expect_err("too large");
    assert!(matches!(error, WireError::FrameTooLarge { .. }));
}

#[tokio::test]
async fn an_unknown_message_is_a_protocol_error() {
    let (mut a, mut b) = duplex(64);
    // Variant index 200 does not exist.
    a.write_all(&[0, 0, 0, 1, 200]).await.expect("write");
    let error = read_frame(&mut b, FRAME_LIMIT).await.expect_err("unknown");
    assert_eq!(error.code(), ErrorCode::Protocol);
}

#[tokio::test]
async fn a_clean_end_before_a_frame_is_no_error() {
    let (a, mut b) = duplex(64);
    drop(a);
    assert!(read_frame(&mut b, FRAME_LIMIT)
        .await
        .expect("end")
        .is_none());
}

#[tokio::test]
async fn an_end_inside_a_frame_is_an_error() {
    let (mut a, mut b) = duplex(64);
    a.write_all(&[0, 0, 0, 10, 1]).await.expect("write");
    drop(a);
    assert!(matches!(
        read_frame(&mut b, FRAME_LIMIT).await,
        Err(WireError::Closed)
    ));
}

fn challenge(v: u8, schema: SchemaVersion) -> Message {
    Message::Challenge {
        v,
        nonce_a: [1; 32],
        endpoint_a: [2; 32],
        list: ListRef {
            generation: 3,
            list_hash: [9; 32],
        },
        schema,
    }
}

const SCHEMA: SchemaVersion = SchemaVersion {
    protocol: 2,
    holzi_migration: 40,
    crdt_trigger: 27,
};

/// Spec 059 research R2: the version sits right after the variant tag, and a reader that knows
/// only the fields up to `list` decodes a `Challenge` that has `schema` after them.
#[test]
fn the_version_is_the_byte_after_the_challenge_tag() {
    let bytes = postcard::to_stdvec(&challenge(PROTOCOL_VERSION, SCHEMA)).expect("encode");
    assert_eq!(bytes[0], CHALLENGE_TAG);
    assert_eq!(bytes[1], PROTOCOL_VERSION);
}

#[derive(serde::Deserialize)]
#[allow(dead_code)]
enum OlderMessage {
    Challenge {
        v: u8,
        nonce_a: [u8; 32],
        endpoint_a: [u8; 32],
        list: ListRef,
    },
}

#[test]
fn an_older_reader_decodes_a_challenge_with_a_schema_at_the_end() {
    let bytes = postcard::to_stdvec(&challenge(PROTOCOL_VERSION, SCHEMA)).expect("encode");
    let OlderMessage::Challenge { v, .. } = postcard::from_bytes(&bytes).expect("older decode");
    assert_eq!(v, PROTOCOL_VERSION);
}

#[tokio::test]
async fn a_challenge_of_another_version_is_named_before_it_is_decoded() {
    let (mut a, mut b) = duplex(1 << 10);
    // An older Challenge: version 1 and no schema, so it would not decode as a current one.
    let mut old = postcard::to_stdvec(&challenge(1, SCHEMA)).expect("encode");
    old.truncate(old.len() - 3);
    a.write_all(&(old.len() as u32).to_be_bytes())
        .await
        .expect("len");
    a.write_all(&old).await.expect("body");

    let error = expect_challenge(&mut b, HANDSHAKE_FRAME_LIMIT)
        .await
        .expect_err("another version");
    assert!(matches!(error, ChallengeError::Version(1)), "{error:?}");
}

#[tokio::test]
async fn a_current_challenge_is_read_whole() {
    let (mut a, mut b) = duplex(1 << 10);
    let sent = challenge(PROTOCOL_VERSION, SCHEMA);
    write_frame(&mut a, &sent, HANDSHAKE_FRAME_LIMIT)
        .await
        .expect("write");
    let read = expect_challenge(&mut b, HANDSHAKE_FRAME_LIMIT)
        .await
        .expect("read");
    assert_eq!(read, sent);
}
