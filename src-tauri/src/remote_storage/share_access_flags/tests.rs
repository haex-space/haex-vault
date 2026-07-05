//! Tests for [`super`] — bitmap constants and [`has_flag`].

#![cfg(test)]

use super::{has_flag, DELETE, GET, LIST, PUT, READ_ONLY, READ_WRITE};

#[test]
fn read_only_has_list_and_get() {
    assert!(has_flag(READ_ONLY, LIST));
    assert!(has_flag(READ_ONLY, GET));
}

#[test]
fn read_only_does_not_have_put_or_delete() {
    assert!(!has_flag(READ_ONLY, PUT));
    assert!(!has_flag(READ_ONLY, DELETE));
}

#[test]
fn read_write_has_all_four_flags() {
    assert!(has_flag(READ_WRITE, LIST));
    assert!(has_flag(READ_WRITE, GET));
    assert!(has_flag(READ_WRITE, PUT));
    assert!(has_flag(READ_WRITE, DELETE));
}

#[test]
fn has_flag_true_only_when_all_bits_present() {
    // Query mask covers multiple bits; true only if all are set.
    let combined = LIST | PUT;
    assert!(has_flag(READ_WRITE, combined));
    assert!(!has_flag(READ_ONLY, combined)); // READ_ONLY lacks PUT
    assert!(!has_flag(GET, combined)); // GET alone lacks LIST and PUT
}

#[test]
fn has_flag_zero_mask_zero_flag() {
    // Edge case: querying flag=0 always returns true (0 & 0 == 0).
    assert!(has_flag(0, 0));
    assert!(has_flag(READ_WRITE, 0));
}

#[test]
fn constants_have_expected_bit_values() {
    assert_eq!(LIST, 0b0001);
    assert_eq!(GET, 0b0010);
    assert_eq!(PUT, 0b0100);
    assert_eq!(DELETE, 0b1000);
    assert_eq!(READ_ONLY, 0b0011);
    assert_eq!(READ_WRITE, 0b1111);
}
