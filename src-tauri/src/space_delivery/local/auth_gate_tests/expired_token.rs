use std::collections::HashMap;

use tokio::sync::RwLock;

use super::helpers::{assert_single_audit_row, authorize_default, empty_db, make_peer};
use crate::space_delivery::local::protocol::{Request, Response};
use crate::space_delivery::local::types::ConnectedPeer;
use crate::ucan::{CapabilitySet, ValidatedUcan};

#[tokio::test]
async fn rejects_request_with_expired_cached_ucan() {
    // Stage 3: `validate_token` enforced `exp` at Announce time, but the
    // cached `ValidatedUcan` rides along for the lifetime of the QUIC
    // connection. A session that started fresh and then outlived its UCAN
    // must be rejected on the next gated request — otherwise an expired
    // capability silently keeps granting access until the peer disconnects.
    //
    // Set `expires_at = 0` so the check is independent of the system clock:
    // any positive `now` will reject. The audience matches the connection
    // DID and the capability is sufficient, so the test isolates the
    // expiry stage — only `require_not_expired` can be the rejecting
    // layer.
    let (db, _hlc, log_sink) = empty_db();
    let expired_ucan = ValidatedUcan {
        issuer: "did:key:zIssuer".to_string(),
        audience: "did:key:zPeer".to_string(),
        capabilities: HashMap::from([(
            "SPACE".to_string(),
            CapabilitySet::builder().write(false).build(),
        )]),
        row_capabilities: HashMap::new(),
        expires_at: 0,
        root_did: "did:key:zRoot".to_string(),
    };
    let mut peers_map: HashMap<String, ConnectedPeer> = HashMap::new();
    peers_map.insert(
        "endpoint-id".to_string(),
        make_peer("endpoint-id", "did:key:zPeer", expired_ucan),
    );
    let peers = RwLock::new(peers_map);

    let request = Request::MlsUploadKeyPackages {
        space_id: "SPACE".into(),
        packages: vec![],
        pops: vec![],
    };

    let result = authorize_default(
        &request,
        "did:key:zPeer",
        "endpoint-id",
        &peers,
        &db,
        Some(&log_sink),
    )
    .await;

    match result {
        Err(Response::Error { message }) => assert!(
            message.to_lowercase().contains("expired"),
            "expected peer-facing expired-UCAN message, got: {message}"
        ),
        other => panic!("expected expired-UCAN reject, got {other:?}"),
    }

    assert_single_audit_row(&db, "warn", "AuthGate", &request, "cached UCAN expired");
}
