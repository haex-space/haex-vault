use std::collections::HashMap;

use tokio::sync::RwLock;

use super::helpers::{assert_single_audit_row, authorize_default, empty_db};
use crate::space_delivery::local::protocol::{Request, Response};
use crate::space_delivery::local::types::{ConnectedPeer, PeerClaim};

#[tokio::test]
async fn rejects_request_without_prior_announce() {
    let (db, _hlc, log_sink) = empty_db();
    let peers: RwLock<HashMap<String, ConnectedPeer>> = RwLock::new(HashMap::new());

    let request = Request::MlsUploadKeyPackages {
        space_id: "SPACE".into(),
        packages: vec![],
    };

    let result =
        authorize_default(&request, "did:key:zPeer", "endpoint-id", &peers, &db, Some(&log_sink)).await;

    match result {
        Err(Response::Error { message }) => {
            assert!(message.contains("Announce"), "got: {message}")
        }
        other => panic!("expected reject, got {other:?}"),
    }

    assert_single_audit_row(&db, "warn", "AuthGate", &request, "no peer entry");
}

#[tokio::test]
async fn rejects_request_when_peer_announced_without_ucan() {
    // Stage 2b: the peer DOES have an entry in `connected_peers` (so it
    // didn't forge an endpoint-id), but `validated_ucan` is `None`. This
    // is the ClaimInvite-without-follow-up-Announce shape — the whole
    // reason `ConnectedPeer::validated_ucan` is `Option<ValidatedUcan>`
    // and the gate's `None` arm exists (see `auth_gate.rs:31-39`).
    // Silently treating `None` as a pass would defeat the entire gate.
    let (db, _hlc, log_sink) = empty_db();
    let peer = ConnectedPeer {
        endpoint_id: "endpoint-id".to_string(),
        did: "did:key:zPeer".to_string(),
        label: None,
        claims: Vec::<PeerClaim>::new(),
        connected_at: "2026-06-12T00:00:00Z".to_string(),
        validated_ucan: None,
    };
    let mut peers_map: HashMap<String, ConnectedPeer> = HashMap::new();
    peers_map.insert("endpoint-id".to_string(), peer);
    let peers = RwLock::new(peers_map);

    let request = Request::MlsUploadKeyPackages {
        space_id: "SPACE".into(),
        packages: vec![],
    };

    let result =
        authorize_default(&request, "did:key:zPeer", "endpoint-id", &peers, &db, Some(&log_sink)).await;

    match result {
        Err(Response::Error { message }) => {
            assert!(
                message.contains("Announce"),
                "expected reject mentioning Announce, got: {message}"
            );
        }
        other => panic!("expected reject, got {other:?}"),
    }

    assert_single_audit_row(&db, "warn", "AuthGate", &request, "no cached UCAN");
}
