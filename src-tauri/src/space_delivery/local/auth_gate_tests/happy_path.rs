use std::collections::HashMap;

use tokio::sync::RwLock;

use super::helpers::{authorize_default, make_peer, select_audit_logs};
use crate::space_delivery::local::protocol::Request;
use crate::space_delivery::local::test_support::{
    insert_identity, insert_member, make_ucan_with_set, setup_membership_db,
};
use crate::space_delivery::local::types::ConnectedPeer;
use crate::ucan::{Cap, CapabilitySet};

#[tokio::test]
async fn accepts_valid_request_from_active_member() {
    // Happy path: cached UCAN's audience == connection DID, capability
    // satisfies the request floor, and the member is in
    // haex_space_members for the target space. The gate returns
    // `Ok(Some(validated))` so the dispatch site can use the UCAN for
    // origin attribution (`authored_by_did`).
    let (db, _hlc, log_sink) = setup_membership_db();
    insert_identity(&db, "id-peer", "did:key:zPeer");
    insert_member(&db, "mem-peer", "SPACE", "id-peer", "write");

    // The seeded member has role='write' — the actual issuer under W4 PR-3
    // grants `Read` + `Write` together (the two are orthogonal, and a write
    // member is expected to be able to read too). Building the set with
    // both caps mirrors that issuance shape and keeps the test isolating
    // membership/audience rather than the cap floor.
    let set = CapabilitySet::builder().read(false).write(false).build();
    let mut peers_map: HashMap<String, ConnectedPeer> = HashMap::new();
    peers_map.insert(
        "endpoint-id".to_string(),
        make_peer(
            "endpoint-id",
            "did:key:zPeer",
            make_ucan_with_set("did:key:zPeer", "SPACE", set),
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
        Ok(Some(validated)) => {
            assert_eq!(validated.audience, "did:key:zPeer");
            let set = validated
                .capabilities
                .get("SPACE")
                .expect("validated UCAN should carry a CapabilitySet for SPACE");
            assert!(
                set.can(Cap::Write),
                "validated UCAN should hold Cap::Write for SPACE, got {set:?}",
            );
        }
        other => panic!("expected Ok(Some(_)) for active member, got {other:?}"),
    }

    assert!(
        select_audit_logs(&db).is_empty(),
        "happy-path gate pass must not write any audit row"
    );
}
