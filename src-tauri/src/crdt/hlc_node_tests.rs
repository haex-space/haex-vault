//! Tests for the HLC node-id helpers in [`super::hlc`] used by the
//! scanner-origin filter (push ping-pong defence).
//!
//! These exercise the round-trip between a device UUID and the textual
//! node-id form that uhlc emits (`<u64>/<hex>`), including the leading-zero
//! corner case where uhlc strips zeros via `format!("{:x}", _)`.

#![cfg(test)]

use super::hlc::{
    device_uuid_to_hlc_node, hlc_is_from_node, hlc_node_id_suffix,
};
use uhlc::{HLCBuilder, ID};
use uuid::Uuid;

#[test]
fn extracts_node_id_suffix() {
    assert_eq!(hlc_node_id_suffix("12345/abcdef"), Some("abcdef"));
    assert_eq!(hlc_node_id_suffix("nopes"), None);
}

#[test]
fn round_trips_via_real_uhlc() {
    let uuid_str = "01020304-0506-0708-090a-0b0c0d0e0f10";
    let uuid = Uuid::parse_str(uuid_str).unwrap();
    let node_id = ID::try_from(*uuid.as_bytes()).unwrap();
    let hlc = HLCBuilder::new().with_id(node_id).build();
    let ts = hlc.new_timestamp().to_string();

    let our_node = device_uuid_to_hlc_node(uuid_str).expect("UUID parses");
    assert!(
        hlc_is_from_node(&ts, our_node),
        "round-trip should match: {ts} vs {our_node:032x}"
    );
}

#[test]
fn rejects_foreign_node() {
    let our_uuid = "01020304-0506-0708-090a-0b0c0d0e0f10";
    let other_uuid = "ffeeddcc-bbaa-9988-7766-554433221100";
    let other_id = ID::try_from(*Uuid::parse_str(other_uuid).unwrap().as_bytes()).unwrap();
    let other_hlc = HLCBuilder::new().with_id(other_id).build();
    let foreign_ts = other_hlc.new_timestamp().to_string();

    let our_node = device_uuid_to_hlc_node(our_uuid).unwrap();
    assert!(!hlc_is_from_node(&foreign_ts, our_node));
}

#[test]
fn handles_uhlc_leading_zero_stripping() {
    // uhlc serialises node-ids via `format!("{:x}", u128)` which strips
    // leading zeros. A naive string compare against the full 32-char UUID-hex
    // form would miss small node values; the helper must canonicalise
    // numerically.
    //
    // uhlc reads UUID bytes little-endian, so to land on u128 == 1 we put the
    // 0x01 at byte index 0 (the LSB) — i.e. the leading nibble of the UUID
    // string, since UUIDs print bytes in index order.
    let leading_zero_uuid = "01000000-0000-0000-0000-000000000000";
    let our_node = device_uuid_to_hlc_node(leading_zero_uuid).unwrap();
    assert_eq!(our_node, 1, "expected u128 value 1 for byte-0 = 0x01");
    assert!(hlc_is_from_node("12345/1", our_node));
    assert!(hlc_is_from_node(
        "12345/00000000000000000000000000000001",
        our_node
    ));
}
