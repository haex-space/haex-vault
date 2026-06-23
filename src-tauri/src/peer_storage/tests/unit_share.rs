use std::collections::HashSet;

use crate::peer_storage::endpoint::{PeerState, SharedFolder};

#[test]
fn share_only_accessible_in_matching_space() {
    let mut state = PeerState::default();

    state.shares.insert(
        "share-1".to_string(),
        SharedFolder {
            name: "Photos".to_string(),
            local_path: String::from("/tmp/photos"),
            space_id: "space-1".to_string(),
        },
    );

    state.shares.insert(
        "share-2".to_string(),
        SharedFolder {
            name: "Docs".to_string(),
            local_path: String::from("/tmp/docs"),
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
            local_path: String::from("/tmp/secret"),
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
