use super::tests::{handshake, run};
use super::*;
use crate::sync::test_support::Member;

#[tokio::test]
async fn a_signature_for_another_endpoint_is_refused() {
    let main = Member::genesis();
    let linked = Member::join(&main);
    main.add(&linked);

    // The connection claims to come from an endpoint the dialer did not sign for.
    let (at_main, _) = run(&main, &linked, main.local(), linked.local(), [9; 32]).await;

    assert!(matches!(
        at_main,
        Err(HandshakeError::Refused(RejectCode::BadSignature))
    ));
}

#[tokio::test]
async fn a_device_of_another_vault_is_refused() {
    let main = Member::genesis();
    let foreign = Member::genesis();

    let (at_main, at_foreign) = handshake(&main, &foreign).await;

    assert!(matches!(
        at_main,
        Err(HandshakeError::Refused(RejectCode::ForeignVault))
    ));
    assert!(matches!(
        at_foreign,
        Err(HandshakeError::RefusedByPeer(RejectCode::ForeignVault))
    ));
}

#[tokio::test]
async fn another_schema_version_is_refused() {
    let main = Member::genesis();
    let linked = Member::join(&main);
    main.add(&linked);
    let mut newer = linked.local();
    newer.schema.holzi_migration += 1;

    let (at_main, at_linked) =
        run(&main, &linked, main.local(), newer, linked.keys.endpoint_id).await;

    assert!(
        matches!(
            at_main,
            Err(HandshakeError::Halted {
                problem: Problem::IncompatibleVersion,
                device,
            }) if device == linked.keys.device_pubkey
        ),
        "{at_main:?}"
    );
    assert!(matches!(
        at_linked,
        Err(HandshakeError::RefusedByPeer(RejectCode::Incompatible))
    ));
}

#[tokio::test]
async fn the_schema_is_only_judged_after_the_peer_proved_its_key() {
    let main = Member::genesis();
    let linked = Member::join(&main);
    main.add(&linked);
    let mut newer = linked.local();
    newer.schema.crdt_trigger += 1;

    // The connection claims to come from an endpoint the dialer did not
    // sign for: nobody proved anything, so no device gets a problem.
    let (at_main, _) = run(&main, &linked, main.local(), newer, [9; 32]).await;

    assert!(matches!(
        at_main,
        Err(HandshakeError::Refused(RejectCode::BadSignature))
    ));
}

#[tokio::test]
async fn a_listed_key_speaking_from_another_endpoint_is_a_duplicate() {
    let main = Member::genesis();
    let linked = Member::join(&main);
    main.add(&linked);
    // A copy of the linked installation: same device key, other endpoint.
    let mut copy_keys = linked.keys.clone();
    copy_keys.endpoint_id = [5; 32];
    let copy = Local {
        keys: &copy_keys,
        vault: linked.vault,
        schema: local_schema(),
    };

    let (at_main, at_copy) = run(&main, &linked, main.local(), copy, [5; 32]).await;

    assert!(
        matches!(
            at_main,
            Err(HandshakeError::Halted {
                problem: Problem::Duplicate,
                device,
            }) if device == linked.keys.device_pubkey
        ),
        "{at_main:?}"
    );
    assert!(matches!(
        at_copy,
        Err(HandshakeError::RefusedByPeer(RejectCode::Duplicate))
    ));
}

#[tokio::test]
async fn a_copy_of_the_accepting_device_is_a_duplicate() {
    let main = Member::genesis();
    let linked = Member::join(&main);
    main.add(&linked);
    linked.device.pull_from(&main.device);
    // Dials with main's own device key from another endpoint.
    let mut copy_keys = main.keys.clone();
    copy_keys.endpoint_id = [6; 32];
    let copy = Local {
        keys: &copy_keys,
        vault: main.vault,
        schema: local_schema(),
    };

    let (at_main, _) = run(&main, &linked, main.local(), copy, [6; 32]).await;

    assert!(
        matches!(
            at_main,
            Err(HandshakeError::Halted {
                problem: Problem::Duplicate,
                device,
            }) if device == main.keys.device_pubkey
        ),
        "{at_main:?}"
    );
}

#[test]
fn the_schema_names_this_builds_migrations_and_triggers() {
    let schema = local_schema();
    assert_eq!(schema.protocol, u32::from(PROTOCOL_VERSION));
    assert!(schema.holzi_migration >= 21);
    assert_eq!(schema.crdt_trigger, HOLZI_TRIGGER_VERSION as u32);
}
