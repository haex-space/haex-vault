//! Tests for cancellation tokens used by the file serving protocol.

use super::{StreamKey, Streams};
use crate::files::share::FileShareLevel;

#[test]
fn cancelling_own_device_requests_only_cancels_the_current_generation() {
    let streams = Streams::default();
    let current = streams.own_device_token();

    streams.cancel_own_device();

    assert!(current.is_cancelled());
    assert!(!streams.own_device_token().is_cancelled());

    let contact = streams.token(StreamKey {
        share: "share".to_owned(),
        vault: [1; 32],
        level: FileShareLevel::Read,
    });
    streams.cancel_own_device();
    assert!(!contact.is_cancelled());
}
