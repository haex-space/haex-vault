use super::*;

#[test]
fn derived_ids_are_stable_for_the_same_key() {
    assert_eq!(target_id("c1", "d1"), target_id("c1", "d1"));
    assert_eq!(link_id("d1", 7), link_id("d1", 7));
    assert_eq!(member_id("g1", "c1"), member_id("g1", "c1"));
    assert_eq!(issue_id("d1", "c1"), issue_id("d1", "c1"));
}

#[test]
fn derived_ids_differ_per_key_and_per_kind() {
    assert_ne!(target_id("c1", "d1"), target_id("c1", "d2"));
    assert_ne!(target_id("c1", "d1"), target_id("c2", "d1"));
    assert_ne!(link_id("d1", 7), link_id("d1", 8));
    assert_ne!(member_id("g1", "c1"), member_id("g2", "c1"));
    // The same parts under another kind of row give another id.
    assert_ne!(target_id("d1", "c1"), issue_id("d1", "c1"));
}

#[test]
fn derived_ids_are_uuids() {
    assert!(uuid::Uuid::parse_str(&target_id("c1", "d1")).is_ok());
}
