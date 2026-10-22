use tokio::io::AsyncWriteExt;

use super::*;
use crate::sync::wire::write_value;

fn extension() -> ExtensionRef {
    ExtensionRef {
        public_key: "ab".repeat(32),
        name: "haex-doom".to_owned(),
    }
}

fn every_message() -> Vec<Message> {
    vec![
        Message::Invite {
            v: VERSION,
            invite: [1; 16],
            extension: extension(),
            payload: br#"{"map":"e1m1"}"#.to_vec(),
            expires_at: 1_700_000_000_000,
        },
        Message::Join {
            v: VERSION,
            lobby: [2; 16],
            proof: [3; 32],
            extension: extension(),
            display_name: "Ana".to_owned(),
            payload: b"{}".to_vec(),
        },
        Message::JoinReply {
            v: VERSION,
            status: JoinStatus::NotAdmitted,
        },
        Message::InviteReply {
            v: VERSION,
            invite: [1; 16],
            status: InviteStatus::NotInstalled,
        },
        Message::InviteConfirm {
            v: VERSION,
            invite: [1; 16],
        },
        Message::Withdraw {
            v: VERSION,
            invite: [1; 16],
        },
        Message::Reliable {
            v: VERSION,
            data: vec![7; RELIABLE_BYTES],
        },
        Message::Close {
            v: VERSION,
            reason: CloseReason::Closed,
        },
    ]
}

#[tokio::test]
async fn every_message_reads_back_as_written() {
    for message in every_message() {
        let mut bytes = Vec::new();
        write(&mut bytes, &message, CHANNEL_FRAME_LIMIT)
            .await
            .expect("write");
        let read_back = read(&mut bytes.as_slice(), CHANNEL_FRAME_LIMIT)
            .await
            .expect("read")
            .expect("a message");
        assert_eq!(read_back, message);
    }
}

#[tokio::test]
async fn a_reliable_message_of_the_limit_fits_a_channel_frame_and_not_an_admission_frame() {
    let largest = Message::Reliable {
        v: VERSION,
        data: vec![0; RELIABLE_BYTES],
    };
    let mut bytes = Vec::new();
    write(&mut bytes, &largest, CHANNEL_FRAME_LIMIT)
        .await
        .expect("fits");
    assert!(matches!(
        write(&mut Vec::new(), &largest, ADMISSION_FRAME_LIMIT).await,
        Err(WireError::FrameTooLarge { .. })
    ));
}

/// A frame head over the limit ends the read before anything is allocated or awaited: the body
/// is never sent here, and the read does not wait for it.
#[tokio::test]
async fn an_oversized_frame_is_refused_from_its_head() {
    let (mut near, mut far) = tokio::io::duplex(64);
    let head = u32::try_from(CHANNEL_FRAME_LIMIT + 1).expect("fits u32");
    near.write_all(&head.to_be_bytes()).await.expect("head");
    let refused = read(&mut far, CHANNEL_FRAME_LIMIT).await;
    assert!(
        matches!(
            refused,
            Err(MessageError::Wire(WireError::FrameTooLarge { len, .. })) if len == CHANNEL_FRAME_LIMIT + 1
        ),
        "{refused:?}"
    );
}

#[tokio::test]
async fn another_version_is_refused() {
    let mut bytes = Vec::new();
    let later = Message::Close {
        v: VERSION + 1,
        reason: CloseReason::Closed,
    };
    write_value(&mut bytes, &later, CHANNEL_FRAME_LIMIT)
        .await
        .expect("write");
    assert!(matches!(
        read(&mut bytes.as_slice(), CHANNEL_FRAME_LIMIT).await,
        Err(MessageError::Version(v)) if v == VERSION + 1
    ));
}

#[tokio::test]
async fn a_frame_that_is_no_message_and_an_empty_stream_are_told_apart() {
    let mut garbage = Vec::new();
    garbage.extend_from_slice(&3u32.to_be_bytes());
    garbage.extend_from_slice(&[0xff, 0xff, 0xff]);
    assert!(matches!(
        read(&mut garbage.as_slice(), CHANNEL_FRAME_LIMIT).await,
        Err(MessageError::Wire(WireError::Codec(_)))
    ));
    let mut empty: &[u8] = &[];
    assert!(read(&mut empty, CHANNEL_FRAME_LIMIT)
        .await
        .expect("a clean end")
        .is_none());
}
