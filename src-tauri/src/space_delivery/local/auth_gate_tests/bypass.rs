use std::collections::HashMap;

use tokio::sync::RwLock;

use super::helpers::{authorize_default, empty_db, select_audit_logs};
use crate::space_delivery::local::protocol::Request;
use crate::space_delivery::local::types::ConnectedPeer;

#[tokio::test]
async fn bypasses_claim_invite_cleanly() {
    // Stage 1 bypass: ClaimInvite has its own invite-token authentication
    // mechanism (the claimer is not yet a member, so neither cache nor
    // membership lookups can authorise them). `Request::required_capability`
    // returns `None`, so authorize_request must short-circuit with
    // `Ok(None)` — even with an empty connected_peers map and an empty DB.
    let (db, hlc) = empty_db();
    let peers: RwLock<HashMap<String, ConnectedPeer>> = RwLock::new(HashMap::new());

    let request = Request::ClaimInvite {
        space_id: "SPACE".into(),
        token: "invite-token".into(),
        endpoint_id: "endpoint-id".into(),
        key_packages: vec![],
        label: None,
        public_key: None,
    };

    let result = authorize_default(
        &request,
        "did:key:zNewcomer",
        "endpoint-id",
        &peers,
        &db,
        &hlc,
    )
    .await;

    match result {
        Ok(None) => {}
        other => panic!("expected Ok(None) bypass, got {other:?}"),
    }

    assert!(
        select_audit_logs(&db).is_empty(),
        "bypass path must not write any audit row"
    );
}
