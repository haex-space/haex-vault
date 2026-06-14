use super::*;
use crate::ucan::ValidatedUcan;
use std::collections::HashMap;

#[test]
fn connected_peer_carries_validated_ucan() {
    let mut capabilities = HashMap::new();
    capabilities.insert(
        "space-1".to_string(),
        crate::ucan::CapabilityLevel::Read,
    );
    let validated = ValidatedUcan {
        issuer: "did:key:zIssuer".into(),
        audience: "did:key:zPeer".into(),
        capabilities,
        expires_at: 9999999999,
    };
    let peer = ConnectedPeer {
        endpoint_id: "ep".into(),
        did: "did:key:zPeer".into(),
        label: None,
        claims: vec![],
        connected_at: "2026-06-12T00:00:00Z".into(),
        validated_ucan: Some(validated),
    };
    let cached = peer.validated_ucan.expect("validated_ucan present");
    assert_eq!(cached.audience, "did:key:zPeer");
    assert_eq!(cached.issuer, "did:key:zIssuer");
}
