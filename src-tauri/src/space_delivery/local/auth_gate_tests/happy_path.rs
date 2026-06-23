use std::collections::HashMap;

use tokio::sync::RwLock;

use super::helpers::{authorize_default, make_peer, select_audit_logs};
use crate::space_delivery::local::protocol::Request;
use crate::space_delivery::local::test_support::{
    insert_identity, insert_member, make_ucan, setup_membership_db,
};
use crate::space_delivery::local::types::ConnectedPeer;
use crate::ucan::CapabilityLevel;

#[tokio::test]
async fn accepts_valid_request_from_active_member() {
    // Happy path: cached UCAN's audience == connection DID, capability
    // satisfies the request floor, and the member is in
    // haex_space_members for the target space. The gate returns
    // `Ok(Some(validated))` so the dispatch site can use the UCAN for
    // origin attribution (`authored_by_did`).
    let (db, hlc) = setup_membership_db();
    insert_identity(&db, "id-peer", "did:key:zPeer");
    insert_member(&db, "mem-peer", "SPACE", "id-peer", "write");

    let mut peers_map: HashMap<String, ConnectedPeer> = HashMap::new();
    peers_map.insert(
        "endpoint-id".to_string(),
        make_peer(
            "endpoint-id",
            "did:key:zPeer",
            make_ucan("did:key:zPeer", "SPACE", CapabilityLevel::Write),
        ),
    );
    let peers = RwLock::new(peers_map);

    let request = Request::MlsUploadKeyPackages {
        space_id: "SPACE".into(),
        packages: vec![],
    };

    let result =
        authorize_default(&request, "did:key:zPeer", "endpoint-id", &peers, &db, &hlc).await;

    match result {
        Ok(Some(validated)) => {
            assert_eq!(validated.audience, "did:key:zPeer");
            assert_eq!(
                validated.capabilities.get("SPACE"),
                Some(&CapabilityLevel::Write),
                "validated UCAN should carry the Write capability for SPACE",
            );
        }
        other => panic!("expected Ok(Some(_)) for active member, got {other:?}"),
    }

    assert!(
        select_audit_logs(&db).is_empty(),
        "happy-path gate pass must not write any audit row"
    );
}
