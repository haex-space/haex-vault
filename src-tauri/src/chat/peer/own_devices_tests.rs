//! Own devices that are never online together end with the same conversation (spec 072 T028,
//! FR-016, US2 scenario 6, quickstart A4): every rumor is also wrapped to the own other devices,
//! since spec 095 on the session with each of them, opened from its chat key record.

use super::test_support::{mock_relay, two_vaults, ChatDevice};
use super::{ids, messages, outbox, send, store};

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn two_own_devices_never_online_together_hold_the_same_rows() {
    let (_relay, url) = mock_relay().await;
    let (anna, ben) = two_vaults(&[url]).await;
    let anna_phone = anna.link();
    ben.learn(&anna).await;
    // The phone was online once, so its chat key record is on the relay.
    anna_phone.start().await;
    anna_phone.stop();
    ben.start().await;

    // The laptop writes while the phone is closed.
    anna.start().await;
    anna.send(&ben, "vom Laptop");
    ben.wait_for(&anna, "the laptop's message", |m| !m.is_empty())
        .await;
    anna.stop();

    // Ben answers while both of Anna's devices are closed.
    ben.send(&anna, "an Anna");
    ben.wait_for(&anna, "sent", |m| {
        m.iter()
            .any(|m| m.own && m.delivery != Some(super::messages::DeliveryState::Waiting))
    })
    .await;

    // The phone opens alone: it gets its own copy of the laptop's message and Ben's answer.
    anna_phone.start().await;
    let on_phone = anna_phone
        .wait_for(&ben, "both messages on the phone", |m| m.len() == 2)
        .await;
    let laptop = anna.messages_with(&ben);
    let laptop_ids: Vec<_> = laptop.iter().map(|m| m.id.clone()).collect();
    assert!(
        on_phone.iter().any(|m| m.own && laptop_ids.contains(&m.id)),
        "the laptop's message has the same id on the phone"
    );
    assert!(on_phone
        .iter()
        .any(|m| !m.own && m.text.as_deref() == Some("an Anna")));
}

/// A note written on the laptop reaches the phone through the Nostr-Relay although the laptop is
/// offline by then and never meets the phone (spec 072 FR-002b, T144, quickstart notes).
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_note_reaches_an_own_device_that_was_closed_while_the_sender_is_offline() {
    let (_relay, url) = mock_relay().await;
    let anna = ChatDevice::genesis(&[url]);
    let phone = anna.link();
    // The phone was online once, so its chat key record is on the relay.
    phone.start().await;
    phone.stop();

    anna.start().await;
    let (me, own) = (anna.me.clone(), anna.me.vault_hex.clone());
    let now = anna.clock.now();
    let thread = anna
        .db
        .write_blocking(move |tx| {
            let thread = store::open_note(tx, &own, now)?;
            send::message(tx, &me, thread, "n1", "Reifen wechseln", Vec::new(), now)?;
            Ok(thread)
        })
        .expect("note");
    assert_eq!(thread, ids::note_thread_id(&anna.me.vault_hex));
    anna.session.kick.notify_one();
    // The laptop's wrap is on the relay once it shows as sent; then it goes offline.
    tokio::time::timeout(std::time::Duration::from_secs(30), async {
        loop {
            let waiting = anna
                .db
                .read_blocking(|r| Ok(outbox::due(r, i64::MAX - 1)?))
                .expect("outbox");
            if waiting.is_empty() {
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(100)).await;
        }
    })
    .await
    .expect("the note was published");
    anna.stop();

    phone.start().await;
    let own = phone.me.vault_hex.clone();
    let found = tokio::time::timeout(std::time::Duration::from_secs(30), async {
        loop {
            let shown = phone
                .db
                .read_blocking({
                    let own = own.clone();
                    move |r| Ok(messages::list(r, &own, thread, Default::default())?)
                })
                .expect("messages");
            if !shown.is_empty() {
                break shown;
            }
            tokio::time::sleep(std::time::Duration::from_millis(100)).await;
        }
    })
    .await
    .expect("the note reached the phone");
    assert_eq!(found.len(), 1);
    assert_eq!(found[0].text.as_deref(), Some("Reifen wechseln"));
    assert!(found[0].own);
    assert_eq!(found[0].delivery, None, "no delivery state for a note");
}
