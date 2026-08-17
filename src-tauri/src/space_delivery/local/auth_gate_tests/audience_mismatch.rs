use std::collections::HashMap;

use tokio::sync::RwLock;

use super::helpers::{assert_single_audit_row, authorize_default, empty_db, make_peer};
use crate::space_delivery::local::protocol::{Request, Response};
use crate::space_delivery::local::test_support::make_ucan;
use crate::space_delivery::local::types::ConnectedPeer;
use crate::ucan::Cap;

#[tokio::test]
async fn rejects_audience_mismatch() {
    // Stage 4: a peer announces with a UCAN issued *to* someone else's DID
    // (e.g. a stolen-and-replayed token). The connection-authenticated DID
    // is `did:key:zPeer`, but the cached UCAN's audience is
    // `did:key:zSomeoneElse` — require_audience must reject.
    let (db, _hlc, log_sink) = empty_db();
    let mut peers_map: HashMap<String, ConnectedPeer> = HashMap::new();
    peers_map.insert(
        "endpoint-id".to_string(),
        make_peer(
            "endpoint-id",
            "did:key:zPeer",
            make_ucan("did:key:zSomeoneElse", "SPACE", Cap::Write),
        ),
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
            message.to_lowercase().contains("audience"),
            "expected peer-facing audience-mismatch message, got: {message}"
        ),
        other => panic!("expected audience-mismatch reject, got {other:?}"),
    }

    assert_single_audit_row(&db, "warn", "AuthGate", &request, "audience");
}
