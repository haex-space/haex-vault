//! Invitations between two vaults (spec 057 T033, T035; FR-009–FR-019, SC-006, SC-009): a vault
//! `a` invites the contact `b`, which has two devices; the answers come from the test double of
//! the user, the answer of the prompt itself.

use std::time::{Duration, Instant};

use serde_json::{json, Value};

use super::*;
use crate::contacts::card::received;
use crate::contacts::model::{ContactInput, Head};
use crate::contacts::service::{Caller, ContactsService};
use crate::extensions::peers::invited::Answered;
use crate::extensions::peers::prompt::{INVITATION, INVITATION_GONE};
use crate::extensions::peers::state::Timing;
use crate::extensions::peers::test_scene::Scene;
use crate::extensions::peers::test_support::{self, probe, Admit, Vault, Window};
use crate::sync::keys;

fn statuses(window: &Window) -> Vec<Value> {
    window
        .frame_events(INVITE_STATUS)
        .into_iter()
        .map(|(_, _, data)| data)
        .collect()
}

fn opened(window: &Window) -> Vec<Value> {
    window
        .frame_events(CHANNEL_OPENED)
        .into_iter()
        .map(|(_, _, data)| data)
        .collect()
}

async fn invite_b(scene: &Scene, payload: Value) -> Uuid {
    invite(
        &scene.a.host.peers,
        scene.a.extension_id,
        probe(),
        &scene.b_on_a,
        &payload,
    )
    .await
    .expect("invite")
}

/// Waits until `vault` asks its user, and gives the invitation's id.
async fn asked(vault: &Vault) -> Uuid {
    vault
        .window
        .wait_until(|w| !w.window_events(INVITATION).is_empty())
        .await;
    let event = vault.window.window_events(INVITATION).remove(0);
    Uuid::parse_str(event["inviteId"].as_str().expect("an id")).expect("a uuid")
}

async fn gone(vault: &Vault) {
    vault
        .window
        .wait_until(|w| !w.window_events(INVITATION_GONE).is_empty())
        .await;
}

