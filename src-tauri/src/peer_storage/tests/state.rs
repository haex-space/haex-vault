use std::collections::{HashMap, HashSet};

use crate::peer_storage::endpoint::PeerEndpoint;

#[tokio::test]
async fn endpoint_set_allowed_peers_updates_state() {
    let ep = PeerEndpoint::new_ephemeral();

    let mut allowed = HashMap::new();
    let mut spaces = HashSet::new();
    spaces.insert("space-1".to_string());
    allowed.insert("peer-abc".to_string(), spaces);

    ep.set_allowed_peers(allowed).await;

    let state = ep.state.read().await;
    assert!(state.allowed_peers.contains_key("peer-abc"));
}

#[tokio::test]
async fn endpoint_revoke_clears_allowed_peers() {
    let ep = PeerEndpoint::new_ephemeral();

    let mut allowed = HashMap::new();
    let mut spaces = HashSet::new();
    spaces.insert("space-1".to_string());
    allowed.insert("peer-abc".to_string(), spaces);
    ep.set_allowed_peers(allowed).await;

    ep.set_allowed_peers(HashMap::new()).await;

    let state = ep.state.read().await;
    assert!(state.allowed_peers.is_empty());
}

#[tokio::test]
async fn endpoint_add_and_remove_share() {
    let ep = PeerEndpoint::new_ephemeral();

    ep.add_share(
        "s1".to_string(),
        "Photos".to_string(),
        "/tmp/photos".to_string(),
        "space-1".to_string(),
    )
    .await;

    assert_eq!(ep.list_shares().await.len(), 1);

    ep.remove_share("s1").await;
    assert_eq!(ep.list_shares().await.len(), 0);
}

#[tokio::test]
async fn endpoint_clear_shares() {
    let ep = PeerEndpoint::new_ephemeral();

    ep.add_share(
        "s1".to_string(),
        "A".to_string(),
        "/a".to_string(),
        "sp1".to_string(),
    )
    .await;
    ep.add_share(
        "s2".to_string(),
        "B".to_string(),
        "/b".to_string(),
        "sp1".to_string(),
    )
    .await;

    assert_eq!(ep.list_shares().await.len(), 2);
    ep.clear_shares().await;
    assert_eq!(ep.list_shares().await.len(), 0);
}

#[tokio::test]
async fn endpoint_rapid_peer_updates_final_state_correct() {
    let ep = PeerEndpoint::new_ephemeral();

    // Rapid grant/revoke
    for _ in 0..100 {
        let mut allowed = HashMap::new();
        let mut spaces = HashSet::new();
        spaces.insert("space-1".to_string());
        allowed.insert("peer-abc".to_string(), spaces);
        ep.set_allowed_peers(allowed).await;
        ep.set_allowed_peers(HashMap::new()).await;
    }

    // Final state: revoked
    let state = ep.state.read().await;
    assert!(state.allowed_peers.is_empty());
}

#[tokio::test]
async fn endpoint_revoke_one_keep_others() {
    let ep = PeerEndpoint::new_ephemeral();

    let mut allowed = HashMap::new();
    let mut spaces = HashSet::new();
    spaces.insert("space-1".to_string());
    allowed.insert("peer-evil".to_string(), spaces.clone());
    allowed.insert("peer-good".to_string(), spaces);
    ep.set_allowed_peers(allowed).await;

    // Revoke only evil peer
    let mut new_allowed = HashMap::new();
    let mut spaces = HashSet::new();
    spaces.insert("space-1".to_string());
    new_allowed.insert("peer-good".to_string(), spaces);
    ep.set_allowed_peers(new_allowed).await;

    let state = ep.state.read().await;
    assert!(state.allowed_peers.get("peer-evil").is_none());
    assert!(state.allowed_peers.get("peer-good").is_some());
}
