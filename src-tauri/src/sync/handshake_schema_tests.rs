//! Both sides know the other's version state before either decides (spec 059 FR-005, SC-005),
//! and a device of another protocol version ends with a refusal, not a decode error (research R2).

use std::collections::BTreeSet;
use std::sync::Mutex;

use tokio::io::{duplex, split, AsyncWriteExt};

use super::tests::handshake;
use super::*;
use crate::sync::test_support::Member;
use crate::sync::wire::{read_frame, FRAME_LIMIT};

type Seen = Mutex<Vec<SchemaVersion>>;

fn noting(seen: &Seen) -> impl Fn(SchemaVersion) + Send + Sync + '_ {
    move |schema| seen.lock().expect("lock").push(schema)
}

fn seen(seen: &Seen) -> Vec<SchemaVersion> {
    seen.lock().expect("lock").clone()
}

/// Runs the two sides with callbacks for the version states, `a` accepting and `d` dialing.
async fn run_noting(
    a: &Member,
    d: &Member,
    a_local: Local<'_>,
    d_local: Local<'_>,
    at_a: &Seen,
    at_d: &Seen,
) -> (Result<Peer, HandshakeError>, Result<Peer, HandshakeError>) {
    let noop = |_tables: BTreeSet<String>| {};
    let (a_side, d_side) = duplex(1 << 20);
    let (mut a_recv, mut a_send) = split(a_side);
    let (mut d_recv, mut d_send) = split(d_side);
    let (on_a, on_d) = (noting(at_a), noting(at_d));
    tokio::join!(
        accept_with(
            &mut a_send,
            &mut a_recv,
            &a.device.replica,
            &a_local,
            d.keys.endpoint_id,
            &noop,
            &on_a,
        ),
        dial_with(
            &mut d_send,
            &mut d_recv,
            &d.device.replica,
            &d_local,
            a.keys.endpoint_id,
            &noop,
            &on_d,
        ),
    )
}

fn pair() -> (Member, Member) {
    let main = Member::genesis();
    let linked = Member::join(&main);
    main.add(&linked);
    linked.device.pull_from(&main.device);
    (main, linked)
}

#[tokio::test]
async fn both_sides_know_the_other_state_when_the_handshake_is_accepted() {
    let (main, linked) = pair();
    let (at_main, at_linked) = (Seen::default(), Seen::default());

    let (a, d) = run_noting(
        &main,
        &linked,
        main.local(),
        linked.local(),
        &at_main,
        &at_linked,
    )
    .await;

    assert!(a.is_ok() && d.is_ok(), "{a:?} / {d:?}");
    assert_eq!(seen(&at_main), [linked.local().schema]);
    assert_eq!(seen(&at_linked), [main.local().schema]);
}

#[tokio::test]
async fn both_sides_know_the_other_state_when_the_versions_differ_in_any_value() {
    type Bump = fn(&mut SchemaVersion);
    let bumps: [(&str, Bump); 3] = [
        ("protocol", |s| s.protocol += 1),
        ("holzi_migration", |s| s.holzi_migration += 1),
        ("crdt_trigger", |s| s.crdt_trigger += 1),
    ];
    for (name, bump) in bumps {
        let (main, linked) = pair();
        let mut other = linked.local();
        bump(&mut other.schema);
        let other_schema = other.schema;
        let (at_main, at_linked) = (Seen::default(), Seen::default());

        let (a, d) = run_noting(&main, &linked, main.local(), other, &at_main, &at_linked).await;

        assert!(
            matches!(
                a,
                Err(HandshakeError::Halted {
                    problem: Problem::IncompatibleVersion,
                    ..
                })
            ),
            "{name}: {a:?}"
        );
        assert!(
            matches!(
                d,
                Err(HandshakeError::RefusedByPeer(RejectCode::Incompatible))
            ),
            "{name}: {d:?}"
        );
        assert_eq!(
            seen(&at_main),
            [other_schema],
            "{name}: the acceptor knows the dialer"
        );
        assert_eq!(
            seen(&at_linked),
            [main.local().schema],
            "{name}: the dialer knows the acceptor"
        );
    }
}

#[tokio::test]
async fn nothing_is_noted_for_a_device_of_another_vault_or_a_bad_signature() {
    let main = Member::genesis();
    let foreign = Member::genesis();
    let at_main = Seen::default();
    let at_foreign = Seen::default();
    let (a, _) = run_noting(
        &main,
        &foreign,
        main.local(),
        foreign.local(),
        &at_main,
        &at_foreign,
    )
    .await;
    assert!(matches!(
        a,
        Err(HandshakeError::Refused(RejectCode::ForeignVault))
    ));
    assert!(
        seen(&at_main).is_empty(),
        "a foreign vault's state is not kept"
    );

    let (main, linked) = pair();
    let at_main = Seen::default();
    let none = Seen::default();
    // The connection claims to come from an endpoint the dialer did not sign for.
    let noop = |_tables: BTreeSet<String>| {};
    let (a_side, d_side) = duplex(1 << 20);
    let (mut a_recv, mut a_send) = split(a_side);
    let (mut d_recv, mut d_send) = split(d_side);
    let (on_a, on_d) = (noting(&at_main), noting(&none));
    let (main_local, linked_local) = (main.local(), linked.local());
    let (a, _) = tokio::join!(
        accept_with(
            &mut a_send,
            &mut a_recv,
            &main.device.replica,
            &main_local,
            [9; 32],
            &noop,
            &on_a,
        ),
        dial_with(
            &mut d_send,
            &mut d_recv,
            &linked.device.replica,
            &linked_local,
            main.keys.endpoint_id,
            &noop,
            &on_d,
        ),
    );
    assert!(matches!(
        a,
        Err(HandshakeError::Refused(RejectCode::BadSignature))
    ));
    assert!(
        seen(&at_main).is_empty(),
        "nobody proved a key, so nothing is kept"
    );
}

