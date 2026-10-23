//! The invited side's own rules (spec 057 T038, FR-013, FR-016, research R10): who may invite,
//! how many invitations wait, and how the prompt goes when the other side leaves. The inviting
//! side is raw here, so each message is one the test chose.

use std::time::Duration;

use iroh::endpoint::{Connection, RecvStream, SendStream};
use serde_json::json;

use super::*;
use crate::extensions::peers::prompt::{INVITATION, INVITATION_GONE};
use crate::extensions::peers::state::Timing;
use crate::extensions::peers::test_scene::Scene;
use crate::extensions::peers::test_support::{probe, Vault};
use crate::sync::foreign_auth::Nonces;

type Raw = (Connection, SendStream, RecvStream);

/// Dials `to` as `from` and sends an `Invite` for the test extension with `payload`.
async fn raw_invite(from: &Vault, to: &Vault, invite: [u8; 16], payload: &[u8]) -> Raw {
    let (connection, mut send, recv, verified) = from.dial(to).await;
    verified.expect("the exchange worked");
    let message = Message::Invite {
        v: VERSION,
        invite,
        extension: probe(),
        payload: payload.to_vec(),
        expires_at: 0,
    };
    wire::write(&mut send, &message, ADMISSION_FRAME_LIMIT)
        .await
        .expect("send the invite");
    (connection, send, recv)
}

/// Whether the other side ended the connection without saying anything.
async fn cut_off(mut raw: Raw) -> bool {
    matches!(
        tokio::time::timeout(
            Duration::from_secs(20),
            wire::read(&mut raw.2, ADMISSION_FRAME_LIMIT)
        )
        .await,
        Ok(Ok(None) | Err(_))
    )
}

async fn asked_n(vault: &Vault, n: usize) {
    vault
        .window
        .wait_until(|w| w.window_events(INVITATION).len() >= n)
        .await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn an_invitation_of_an_own_device_is_cut_off_without_a_prompt() {
    let scene = Scene::start().await;
    // `b2` is a listed device of the vault of `b1`: no contact, so no invitation.
    scene.b2.learn(&scene.b1).await;
    let raw = raw_invite(&scene.b2, &scene.b1, [1; 16], b"{}").await;

    assert!(cut_off(raw).await);
    assert!(scene.b1.window.window_events(INVITATION).is_empty());
    scene.shutdown().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_payload_that_is_not_json_or_too_large_is_cut_off() {
    let scene = Scene::start().await;
    let large = vec![b' '; PAYLOAD_BYTES + 1];
    for payload in [&b"not json"[..], &large] {
        let raw = raw_invite(&scene.a, &scene.b1, [2; 16], payload).await;
        assert!(cut_off(raw).await);
    }
    assert!(scene.b1.window.window_events(INVITATION).is_empty());
    scene.shutdown().await;
}

/// R10: four invitations of one vault wait at once; one more is dropped without a prompt.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn only_four_invitations_of_one_vault_wait_at_once() {
    let scene = Scene::start().await;
    let mut kept = Vec::new();
    for id in 1..=4u8 {
        kept.push(raw_invite(&scene.a, &scene.b1, [id; 16], b"{}").await);
        asked_n(&scene.b1, usize::from(id)).await;
    }
    let fifth = raw_invite(&scene.a, &scene.b1, [5; 16], b"{}").await;
    assert!(cut_off(fifth).await);

    assert_eq!(scene.b1.window.window_events(INVITATION).len(), 4);
    scene.shutdown().await;
}

/// A replayed id, while the first of it waits, is dropped without a second prompt.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn an_id_that_waits_already_is_not_asked_twice() {
    let scene = Scene::start().await;
    let _first = raw_invite(&scene.a, &scene.b1, [3; 16], b"{}").await;
    asked_n(&scene.b1, 1).await;
    let again = raw_invite(&scene.a, &scene.b1, [3; 16], b"{}").await;
    assert!(cut_off(again).await);

    assert_eq!(scene.b1.window.window_events(INVITATION).len(), 1);
    scene.shutdown().await;
}

