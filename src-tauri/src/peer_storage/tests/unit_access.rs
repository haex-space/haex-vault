use std::collections::{HashMap, HashSet};

use crate::peer_storage::endpoint::PeerState;

#[test]
fn peer_state_default_denies_all() {
    let state = PeerState::default();
    assert!(state.allowed_peers.is_empty());
    assert!(state.allowed_peers.get("any-peer-id").is_none());
}

#[test]
fn peer_state_allows_registered_peer() {
    let mut state = PeerState::default();
    let mut spaces = HashSet::new();
    spaces.insert("space-1".to_string());
    state.allowed_peers.insert("peer-abc".to_string(), spaces);

    let allowed = state.allowed_peers.get("peer-abc");
    assert!(allowed.is_some());
    assert!(allowed.unwrap().contains("space-1"));
}

#[test]
fn peer_state_denies_unregistered_peer() {
    let mut state = PeerState::default();
    let mut spaces = HashSet::new();
    spaces.insert("space-1".to_string());
    state.allowed_peers.insert("peer-abc".to_string(), spaces);

    assert!(state.allowed_peers.get("peer-xyz").is_none());
}

#[test]
fn peer_state_revoke_removes_access() {
    let mut state = PeerState::default();
    let mut spaces = HashSet::new();
    spaces.insert("space-1".to_string());
    state.allowed_peers.insert("peer-abc".to_string(), spaces);

    assert!(state.allowed_peers.get("peer-abc").is_some());

    // Revoke: update with empty map (simulates reload after device removal)
    state.allowed_peers = HashMap::new();

    assert!(state.allowed_peers.get("peer-abc").is_none());
}

#[test]
fn peer_state_partial_revoke() {
    let mut state = PeerState::default();

    let mut spaces_a = HashSet::new();
    spaces_a.insert("space-1".to_string());
    state.allowed_peers.insert("peer-a".to_string(), spaces_a);

    let mut spaces_b = HashSet::new();
    spaces_b.insert("space-1".to_string());
    state.allowed_peers.insert("peer-b".to_string(), spaces_b);

    // Remove only peer-a
    let mut new_allowed = HashMap::new();
    let mut spaces_b_new = HashSet::new();
    spaces_b_new.insert("space-1".to_string());
    new_allowed.insert("peer-b".to_string(), spaces_b_new);
    state.allowed_peers = new_allowed;

    assert!(state.allowed_peers.get("peer-a").is_none());
    assert!(state.allowed_peers.get("peer-b").is_some());
}

#[test]
fn peer_state_multi_space_access() {
    let mut state = PeerState::default();
    let mut spaces = HashSet::new();
    spaces.insert("space-1".to_string());
    spaces.insert("space-2".to_string());
    state.allowed_peers.insert("peer-a".to_string(), spaces);

    let allowed = state.allowed_peers.get("peer-a").unwrap();
    assert!(allowed.contains("space-1"));
    assert!(allowed.contains("space-2"));
    assert!(!allowed.contains("space-3"));
}

#[test]
fn peer_state_empty_spaces_treated_as_denied() {
    let mut state = PeerState::default();
    state
        .allowed_peers
        .insert("peer-a".to_string(), HashSet::new());

    let spaces = state.allowed_peers.get("peer-a").unwrap();
    assert!(spaces.is_empty());
}
