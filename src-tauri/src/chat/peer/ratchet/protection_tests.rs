//! The protection state of a conversation (spec 095 T040): each state of data-model.md from the
//! rows that produce it, the weakest over all devices of the conversation, none for the notes.

use uuid::Uuid;
use zeroize::Zeroizing;

use super::*;
use crate::chat::peer::delivery::now_ms;
use crate::chat::peer::outbox::{self, Rumor};
use crate::chat::peer::ratchet::store::{PeerRow, SessionRow};
use crate::chat::peer::store as threads;
use crate::chat::peer::test_support::ChatDevice;

const RELAY: &str = "wss://relay.example.org";
const DAY_MS: i64 = 24 * 60 * 60 * 1000;

/// Anna, her phone and her contact Ben, with their direct conversation.
async fn anna_phone_ben() -> (ChatDevice, ChatDevice, ChatDevice, Uuid) {
    let anna = ChatDevice::genesis(&[RELAY.to_owned()]);
    let phone = anna.link();
    let ben = ChatDevice::genesis(&[RELAY.to_owned()]);
    anna.learn(&ben).await;
    let (own, peer) = (anna.me.vault_hex.clone(), ben.me.vault_hex.clone());
    let thread = anna
        .db
        .write_blocking(move |tx| Ok(threads::open_direct(tx, &own, &peer, now_ms())?))
        .expect("conversation");
    (anna, phone, ben, thread)
}

fn protection(device: &ChatDevice, thread: Uuid, now: i64) -> Option<Protection> {
    let me = device.me.clone();
    device
        .db
        .read_blocking(move |r| Ok(of(r, &me, thread, now)?))
        .expect("protection")
}

/// Gives `device` a session with `peer` made at `created_at`, renewed at `renewed_at`. The pickle
/// is never read here.
fn session(device: &ChatDevice, peer: &ChatDevice, created_at: i64, renewed_at: Option<i64>) {
    session_with_id(
        device,
        peer,
        &Uuid::new_v4().to_string(),
        created_at,
        renewed_at,
    );
}

fn session_with_id(
    device: &ChatDevice,
    peer: &ChatDevice,
    session_id: &str,
    created_at: i64,
    renewed_at: Option<i64>,
) {
    let installation = device.me.installation;
    let row = SessionRow {
        peer_device: peer.me.device_hex(),
        session_id: session_id.to_owned(),
        session: Zeroizing::new(String::from("{}")),
        peer_identity_key: String::from("key"),
        created_at,
        last_received_at: None,
        last_sent_at: None,
        own_ratchet_key: None,
        own_key_since: None,
        renewed_at,
    };
    device
        .db
        .write_blocking(move |tx| Ok(store::put_session(tx, installation, &row)?))
        .expect("session");
}

fn recovering(device: &ChatDevice, peer: &ChatDevice, since: i64) {
    let (installation, peer) = (device.me.installation, peer.me.device_hex());
    let row = PeerRow {
        recovering_since: Some(since),
        ..PeerRow::default()
    };
    device
        .db
        .write_blocking(move |tx| Ok(store::put_peer(tx, installation, &peer, &row)?))
        .expect("peer");
}

fn queued_for(device: &ChatDevice, thread: Uuid, vault: &ChatDevice) {
    let pending = vec![vault.me.vault_hex.clone()];
    device
        .db
        .write_blocking(move |tx| {
            Ok(outbox::enqueue(
                tx,
                &Uuid::new_v4().to_string(),
                &thread.to_string(),
                Some(&Uuid::new_v4().to_string()),
                &Rumor {
                    kind: 24120,
                    content: String::from("{}"),
                },
                &pending,
                &pending,
                now_ms(),
            )?)
        })
        .expect("outbox");
}

fn accepted_for(device: &ChatDevice, thread: Uuid, vault: &ChatDevice) {
    let awaiting = vec![vault.me.vault_hex.clone()];
    device
        .db
        .write_blocking(move |tx| {
            Ok(outbox::enqueue(
                tx,
                &Uuid::new_v4().to_string(),
                &thread.to_string(),
                Some(&Uuid::new_v4().to_string()),
                &Rumor {
                    kind: 24120,
                    content: String::from("{}"),
                },
                &[],
                &awaiting,
                now_ms(),
            )?)
        })
        .expect("outbox");
}

fn state(device: &ChatDevice, thread: Uuid, now: i64) -> ProtectionState {
    protection(device, thread, now)
        .expect("a direct conversation has a protection")
        .state
}