/// A dialer of protocol version 1 answers the new `Challenge` in the old way: `v = 1` and its own
/// state. The accepting side keeps that state and refuses it as incompatible.
#[tokio::test]
async fn an_older_dialer_is_noted_and_refused_as_incompatible() {
    let (main, linked) = pair();
    let at_main = Seen::default();
    let noop = |_tables: BTreeSet<String>| {};
    let on_a = noting(&at_main);
    let (a_side, d_side) = duplex(1 << 20);
    let (mut a_recv, mut a_send) = split(a_side);
    let (mut d_recv, mut d_send) = split(d_side);
    let old_schema = SchemaVersion {
        protocol: 1,
        holzi_migration: 39,
        crdt_trigger: 27,
    };

    let main_local = main.local();
    let accepting = accept_with(
        &mut a_send,
        &mut a_recv,
        &main.device.replica,
        &main_local,
        linked.keys.endpoint_id,
        &noop,
        &on_a,
    );
    let older = async {
        let Message::Challenge {
            nonce_a,
            list,
            endpoint_a,
            ..
        } = expect_frame(&mut d_recv, HANDSHAKE_FRAME_LIMIT)
            .await
            .expect("challenge")
        else {
            panic!("a Challenge first");
        };
        assert_eq!(endpoint_a, main.keys.endpoint_id);
        let nonce_d = [3u8; 32];
        let signed = transcript(
            &nonce_a,
            &nonce_d,
            &linked.keys.endpoint_id,
            &main.keys.endpoint_id,
            &linked.vault,
        );
        let response = Message::Response {
            v: 1,
            device_d: linked.keys.device_pubkey,
            vault: linked.vault,
            nonce_d,
            schema: old_schema,
            list,
            sig_d: Signature(sign(&linked.local(), &signed).expect("sign")),
        };
        write_frame(&mut d_send, &response, HANDSHAKE_FRAME_LIMIT)
            .await
            .expect("response");
        loop {
            match read_frame(&mut d_recv, FRAME_LIMIT).await.expect("read") {
                Some(Message::Reject { code }) => return code,
                Some(Message::DeviceListPush { .. }) => continue,
                other => panic!("expected a Reject, got {other:?}"),
            }
        }
    };
    let (a, code) = tokio::join!(accepting, older);

    assert_eq!(code, RejectCode::Incompatible);
    assert!(matches!(
        a,
        Err(HandshakeError::Halted {
            problem: Problem::IncompatibleVersion,
            ..
        })
    ));
    assert_eq!(seen(&at_main), [old_schema]);
}

/// An accepting side of protocol version 1 sends a `Challenge` without a state. The dialer names
/// the version and answers nothing: it cannot know the state, and no callback fires.
#[tokio::test]
async fn an_older_acceptor_is_refused_before_anything_is_answered() {
    let (_main, linked) = pair();
    let at_linked = Seen::default();
    let noop = |_tables: BTreeSet<String>| {};
    let on_d = noting(&at_linked);
    let (a_side, d_side) = duplex(1 << 20);
    let (a_recv, mut a_send) = split(a_side);
    let (mut d_recv, mut d_send) = split(d_side);
    let mut a_recv = a_recv;

    let older = Message::Challenge {
        v: 1,
        nonce_a: [1; 32],
        endpoint_a: [2; 32],
        list: ListRef {
            generation: 1,
            list_hash: [0; 32],
        },
        schema: SchemaVersion {
            protocol: 1,
            holzi_migration: 39,
            crdt_trigger: 27,
        },
    };
    let mut bytes = postcard::to_stdvec(&older).expect("encode");
    bytes.truncate(bytes.len() - 3); // no state in the old Challenge
    a_send
        .write_all(&(bytes.len() as u32).to_be_bytes())
        .await
        .expect("length");
    a_send.write_all(&bytes).await.expect("body");

    let dialed = dial_with(
        &mut d_send,
        &mut d_recv,
        &linked.device.replica,
        &linked.local(),
        [2; 32],
        &noop,
        &on_d,
    )
    .await;
    // Both halves of the dialer's side must go before the other side sees the end.
    drop(d_send);
    drop(d_recv);

    assert!(matches!(
        dialed,
        Err(HandshakeError::Refused(RejectCode::Incompatible))
    ));
    assert!(seen(&at_linked).is_empty());
    assert!(
        read_frame(&mut a_recv, FRAME_LIMIT)
            .await
            .expect("read")
            .is_none(),
        "nothing was answered"
    );
}

#[tokio::test]
async fn the_handshake_helper_still_accepts_two_equal_devices() {
    let (main, linked) = pair();
    let (a, d) = handshake(&main, &linked).await;
    assert!(a.is_ok() && d.is_ok());
}