async fn first_status(a: &Vault) -> Value {
    a.window.wait_until(|w| !statuses(w).is_empty()).await;
    statuses(&a.window).remove(0)
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn the_first_acceptance_wins_and_the_other_device_is_withdrawn() {
    let scene = Scene::start().await;
    let (a, b1, b2) = (&scene.a, &scene.b1, &scene.b2);
    let id = invite_b(&scene, json!({ "mode": "duel" })).await;

    assert_eq!(asked(b1).await, id);
    assert_eq!(asked(b2).await, id);
    // Until the user accepted, no extension of either device heard of it (SC-006).
    assert_eq!(b1.window.all_frame_events(), 0);
    assert_eq!(b2.window.all_frame_events(), 0);

    assert_eq!(b1.host.peers.answer_invitation(id, true), Answered::Open);
    let status = first_status(a).await;
    b1.window.wait_until(|w| !opened(w).is_empty()).await;
    gone(b2).await;

    assert_eq!(status["status"], "accepted");
    assert_eq!(status["inviteId"], id.to_string());
    let channel = Uuid::parse_str(status["channelId"].as_str().expect("channel")).expect("uuid");
    assert!(a.host.peers.channel(a.extension_id, channel).is_ok());
    let event = opened(&b1.window).remove(0);
    assert_eq!(event["origin"], "invitation");
    assert_eq!(event["inviteId"], id.to_string());
    assert_eq!(event["payload"], json!({ "mode": "duel" }));
    let card = b1
        .db
        .read({
            let contact = scene.a_on_b.clone();
            move |q| received::stored_of_contact(q, &contact).map_err(Into::into)
        })
        .await
        .expect("read")
        .expect("a card");
    assert_eq!(event["from"]["name"], card.card.name);
    assert!(opened(&b2.window).is_empty());

    // The other prompt, answered late, finds nothing open (FR-017).
    assert_eq!(b2.host.peers.answer_invitation(id, true), Answered::Gone);
    assert_eq!(statuses(&a.window).len(), 1);
    scene.shutdown().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn two_acceptances_at_once_give_one_channel() {
    let scene = Scene::start().await;
    let (a, b1, b2) = (&scene.a, &scene.b1, &scene.b2);
    let id = invite_b(&scene, json!({})).await;
    asked(b1).await;
    asked(b2).await;

    // The first answer may finish the invitation and withdraw the other prompt before this
    // synchronous test call reaches it. Both results are valid; the assertions below verify that
    // exactly one device still gets a channel.
    assert!(matches!(
        b1.host.peers.answer_invitation(id, true),
        Answered::Open | Answered::Gone
    ));
    assert!(matches!(
        b2.host.peers.answer_invitation(id, true),
        Answered::Open | Answered::Gone
    ));
    let status = first_status(a).await;
    // Each device either got its channel or heard that its prompt is gone, the winner never both.
    for vault in [b1, b2] {
        vault
            .window
            .wait_until(|w| !opened(w).is_empty() || !w.window_events(INVITATION_GONE).is_empty())
            .await;
    }

    assert_eq!(status["status"], "accepted");
    assert_eq!(opened(&b1.window).len() + opened(&b2.window).len(), 1);
    assert_eq!(a.host.peers.remotes().len(), 1);
    assert_eq!(statuses(&a.window).len(), 1);
    scene.shutdown().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn declining_on_every_device_is_one_declined() {
    let scene = Scene::start().await;
    let id = invite_b(&scene, json!({})).await;
    asked(&scene.b1).await;
    asked(&scene.b2).await;

    assert_eq!(
        scene.b1.host.peers.answer_invitation(id, false),
        Answered::Open
    );
    assert!(
        statuses(&scene.a.window).is_empty(),
        "one device has not answered yet"
    );
    assert_eq!(
        scene.b2.host.peers.answer_invitation(id, false),
        Answered::Open
    );
    let status = first_status(&scene.a).await;

    assert_eq!(status["status"], "declined");
    assert!(status.get("channelId").is_none() && status.get("hint").is_none());
    assert!(scene.a.host.peers.remotes().is_empty());
    scene.shutdown().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_mix_of_declined_and_not_installed_is_declined() {
    let scene = Scene::start().await;
    scene.b2.set_enabled(false);
    let id = invite_b(&scene, json!({})).await;
    asked(&scene.b1).await;
    assert_eq!(
        scene.b1.host.peers.answer_invitation(id, false),
        Answered::Open
    );

    assert_eq!(first_status(&scene.a).await["status"], "declined");
    assert!(
        scene.b2.window.window_events(INVITATION).is_empty(),
        "never asked"
    );
    scene.shutdown().await;
}

/// FR-013: a device that has no use for the extension says so and shows nothing.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn an_extension_that_is_missing_disabled_or_denied_is_not_installed() {
    type Break = fn(&Vault);
    let ways: [(&str, Break); 3] = [
        ("missing", Vault::uninstall),
        ("disabled", |vault| vault.set_enabled(false)),
        ("denied", |vault| vault.grant_peers("denied")),
    ];
    for (way, break_it) in ways {
        let scene = Scene::start().await;
        break_it(&scene.b1);
        break_it(&scene.b2);
        invite_b(&scene, json!({})).await;

        assert_eq!(
            first_status(&scene.a).await["status"],
            "not-installed",
            "{way}"
        );
        assert!(
            scene.b1.window.window_events(INVITATION).is_empty(),
            "{way}"
        );
        assert!(
            scene.b2.window.window_events(INVITATION).is_empty(),
            "{way}"
        );
        assert_eq!(scene.b1.desktop.count(), 0, "{way}");
        scene.shutdown().await;
    }
}

/// FR-013, FR-018: an inviter the other side does not know, or blocked, shows nothing there, and
/// its extension hears "unreachable" without learning which.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn an_inviter_that_is_no_contact_or_blocked_is_unreachable_and_shows_nothing() {
    for blocked in [false, true] {
        let scene = Scene::start().await;
        for vault in [&scene.b1, &scene.b2] {
            let service = ContactsService::new(vault.db.clone());
            if blocked {
                service
                    .set_blocked(&Caller::User, keys::hex(&scene.a.vault()), true)
                    .await
                    .expect("block");
            } else {
                service
                    .delete(&Caller::User, &[], vec![scene.a_on_b.clone()])
                    .await
                    .expect("delete the contact");
            }
        }
        invite_b(&scene, json!({})).await;

        let status = first_status(&scene.a).await;
        assert_eq!(status["status"], "unreachable", "blocked: {blocked}");
        assert_eq!(status["hint"], "share_current_card");
        for vault in [&scene.b1, &scene.b2] {
            assert!(vault.window.window_events(INVITATION).is_empty());
            assert_eq!(vault.window.all_frame_events(), 0);
        }
        scene.shutdown().await;
    }
}

/// FR-011, FR-018: a contact whose devices are not found is unreachable, in the time of the dial.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_contact_that_is_not_reached_is_unreachable_within_fifteen_seconds() {
    let scene = Scene::start().await;
    // A vault that knows no address of `b`: only the card names its devices, with no relay.
    let stranger = Vault::start(Admit::AtOnce).await;
    let b_on_stranger = test_support::store_card(&scene.b1, &stranger).await;
    stranger.grant("contacts", "read", "granted");
    let started = Instant::now();
    invite(
        &stranger.host.peers,
        stranger.extension_id,
        probe(),
        &b_on_stranger,
        &json!({}),
    )
    .await
    .expect("invite");

    let status = first_status(&stranger).await;
    assert_eq!(status["status"], "unreachable");
    assert_eq!(status["hint"], "share_current_card");
    assert!(started.elapsed() < Duration::from_secs(15));
    stranger.shutdown().await;
    scene.shutdown().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn an_invitation_nobody_answers_expires_on_every_device() {
    let scene = Scene::start().await;
    scene.a.host.peers.set_timing(Timing {
        invite_lifetime: Duration::from_millis(400),
        ..Timing::default()
    });
    invite_b(&scene, json!({})).await;
    asked(&scene.b1).await;
    asked(&scene.b2).await;

    assert_eq!(first_status(&scene.a).await["status"], "expired");
    gone(&scene.b1).await;
    gone(&scene.b2).await;
    assert_eq!(statuses(&scene.a.window).len(), 1);
    scene.shutdown().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn cancelling_withdraws_everywhere_and_sends_no_status() {
    let scene = Scene::start().await;
    let id = invite_b(&scene, json!({})).await;
    asked(&scene.b1).await;
    asked(&scene.b2).await;

    // Another extension's id is no invitation of this one (FR-006).
    assert_eq!(
        cancel(&scene.a.host.peers, Uuid::new_v4(), id),
        Err(PeerError::InviteNotOpen)
    );
    cancel(&scene.a.host.peers, scene.a.extension_id, id).expect("cancel");
    gone(&scene.b1).await;
    gone(&scene.b2).await;

    assert_eq!(
        cancel(&scene.a.host.peers, scene.a.extension_id, id),
        Err(PeerError::InviteNotOpen)
    );
    assert_eq!(
        scene.b1.host.peers.answer_invitation(id, true),
        Answered::Gone
    );
    assert!(statuses(&scene.a.window).is_empty());
    scene.shutdown().await;
}

/// FR-008: everything an extension has open goes with it, the invitations it sent included.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn ending_an_extension_withdraws_its_invitations_without_a_status() {
    let scene = Scene::start().await;
    invite_b(&scene, json!({})).await;
    asked(&scene.b1).await;
    asked(&scene.b2).await;

    scene.a.host.peers.end_all(scene.a.extension_id);
    gone(&scene.b1).await;
    gone(&scene.b2).await;
    assert!(statuses(&scene.a.window).is_empty());
    assert!(scene.a.host.peers.extensions().is_empty());
    scene.shutdown().await;
}

/// The invited side drops what its extension was never asked: an extension stopping on the
/// invited device takes the prompt away.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn ending_the_invited_extension_takes_its_prompt_away() {
    let scene = Scene::start().await;
    let id = invite_b(&scene, json!({})).await;
    asked(&scene.b1).await;

    scene.b1.host.peers.end_all(scene.b1.extension_id);
    gone(&scene.b1).await;
    assert_eq!(
        scene.b1.host.peers.answer_invitation(id, true),
        Answered::Gone
    );
    scene.shutdown().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn an_invitation_that_is_too_large_or_not_json_is_refused() {
    let scene = Scene::start().await;
    let peers = &scene.a.host.peers;
    let large = json!("x".repeat(PAYLOAD_BYTES));
    assert_eq!(
        invite(peers, scene.a.extension_id, probe(), &scene.b_on_a, &large)
            .await
            .err(),
        Some(PeerError::TooLarge {
            limit: PAYLOAD_BYTES
        })
    );
    scene.shutdown().await;
}

/// T035, 051 FR-031: a contact outside the grant, a missing contact and one without a card are
/// the same answer.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_contact_outside_the_grant_or_without_a_card_is_unknown() {
    let scene = Scene::start().await;
    let a = &scene.a;
    let without_card = ContactsService::new(a.db.clone())
        .create(
            &Caller::User,
            &[],
            ContactInput {
                head: Head {
                    display_name: Some("Zed".to_owned()),
                    ..Head::default()
                },
                ..ContactInput::default()
            },
        )
        .await
        .expect("create")
        .id;
    for contact in [without_card.as_str(), "no-such-contact"] {
        let answer = invite(&a.host.peers, a.extension_id, probe(), contact, &json!({})).await;
        assert_eq!(answer.err(), Some(PeerError::ContactUnknown), "{contact}");
    }

    // Without the grant even the contact that has a card is unknown.
    let guest = Vault::start(Admit::AtOnce).await;
    let b_on_guest = test_support::store_card(&scene.b1, &guest).await;
    let answer = invite(
        &guest.host.peers,
        guest.extension_id,
        probe(),
        &b_on_guest,
        &json!({}),
    )
    .await;
    assert_eq!(answer.err(), Some(PeerError::ContactUnknown));
    guest.shutdown().await;
    scene.shutdown().await;
}

/// T035: the invited extension hears who invited it only when its own `contacts` permission
/// covers that contact (R12).
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn the_contact_is_named_only_where_the_invited_extension_may_read_contacts() {
    for granted in [false, true] {
        let scene = Scene::start().await;
        if granted {
            scene.b1.grant("contacts", "read", "granted");
        }
        let id = invite_b(&scene, json!({})).await;
        asked(&scene.b1).await;
        assert_eq!(
            scene.b1.host.peers.answer_invitation(id, true),
            Answered::Open
        );
        scene.b1.window.wait_until(|w| !opened(w).is_empty()).await;

        let from = &opened(&scene.b1.window)[0]["from"];
        assert!(from["name"].is_string());
        if granted {
            assert_eq!(from["contactId"], scene.a_on_b);
        } else {
            assert!(from.get("contactId").is_none());
        }
        scene.shutdown().await;
    }
}
