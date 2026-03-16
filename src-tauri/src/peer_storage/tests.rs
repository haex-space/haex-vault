#[cfg(test)]
mod tests {
    use std::collections::{HashMap, HashSet};
    use std::path::PathBuf;

    use crate::peer_storage::endpoint::{PeerEndpoint, PeerState, SharedFolder};

    // =========================================================================
    // Unit tests: PeerState access control
    // =========================================================================

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

    // =========================================================================
    // Unit tests: Share space filtering
    // =========================================================================

    #[test]
    fn share_only_accessible_in_matching_space() {
        let mut state = PeerState::default();

        state.shares.insert(
            "share-1".to_string(),
            SharedFolder {
                name: "Photos".to_string(),
                local_path: PathBuf::from("/tmp/photos"),
                space_id: "space-1".to_string(),
            },
        );

        state.shares.insert(
            "share-2".to_string(),
            SharedFolder {
                name: "Docs".to_string(),
                local_path: PathBuf::from("/tmp/docs"),
                space_id: "space-2".to_string(),
            },
        );

        // Peer only has access to space-1
        let mut peer_spaces = HashSet::new();
        peer_spaces.insert("space-1".to_string());

        let accessible: Vec<_> = state
            .shares
            .values()
            .filter(|s| peer_spaces.contains(&s.space_id))
            .collect();

        assert_eq!(accessible.len(), 1);
        assert_eq!(accessible[0].name, "Photos");
    }

    #[test]
    fn share_inaccessible_without_space_membership() {
        let mut state = PeerState::default();

        state.shares.insert(
            "share-1".to_string(),
            SharedFolder {
                name: "Secret".to_string(),
                local_path: PathBuf::from("/tmp/secret"),
                space_id: "space-private".to_string(),
            },
        );

        // Peer has no space access
        let peer_spaces: HashSet<String> = HashSet::new();

        let accessible: Vec<_> = state
            .shares
            .values()
            .filter(|s| peer_spaces.contains(&s.space_id))
            .collect();

        assert_eq!(accessible.len(), 0);
    }

    // =========================================================================
    // Async unit tests: PeerEndpoint state management
    // =========================================================================

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
            PathBuf::from("/tmp/photos"),
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

        ep.add_share("s1".to_string(), "A".to_string(), PathBuf::from("/a"), "sp1".to_string()).await;
        ep.add_share("s2".to_string(), "B".to_string(), PathBuf::from("/b"), "sp1".to_string()).await;

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

}
