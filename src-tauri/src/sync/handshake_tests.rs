use std::collections::BTreeSet;
use std::sync::{Arc, Mutex};

use tokio::io::{duplex, split};
use zeroize::Zeroizing;

use super::*;
use crate::sync::device_list::{DeviceList, ListedDevice, RemovedDevice};
use crate::sync::test_support::Member;

/// Runs `accept` on `a` and `dial` on `d` against each other, each seeing
/// the other's endpoint unless `d_claims` overrides what `a` sees.
pub(super) async fn run(
    a: &Member,
    d: &Member,
    a_local: Local<'_>,
    d_local: Local<'_>,
    endpoint_a_sees: [u8; 32],
) -> (Result<Peer, HandshakeError>, Result<Peer, HandshakeError>) {
    let noop = |_tables: BTreeSet<String>| {};
    run_with_callbacks(a, d, a_local, d_local, endpoint_a_sees, &noop, &noop).await
}

async fn run_with_callbacks(
    a: &Member,
    d: &Member,
    a_local: Local<'_>,
    d_local: Local<'_>,
    endpoint_a_sees: [u8; 32],
    a_applied: &(dyn Fn(BTreeSet<String>) + Send + Sync),
    d_applied: &(dyn Fn(BTreeSet<String>) + Send + Sync),
) -> (Result<Peer, HandshakeError>, Result<Peer, HandshakeError>) {
    let no_schema = |_schema: SchemaVersion| {};
    let (a_side, d_side) = duplex(1 << 20);
    let (mut a_recv, mut a_send) = split(a_side);
    let (mut d_recv, mut d_send) = split(d_side);
    tokio::join!(
        accept_with(
            &mut a_send,
            &mut a_recv,
            &a.device.replica,
            &a_local,
            endpoint_a_sees,
            a_applied,
            &no_schema,
        ),
        dial_with(
            &mut d_send,
            &mut d_recv,
            &d.device.replica,
            &d_local,
            a.keys.endpoint_id,
            d_applied,
            &no_schema,
        ),
    )
}

pub(super) async fn handshake(
    a: &Member,
    d: &Member,
) -> (Result<Peer, HandshakeError>, Result<Peer, HandshakeError>) {
    run(a, d, a.local(), d.local(), d.keys.endpoint_id).await
}

#[tokio::test]
async fn two_listed_devices_accept_each_other() {
    let main = Member::genesis();
    let linked = Member::join(&main);
    main.add(&linked);
    linked.device.pull_from(&main.device);

    let (at_main, at_linked) = handshake(&main, &linked).await;

    let peer = at_main.expect("main accepts");
    assert_eq!(peer.device_pubkey, linked.keys.device_pubkey);
    assert_eq!(peer.role, Role::Linked);
    assert_eq!(peer.vault_device_uuid, linked.device.db().device_id());
    let peer = at_linked.expect("linked accepts");
    assert_eq!(peer.device_pubkey, main.keys.device_pubkey);
    assert_eq!(peer.role, Role::Main);
}

#[tokio::test]
async fn the_newer_list_travels_in_the_handshake_both_ways() {
    let main = Member::genesis();
    let linked = Member::join(&main);
    main.add(&linked);
    // The linked device still holds generation 1, which does not name it.

    let (at_main, at_linked) = handshake(&main, &linked).await;
    assert!(
        at_main.is_ok() && at_linked.is_ok(),
        "{at_main:?} {at_linked:?}"
    );
    assert_eq!(at_linked.expect("peer").list.generation, 2);

    // And from the dialing side: the main device dials, the linked device
    // accepts while it holds the older list.
    let other = Member::join(&main);
    main.add(&other);
    let (at_other, at_main) = handshake(&other, &main).await;
    assert!(
        at_other.is_ok() && at_main.is_ok(),
        "{at_other:?} {at_main:?}"
    );
    assert_eq!(at_other.expect("peer").list.generation, 3);
}

#[tokio::test]
async fn a_device_list_learned_during_the_handshake_notifies_the_service() {
    let main = Member::genesis();
    let linked = Member::join(&main);
    main.add(&linked);
    let applied = Arc::new(Mutex::new(Vec::<BTreeSet<String>>::new()));
    let signal = Arc::clone(&applied);
    let on_linked_applied = move |tables: BTreeSet<String>| {
        signal.lock().expect("callback lock").push(tables);
    };
    let noop = |_tables: BTreeSet<String>| {};

    let (_, at_linked) = run_with_callbacks(
        &main,
        &linked,
        main.local(),
        linked.local(),
        linked.keys.endpoint_id,
        &noop,
        &on_linked_applied,
    )
    .await;
    at_linked.expect("handshake");

    assert!(applied
        .lock()
        .expect("callback lock")
        .iter()
        .any(|tables| tables.contains("device_lists")));
}