#[tokio::test]
async fn protected_with_a_session_renewed_within_seven_days() {
    let (anna, phone, ben, thread) = anna_phone_ben().await;
    let now = now_ms();
    session(&anna, &ben, now - 30 * DAY_MS, Some(now - 6 * DAY_MS));
    session(&anna, &phone, now - DAY_MS, None);
    assert_eq!(
        protection(&anna, thread, now),
        Some(Protection {
            state: ProtectionState::Protected,
            renewed_at: Some(now - 6 * DAY_MS),
        })
    );
}

#[tokio::test]
async fn renewal_outstanding_after_more_than_seven_days() {
    let (anna, phone, ben, thread) = anna_phone_ben().await;
    let now = now_ms();
    session(&anna, &phone, now, None);
    session_with_id(
        &anna,
        &ben,
        "00000000-0000-4000-8000-000000000001",
        now - 8 * DAY_MS,
        None,
    );
    assert_eq!(
        state(&anna, thread, now),
        ProtectionState::RenewalOutstanding,
        "created more than seven days ago, never renewed"
    );
    session_with_id(
        &anna,
        &ben,
        "00000000-0000-4000-8000-000000000002",
        now - 30 * DAY_MS,
        Some(now - DAY_MS),
    );
    assert_eq!(
        protection(&anna, thread, now),
        Some(Protection {
            state: ProtectionState::RenewalOutstanding,
            renewed_at: Some(now - 8 * DAY_MS),
        }),
        "a newer session does not hide the session used for sending"
    );
}

#[tokio::test]
async fn awaiting_recipient_still_waits_for_keys_after_partial_acceptance() {
    let (anna, phone, ben, thread) = anna_phone_ben().await;
    let now = now_ms();
    session(&anna, &phone, now, None);
    accepted_for(&anna, thread, &ben);

    assert_eq!(state(&anna, thread, now), ProtectionState::WaitingForKeys);

    session(&anna, &ben, now, None);
    assert_eq!(state(&anna, thread, now), ProtectionState::Protected);
}

#[tokio::test]
async fn waiting_for_keys_only_while_an_event_waits_for_a_device_without_a_session() {
    let (anna, phone, ben, thread) = anna_phone_ben().await;
    let now = now_ms();
    session(&anna, &phone, now, None);
    assert_eq!(
        state(&anna, thread, now),
        ProtectionState::Protected,
        "nothing was sent to Ben's device yet"
    );
    queued_for(&anna, thread, &ben);
    assert_eq!(state(&anna, thread, now), ProtectionState::WaitingForKeys);
    session(&anna, &ben, now, None);
    assert_eq!(
        state(&anna, thread, now),
        ProtectionState::Protected,
        "a session with the device ends the wait"
    );
}

#[tokio::test]
async fn recovering_while_a_reset_is_open() {
    let (anna, phone, ben, thread) = anna_phone_ben().await;
    let now = now_ms();
    session(&anna, &phone, now, None);
    session(&anna, &ben, now, None);
    recovering(&anna, &ben, now);
    assert_eq!(state(&anna, thread, now), ProtectionState::Recovering);
}

#[tokio::test]
async fn the_weakest_device_of_the_conversation_wins() {
    let (anna, phone, ben, thread) = anna_phone_ben().await;
    let now = now_ms();
    session(&anna, &ben, now, None);
    session(&anna, &phone, now - 8 * DAY_MS, None);
    assert_eq!(
        protection(&anna, thread, now),
        Some(Protection {
            state: ProtectionState::RenewalOutstanding,
            renewed_at: Some(now - 8 * DAY_MS),
        }),
        "the own other device counts too"
    );
    recovering(&anna, &ben, now);
    assert_eq!(state(&anna, thread, now), ProtectionState::Recovering);
    queued_for(&anna, thread, &anna);
    session(&anna, &ben, now, None);
    let phone_hex = phone.me.device_hex();
    let installation = anna.me.installation;
    anna.db
        .write_blocking(move |tx| Ok(store::delete_sessions(tx, installation, &phone_hex)?))
        .expect("delete");
    assert_eq!(
        state(&anna, thread, now),
        ProtectionState::WaitingForKeys,
        "waiting for keys is the weakest"
    );
    assert!(
        ProtectionState::Protected < ProtectionState::RenewalOutstanding
            && ProtectionState::RenewalOutstanding < ProtectionState::Recovering
            && ProtectionState::Recovering < ProtectionState::WaitingForKeys
    );
}

#[tokio::test]
async fn the_notes_have_none() {
    let anna = ChatDevice::genesis(&[RELAY.to_owned()]);
    let own = anna.me.vault_hex.clone();
    let notes = anna
        .db
        .write_blocking(move |tx| Ok(threads::open_note(tx, &own, now_ms())?))
        .expect("notes");
    assert_eq!(protection(&anna, notes, now_ms()), None);
    assert_eq!(protection(&anna, Uuid::new_v4(), now_ms()), None);
}
