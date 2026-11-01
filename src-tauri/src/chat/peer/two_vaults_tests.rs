//! Two vaults chat over a Nostr-Relay (spec 072 T027, US2 scenarios 1–5, 7–9, quickstart A3): the
//! receiver closed, both open, the sender offline, a recipient with two devices, no Nostr-Relay at
//! all, and a newer card that names a new device.

use super::messages::DeliveryState;
use super::test_support::{mock_relay, two_vaults, ChatDevice};

fn state_of(messages: &[super::messages::PeerMessage], text: &str) -> Option<DeliveryState> {
    messages
        .iter()
        .find(|m| m.text.as_deref() == Some(text))
        .and_then(|m| m.delivery)
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_message_to_a_closed_vault_arrives_when_it_opens_and_the_answer_comes_back() {
    let (_relay, url) = mock_relay().await;
    let (anna, ben) = two_vaults(&[url]).await;
    anna.start().await;

    anna.send(&ben, "Kommst du heute Abend?");
    anna.wait_for(&ben, "sent", |m| {
        state_of(m, "Kommst du heute Abend?") == Some(DeliveryState::Sent)
    })
    .await;
    assert!(ben.messages_with(&anna).is_empty(), "Ben's vault is closed");

    ben.start().await;
    let at_ben = ben
        .wait_for(&anna, "the message at Ben", |m| !m.is_empty())
        .await;
    assert_eq!(at_ben[0].text.as_deref(), Some("Kommst du heute Abend?"));
    assert!(!at_ben[0].own);
    anna.wait_for(&ben, "delivered", |m| {
        state_of(m, "Kommst du heute Abend?") == Some(DeliveryState::Delivered)
    })
    .await;

    ben.send(&anna, "Ja, gern!");
    anna.wait_for(&ben, "the answer", |m| {
        m.iter()
            .any(|m| m.text.as_deref() == Some("Ja, gern!") && !m.own)
    })
    .await;

    let (me, thread) = (ben.me.clone(), ben.thread_with(&anna));
    ben.db
        .write_blocking(move |tx| {
            let upto = super::messages::newest(tx, thread)?;
            Ok(super::send::read_up_to(
                tx,
                &me,
                thread,
                upto,
                super::delivery::now_ms(),
            )?)
        })
        .expect("read");
    ben.session.kick.notify_one();
    anna.wait_for(&ben, "read", |m| {
        state_of(m, "Kommst du heute Abend?") == Some(DeliveryState::Read)
    })
    .await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_sender_offline_queues_and_sends_when_it_comes_back() {
    let (_relay, url) = mock_relay().await;
    let (anna, ben) = two_vaults(&[url]).await;
    ben.start().await;

    anna.send(&ben, "aus dem Zug");
    assert_eq!(
        state_of(&anna.messages_with(&ben), "aus dem Zug"),
        Some(DeliveryState::Waiting)
    );
    anna.start().await;
    ben.wait_for(&anna, "the message", |m| !m.is_empty()).await;
    anna.wait_for(&ben, "delivered", |m| {
        state_of(m, "aus dem Zug") == Some(DeliveryState::Delivered)
    })
    .await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_recipient_with_two_devices_gets_it_on_both_once() {
    let (_relay, url) = mock_relay().await;
    let (anna, ben) = two_vaults(&[url]).await;
    let ben_phone = ben.link();
    anna.learn(&ben).await;
    for device in [&anna, &ben, &ben_phone] {
        device.start().await;
    }

    anna.send(&ben, "an beide");
    for device in [&ben, &ben_phone] {
        let found = device
            .wait_for(&anna, "the message", |m| !m.is_empty())
            .await;
        assert_eq!(found.len(), 1, "one row per device");
    }
    // The own-device sync brings the two rows together on one id.
    ben.member.device.pull_from(&ben_phone.member.device);
    assert_eq!(ben.messages_with(&anna).len(), 1);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn without_any_nostr_relay_the_message_fails_with_the_reason() {
    let anna = ChatDevice::genesis(&[]);
    let ben = ChatDevice::genesis(&[]);
    anna.learn(&ben).await;
    anna.start().await;
    anna.send(&ben, "ins Leere");
    let found = anna
        .wait_for(&ben, "failed", |m| {
            state_of(m, "ins Leere") == Some(DeliveryState::Failed)
        })
        .await;
    assert_eq!(found[0].failed_reason.as_deref(), Some("noRelay"));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_card_without_nostr_relays_is_reached_over_the_own_ones() {
    let (_relay, url) = mock_relay().await;
    let anna = ChatDevice::genesis(std::slice::from_ref(&url));
    // Ben listens on the same Nostr-Relay but his card names none (US2 scenario 8).
    let ben = ChatDevice::genesis(&[url]);
    let mut bare = ben.card();
    bare.card.nostr_relays.clear();
    let secret = ben
        .db
        .read_blocking(|r| crate::sync::keys::vault_secret(r))
        .expect("read")
        .expect("main device");
    let bare = crate::contacts::card::format::sign(bare.card, &secret)
        .expect("sign")
        .0;
    anna.learn_card(bare).await;
    ben.learn(&anna).await;
    anna.start().await;
    ben.start().await;
    anna.send(&ben, "über meine Nostr-Relays");
    ben.wait_for(&anna, "the message", |m| !m.is_empty()).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_newer_card_in_a_message_brings_the_new_device_of_the_sender() {
    let (_relay, url) = mock_relay().await;
    let (anna, ben) = two_vaults(&[url]).await;
    // Ben links a phone; Anna has only his old card with one device.
    let ben_phone = ben.link();
    for device in [&anna, &ben, &ben_phone] {
        device.start().await;
    }
    ben.send(&anna, "neues Telefon");
    anna.wait_for(&ben, "Ben's message", |m| !m.is_empty())
        .await;

    anna.send(&ben, "angekommen");
    ben_phone
        .wait_for(&anna, "the answer on the new device", |m| {
            m.iter().any(|m| m.text.as_deref() == Some("angekommen"))
        })
        .await;
}
