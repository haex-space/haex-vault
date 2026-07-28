use super::limits::{check_value_bytes_len, MAX_VALUE_BYTES_LEN};

#[test]
fn max_value_bytes_len_is_10_mib() {
    assert_eq!(MAX_VALUE_BYTES_LEN, 10 * 1024 * 1024);
}

#[test]
fn check_accepts_at_limit() {
    assert!(check_value_bytes_len(MAX_VALUE_BYTES_LEN).is_ok());
}

#[test]
fn check_rejects_over_limit() {
    assert!(check_value_bytes_len(MAX_VALUE_BYTES_LEN + 1).is_err());
}