#[tokio::test]
async fn a_device_on_no_list_is_refused() {
    let main = Member::genesis();
    let stranger = Member::join(&main);

    let (at_main, at_stranger) = handshake(&main, &stranger).await;

    assert!(matches!(
        at_main,
        Err(HandshakeError::Refused(RejectCode::NotOnList))
    ));
    assert!(matches!(
        at_stranger,
        Err(HandshakeError::RefusedByPeer(RejectCode::NotOnList))
    ));
}

#[tokio::test]
async fn a_removed_device_stays_refused() {
    let main = Member::genesis();
    let linked = Member::join(&main);
    main.add(&linked);
    let removed = RemovedDevice {
        device_pubkey: linked.keys.device_pubkey,
        vault_device_uuid: linked.device.db().device_id(),
        limit_hlc: "0/0".to_string(),
        removed_at: 3,
    };
    main.issue_list(|mut list| {
        list.devices
            .retain(|d| d.device_pubkey != removed.device_pubkey);
        list.removed.push(removed);
        list
    });

    let (at_main, _) = handshake(&main, &linked).await;

    assert!(matches!(
        at_main,
        Err(HandshakeError::Refused(RejectCode::Removed))
    ));
}

#[tokio::test]
async fn a_device_refused_as_removed_still_learns_of_its_removal() {
    let main = Member::genesis();
    let linked = Member::join(&main);
    main.add(&linked);
    linked.device.pull_from(&main.device);
    let removed = RemovedDevice {
        device_pubkey: linked.keys.device_pubkey,
        vault_device_uuid: linked.device.db().device_id(),
        limit_hlc: "0/0".to_string(),
        removed_at: 3,
    };
    main.issue_list(|mut list| {
        list.devices
            .retain(|d| d.device_pubkey != removed.device_pubkey);
        list.removed.push(removed);
        list
    });
    let knows_removal = |member: &Member| {
        crate::storage::query::read(member.device.db(), |r| {
            let valid = device_list::valid_lists(&device_list::load_all(r)?, &member.vault);
            Ok(device_list::effective(&valid)
                .is_some_and(|s| s.list.removes(&linked.keys.device_pubkey)))
        })
        .expect("read list")
    };
    assert!(
        !knows_removal(&linked),
        "the linked device has not heard yet"
    );

    // The linked device dials the main device, which knows and refuses it.
    let (at_main, at_linked) = run(
        &main,
        &linked,
        main.local(),
        linked.local(),
        linked.keys.endpoint_id,
    )
    .await;

    assert!(matches!(
        at_main,
        Err(HandshakeError::Refused(RejectCode::Removed))
    ));
    assert!(matches!(
        at_linked,
        Err(HandshakeError::RefusedByPeer(RejectCode::Removed))
    ));
    assert!(
        knows_removal(&linked),
        "FR-034: the list pushed before the refusal is kept"
    );
}

