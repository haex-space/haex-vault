//! Tests for the local-delivery / owner-sync command helpers.
//!
//! Only the pure, AppHandle-free helpers are covered here. Full behavioral
//! start/stop/force coverage of `owner_sync_start` / `owner_sync_stop` /
//! `owner_sync_force` (which need a live QUIC endpoint + AppHandle) is the
//! Task-8 capstone, not this unit file.

use super::peers_to_start;
use std::collections::HashSet;

/// Helper: build a `HashSet<String>` from string literals.
fn running(eps: &[&str]) -> HashSet<String> {
    eps.iter().map(|s| s.to_string()).collect()
}

/// Helper: build a `Vec<String>` from string literals.
fn discovered(eps: &[&str]) -> Vec<String> {
    eps.iter().map(|s| s.to_string()).collect()
}

#[test]
fn peers_to_start_returns_all_when_none_running() {
    let disc = discovered(&["a", "b", "c"]);
    let run = running(&[]);
    assert_eq!(peers_to_start(&disc, &run), vec!["a", "b", "c"]);
}

#[test]
fn peers_to_start_skips_already_running() {
    let disc = discovered(&["a", "b", "c"]);
    let run = running(&["b"]);
    // Only the not-yet-running peers come back, in discovery order.
    assert_eq!(peers_to_start(&disc, &run), vec!["a", "c"]);
}

#[test]
fn peers_to_start_empty_when_all_running() {
    let disc = discovered(&["a", "b"]);
    let run = running(&["a", "b"]);
    assert!(peers_to_start(&disc, &run).is_empty());
}

#[test]
fn peers_to_start_ignores_running_peers_not_discovered() {
    // A running peer that is no longer discovered must not appear in the
    // result (the result is a subset of `discovered`, never of `running`).
    let disc = discovered(&["a"]);
    let run = running(&["b", "c"]);
    assert_eq!(peers_to_start(&disc, &run), vec!["a"]);
}

#[test]
fn peers_to_start_empty_discovery_is_empty() {
    let disc = discovered(&[]);
    let run = running(&["a"]);
    assert!(peers_to_start(&disc, &run).is_empty());
}