#[test]
fn sixteen_invitations_wait_in_all() {
    let remote = |vault: u8| Verified {
        device_pubkey: [0; 32],
        vault: [vault; 32],
        endpoint_id: [0; 32],
        trust: Trust::Unverified,
        nonces: Nonces {
            dial: [0; 32],
            accept: [0; 32],
        },
    };
    let incoming = |vault: u8| Incoming {
        extension_id: Uuid::nil(),
        remote: remote(vault),
        answer: None,
        end: CancellationToken::new(),
    };
    let mut open = Invitations::default();
    // Four vaults with four each fill the 16 places; a fifth vault finds none.
    for vault in 0..4u8 {
        for _ in 0..INCOMING_PER_VAULT {
            assert!(open.register(Uuid::new_v4(), incoming(vault)));
        }
    }
    assert!(!open.register(Uuid::new_v4(), incoming(9)));
}

/// The prompt goes when its inviter's connection ends, and an answer after that finds nothing.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn the_prompt_goes_when_the_inviting_side_leaves() {
    let scene = Scene::start().await;
    let (connection, _send, _recv) = raw_invite(&scene.a, &scene.b1, [7; 16], b"{}").await;
    asked_n(&scene.b1, 1).await;

    connection.close(0u32.into(), b"");
    scene
        .b1
        .window
        .wait_until(|w| !w.window_events(INVITATION_GONE).is_empty())
        .await;
    let event = scene.b1.window.window_events(INVITATION_GONE).remove(0);
    assert_eq!(event["reason"], "withdrawn");
    assert_eq!(
        scene
            .b1
            .host
            .peers
            .answer_invitation(Uuid::from_bytes([7; 16]), true),
        Answered::Gone
    );
    scene.shutdown().await;
}

/// R10: an invitation waits [`INVITE_LIFETIME`] here, whatever the inviting side says.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn an_invitation_nobody_answers_expires_in_the_prompt_too() {
    let scene = Scene::start().await;
    scene.b1.host.peers.set_timing(Timing {
        invite_lifetime: Duration::from_millis(300),
        ..Timing::default()
    });
    let _raw = raw_invite(&scene.a, &scene.b1, [8; 16], b"{}").await;

    scene
        .b1
        .window
        .wait_until(|w| !w.window_events(INVITATION_GONE).is_empty())
        .await;
    let event = scene.b1.window.window_events(INVITATION_GONE).remove(0);
    assert_eq!(event["reason"], "expired");
    scene.shutdown().await;
}

/// An answer to an invitation that was answered already is "gone", never a second answer.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn an_invitation_takes_one_answer() {
    let scene = Scene::start().await;
    let _raw = raw_invite(&scene.a, &scene.b1, [9; 16], b"{}").await;
    asked_n(&scene.b1, 1).await;
    let id = Uuid::from_bytes([9; 16]);

    assert_eq!(
        scene.b1.host.peers.answer_invitation(id, false),
        Answered::Open
    );
    assert_eq!(
        scene.b1.host.peers.answer_invitation(id, true),
        Answered::Gone
    );
    assert_eq!(
        scene.b1.host.peers.answer_invitation(Uuid::new_v4(), true),
        Answered::Gone
    );
    scene.shutdown().await;
}

/// A blocked inviter loses the prompt it has open (FR-034, R12).
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn blocking_the_inviter_takes_its_open_prompt_away() {
    let scene = Scene::start().await;
    let _raw = raw_invite(
        &scene.a,
        &scene.b1,
        [4; 16],
        json!({}).to_string().as_bytes(),
    )
    .await;
    asked_n(&scene.b1, 1).await;

    ContactsService::new(scene.b1.db.clone())
        .set_blocked(
            &Caller::User,
            crate::sync::keys::hex(&scene.a.vault()),
            true,
        )
        .await
        .expect("block");
    super::super::watch::recheck(
        &scene.b1.db,
        &scene.b1.host,
        scene.b1.db.device_id(),
        super::super::watch::Recheck::ALL,
    )
    .await;

    scene
        .b1
        .window
        .wait_until(|w| !w.window_events(INVITATION_GONE).is_empty())
        .await;
    assert!(scene.b1.host.peers.invitation_remotes().is_empty());
    scene.shutdown().await;
}