#[test]
fn a_same_generation_tie_break_loser_does_not_remove_the_winner() {
    // Two main devices, A and B, each remove the other at generation 2 from
    // the same generation-1 base. Per FR-005/FR-043 the removal of the
    // losing (larger-hash) fork must not count: exactly one main device
    // stays admitted.
    let secret = [7u8; 32];
    let vault = signing::xonly_public_key(&secret).expect("pubkey");
    let a_pubkey = [1u8; 32];
    let a_endpoint = [2u8; 32];
    let a_uuid = Uuid::new_v4();
    let b_pubkey = [3u8; 32];
    let b_endpoint = [4u8; 32];
    let b_uuid = Uuid::new_v4();

    let main_device = |pubkey: [u8; 32], endpoint: [u8; 32], uuid: Uuid| ListedDevice {
        device_pubkey: pubkey,
        endpoint_id: endpoint,
        role: Role::Main,
        vault_device_uuid: uuid,
        name_sealed: Vec::new(),
        added_at: 0,
    };
    let genesis = DeviceList {
        vault,
        generation: 1,
        devices: vec![
            main_device(a_pubkey, a_endpoint, a_uuid),
            main_device(b_pubkey, b_endpoint, b_uuid),
        ],
        removed: Vec::new(),
        issued_by: a_pubkey,
        issued_at: 0,
        base_list_hash: None,
    };
    let genesis = device_list::sign_list(genesis, &secret).expect("sign genesis");

    let fork = |issued_by: [u8; 32], keeps, removes: [u8; 32], removes_uuid: Uuid| DeviceList {
        vault,
        generation: 2,
        devices: vec![keeps],
        removed: vec![RemovedDevice {
            device_pubkey: removes,
            vault_device_uuid: removes_uuid,
            limit_hlc: "0/0".to_string(),
            removed_at: 1,
        }],
        issued_by,
        issued_at: 1,
        base_list_hash: Some(genesis.hash),
    };
    let removes_b = device_list::sign_list(
        fork(
            a_pubkey,
            main_device(a_pubkey, a_endpoint, a_uuid),
            b_pubkey,
            b_uuid,
        ),
        &secret,
    )
    .expect("sign fork");
    let removes_a = device_list::sign_list(
        fork(
            b_pubkey,
            main_device(b_pubkey, b_endpoint, b_uuid),
            a_pubkey,
            a_uuid,
        ),
        &secret,
    )
    .expect("sign fork");

    let mut lists = BTreeMap::new();
    lists.insert(genesis.hash, genesis);
    lists.insert(removes_b.hash, removes_b.clone());
    lists.insert(removes_a.hash, removes_a.clone());

    // Whichever fork has the smaller hash wins (FR-043); its own device
    // stays admitted, regardless of what the losing fork claims.
    let (winner_pubkey, winner_endpoint, loser_pubkey) = if removes_b.hash < removes_a.hash {
        (a_pubkey, a_endpoint, b_pubkey)
    } else {
        (b_pubkey, b_endpoint, a_pubkey)
    };

    let observer_keys = DeviceKeys {
        device_secret: Zeroizing::new([9; 32]),
        device_pubkey: [9; 32],
        endpoint_secret: Zeroizing::new([9; 32]),
        endpoint_id: [9; 32],
    };
    let observer = Local {
        keys: &observer_keys,
        vault,
        schema: local_schema(),
    };

    let peer = admit(&lists, &winner_pubkey, &winner_endpoint, &observer)
        .expect("the tie-break winner stays admitted");
    assert_eq!(peer.device_pubkey, winner_pubkey);
    assert_ne!(winner_pubkey, loser_pubkey);
}

#[test]
fn two_keys_for_one_device_id_are_admitted_as_neither() {
    // Two main devices, A and B, each list a different key for one device id
    // from the same base: whichever list wins, its device shares that id with
    // a key of the fork, and neither may sync (FR-030).
    let secret = [7u8; 32];
    let vault = signing::xonly_public_key(&secret).expect("pubkey");
    let shared_uuid = Uuid::new_v4();
    let device = |pubkey: [u8; 32], endpoint: [u8; 32], role: Role, uuid: Uuid| ListedDevice {
        device_pubkey: pubkey,
        endpoint_id: endpoint,
        role,
        vault_device_uuid: uuid,
        name_sealed: Vec::new(),
        added_at: 0,
    };
    let main = device([1; 32], [2; 32], Role::Main, Uuid::new_v4());
    let genesis = device_list::sign_list(
        DeviceList {
            vault,
            generation: 1,
            devices: vec![main.clone()],
            removed: Vec::new(),
            issued_by: [1; 32],
            issued_at: 0,
            base_list_hash: None,
        },
        &secret,
    )
    .expect("sign genesis");
    let fork = |copy: ListedDevice| {
        device_list::sign_list(
            DeviceList {
                vault,
                generation: 2,
                devices: vec![main.clone(), copy],
                removed: Vec::new(),
                issued_by: [1; 32],
                issued_at: 1,
                base_list_hash: Some(genesis.hash),
            },
            &secret,
        )
        .expect("sign fork")
    };
    let first = fork(device([3; 32], [4; 32], Role::Linked, shared_uuid));
    let second = fork(device([5; 32], [6; 32], Role::Linked, shared_uuid));
    let mut lists = BTreeMap::new();
    lists.insert(genesis.hash, genesis);
    lists.insert(first.hash, first.clone());
    lists.insert(second.hash, second.clone());
    let winner = if first.hash < second.hash {
        &first
    } else {
        &second
    };
    let copy = winner.list.devices[1].clone();

    let observer_keys = DeviceKeys {
        device_secret: Zeroizing::new([9; 32]),
        device_pubkey: [9; 32],
        endpoint_secret: Zeroizing::new([9; 32]),
        endpoint_id: [9; 32],
    };
    let observer = Local {
        keys: &observer_keys,
        vault,
        schema: local_schema(),
    };

    let refused = admit(&lists, &copy.device_pubkey, &copy.endpoint_id, &observer);

    assert_eq!(
        refused.expect_err("a duplicate device id"),
        RejectCode::Duplicate
    );
}
