use super::*;

#[test]
fn android_volume_ids_are_recognised() {
    assert!(is_volume_id("1A2B-3C4D"));
    assert!(is_volume_id("abcd-ef01"));
    assert!(!is_volume_id("emulated"));
    assert!(!is_volume_id("self"));
    assert!(!is_volume_id("1A2B3C4D"));
}
