use super::*;

#[test]
fn bounded_reads_accept_the_limit_and_reject_the_next_byte() {
    assert_eq!(
        read_all_limited(std::io::Cursor::new(b"abc"), 3)
            .unwrap()
            .as_slice(),
        b"abc"
    );
    assert!(matches!(
        read_all_limited(std::io::Cursor::new(b"abcd"), 3),
        Err(HaexError::PasswordsImportFailed { reason }) if reason == "too_large"
    ));
}
