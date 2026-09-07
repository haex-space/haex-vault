//! Bridge tests preserving coverage from the pre-extraction
//! `crate::crdt::hlc` module (both `hlc.rs`'s inline `#[cfg(test)] mod
//! tests` and `hlc_node_tests.rs`).
//!
//! When vault's `src/crdt/hlc.rs` was replaced by `haex_crdt` the module
//! carried 21 unit tests. Most are covered upstream by haex_crdt v0.1.1
//! or are trivial uhlc-library-internal mechanics tests (see below).
//! The behavioral tests without upstream coverage are re-hosted here so
//! any future regression trips a vault failure too.
//!
//! Re-hosted here:
//!
//! - `extracts_node_id_suffix` — plain string-parse contract for
//!   `haex_crdt::hlc_node_id_suffix`.
//! - `handles_uhlc_leading_zero_stripping` — the critical corner case
//!   where uhlc serialises node ids via `format!("{:x}", u128)` and
//!   strips leading zeros. A naive full-32-hex string compare would
//!   miss small node values; the helper must canonicalise numerically.
//! - Four `compare_hlc_strings` behavioral cases — order-by-time,
//!   different-node-numeric-ordering, malformed-time fallback,
//!   malformed-node fallback.
//!
//! Dropped (with rationale):
//!
//! - `round_trips_via_real_uhlc`, `rejects_foreign_node` — covered
//!   transitively by `haex_crdt`'s `src/crdt/scanner/tests.rs`
//!   origin-filter tests.
//! - `test_timestamp_format`, `test_timestamp_parsing`,
//!   `test_timestamp_ordering`, `test_hlc_persistence`,
//!   `compare_treats_node_ids_numerically_not_lexically`,
//!   `compare_with_wide_node_ids_orders_numerically`,
//!   `advance_past_remote_{rejects_malformed_string,errors_when_uninitialized,ok_on_empty_string}`
//!   — covered directly by `haex_crdt/src/crdt/hlc.rs` inline tests.
//! - `test_timestamp_time_extraction`,
//!   `test_timestamp_difference_calculation`,
//!   `test_ntp64_nanosecond_precision`, `test_update_with_external_timestamp`
//!   — trivial uhlc-library-internal mechanics; not vault behavior.
//!
//! Once haex_crdt gains equivalent unit tests upstream for the six
//! re-hosted cases, this file can be deleted.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use haex_crdt::{
    compare_hlc_strings, device_uuid_to_hlc_node, hlc_is_from_node, hlc_node_id_suffix,
};

// ---------------------------------------------------------------------------
// hlc_node_id_suffix / device_uuid_to_hlc_node / hlc_is_from_node
// ---------------------------------------------------------------------------

#[test]
fn extracts_node_id_suffix() {
    assert_eq!(hlc_node_id_suffix("12345/abcdef"), Some("abcdef"));
    assert_eq!(hlc_node_id_suffix("nopes"), None);
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

// ---------------------------------------------------------------------------
// compare_hlc_strings — behavioral cases not covered by haex_crdt v0.1.1
// ---------------------------------------------------------------------------

#[test]
fn compare_orders_by_time_first() {
    let a = "5/abc";
    let b = "10/abc";
    assert_eq!(compare_hlc_strings(a, b), std::cmp::Ordering::Less);
    assert_eq!(compare_hlc_strings(b, a), std::cmp::Ordering::Greater);
}

#[test]
fn compare_with_different_node_ids_orders_numerically() {
    // Both have time=5; node 0x1 < node 0x2 numerically.
    assert_eq!(compare_hlc_strings("5/1", "5/2"), std::cmp::Ordering::Less);
    assert_eq!(
        compare_hlc_strings("5/2", "5/1"),
        std::cmp::Ordering::Greater
    );
}

#[test]
fn compare_malformed_time_falls_back_to_zero() {
    // Non-numeric time component falls back to 0, so the well-formed
    // side wins by being greater. The comparator is silent by design
    // (no logging) — see its doc comment.
    let malformed = "not-a-number/abc";
    let valid = "5/abc";
    assert_eq!(
        compare_hlc_strings(malformed, valid),
        std::cmp::Ordering::Less
    );
}

#[test]
fn compare_malformed_node_falls_back_to_zero() {
    // Non-hex node component falls back to 0, so the well-formed
    // node loses the tie-break.
    let malformed = "5/zzzz";
    let valid = "5/1";
    assert_eq!(
        compare_hlc_strings(malformed, valid),
        std::cmp::Ordering::Less
    );
}
