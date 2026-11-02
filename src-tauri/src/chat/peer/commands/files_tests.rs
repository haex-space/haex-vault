use super::{remaining_message_limit, MAX_MESSAGE_PLAINTEXT};

#[test]
fn one_message_cannot_exceed_the_aggregate_file_size_limit() {
    assert_eq!(
        remaining_message_limit(0).expect("empty selection"),
        MAX_MESSAGE_PLAINTEXT
    );
    assert_eq!(
        remaining_message_limit(MAX_MESSAGE_PLAINTEXT - 1).expect("one byte left"),
        1
    );
    assert_eq!(
        remaining_message_limit(MAX_MESSAGE_PLAINTEXT).expect("limit reached"),
        0
    );
    assert!(remaining_message_limit(MAX_MESSAGE_PLAINTEXT + 1).is_err());
}
