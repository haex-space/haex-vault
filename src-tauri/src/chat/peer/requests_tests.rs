//! Requests of vaults that are no contact (spec 072 T048, T055, US5 scenarios 1–4 and 6, quickstart
//! A9): Ben has Anna's card, Anna has not Ben's. Ben's message lands in Anna's "Anfragen" with the
//! name of the card it carried; Ben sees _delivered_ but never _read_. Accepting (reading the card
//! in) moves it to the list, declining removes it until the next message, blocking drops what
//! follows without a receipt. A deleted contact's next message is a request again.

use std::time::Duration;

use haex_crdt::rusqlite::params;

use crate::chat::peer::delivery::now_ms;
use crate::chat::peer::messages::{self, DeliveryState};
use crate::chat::peer::requests::{self, Answer};
use crate::chat::peer::store::{self, Conversation, ConversationState, ListFilter};
use crate::chat::peer::test_support::{mock_relay, ChatDevice};
use crate::contacts::card::{format, received};
use crate::storage::query::Query;

/// Ben knows Anna, Anna does not know Ben; both vaults are open.
async fn stranger_writes() -> (ChatDevice, ChatDevice, nostr_sdk::local_relay::LocalRelay) {
    let (relay, url) = mock_relay().await;
    let anna = ChatDevice::genesis(std::slice::from_ref(&url));
    let ben = ChatDevice::genesis(&[url]);
    ben.learn(&anna).await;
    anna.start().await;
    ben.start().await;
    (anna, ben, relay)
}

fn list(device: &ChatDevice, requests: bool) -> Vec<Conversation> {
    let own = device.me.vault_hex.clone();
    device
        .db
        .read_blocking(move |r| {
            Ok(store::list(
                r,
                &own,
                ListFilter {
                    kind: None,
                    requests,
                },
            )?)
        })
        .expect("list")
}

fn state_of(found: &[messages::PeerMessage], text: &str) -> Option<DeliveryState> {
    found
        .iter()
        .find(|m| m.text.as_deref() == Some(text))
        .and_then(|m| m.delivery)
}

fn answer(anna: &ChatDevice, ben: &ChatDevice, answer: Answer) {
    let thread = anna.thread_with(ben);
    anna.db
        .write_blocking(move |tx| Ok(requests::answer(tx, thread, answer)?))
        .expect("answer");
}

fn read_all(anna: &ChatDevice, ben: &ChatDevice) {
    let (me, thread) = (anna.me.clone(), anna.thread_with(ben));
    anna.db
        .write_blocking(move |tx| {
            let upto = messages::newest(tx, thread)?;
            Ok(crate::chat::peer::send::read_up_to(
                tx,
                &me,
                thread,
                upto,
                now_ms(),
            )?)
        })
        .expect("read");
    anna.session.kick.notify_one();
}

fn outbox_rows(device: &ChatDevice) -> i64 {
    device
        .db
        .read_blocking(|r| {
            Ok(r.query_row(
                "SELECT COUNT(*) FROM chat_outbox_no_sync",
                params![],
                |row| row.get(0),
            )?
            .unwrap_or_default())
        })
        .expect("outbox")
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_message_of_a_stranger_is_a_request_that_shows_delivered_but_never_read() {
    let (anna, ben, _relay) = stranger_writes().await;
    ben.send(&anna, "Hallo, wir kennen uns vom Markt");
    anna.wait_for(&ben, "the request", |m| !m.is_empty()).await;

    let requests = list(&anna, true);
    assert_eq!(requests.len(), 1, "{requests:?}");
    assert_eq!(requests[0].state, ConversationState::Request);
    assert_eq!(requests[0].peer_contact_id, None);
    assert_eq!(
        requests[0].peer_card_name.as_deref(),
        Some(ben.card().card.name.as_str()),
        "the name comes from the carried card"
    );
    assert!(list(&anna, false).is_empty(), "not in the list itself");

    let older_card = ben.card();
    let _second_device = ben.link();
    let newer_card = ben.card();
    let expected_newest = newer_card.clone();
    let (thread, peer_vault) = (anna.thread_with(&ben), ben.me.vault_hex.clone());
    anna.db
        .write_blocking(move |tx| {
            store::mark_request(tx, thread, &peer_vault, &newer_card)?;
            store::mark_request(tx, thread, &peer_vault, &older_card)?;
            Ok(())
        })
        .expect("keep the newest carried card");
    let thread = anna.thread_with(&ben);
    let saved_card = anna
        .db
        .read_blocking(move |r| Ok(store::request_card(r, thread)?))
        .expect("read request card");
    assert_eq!(saved_card, Some(expected_newest));

    ben.wait_for(&anna, "delivered", |m| {
        state_of(m, "Hallo, wir kennen uns vom Markt") == Some(DeliveryState::Delivered)
    })
    .await;

    // Anna opens the request: no read receipt goes out (FR-043).
    read_all(&anna, &ben);
    tokio::time::sleep(Duration::from_secs(3)).await;
    assert_eq!(
        state_of(&ben.messages_with(&anna), "Hallo, wir kennen uns vom Markt"),
        Some(DeliveryState::Delivered)
    );
    assert_eq!(outbox_rows(&anna), 0, "nothing waits to go to Ben");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn accepting_reads_the_carried_card_in_and_moves_the_conversation_to_the_list() {
    let (anna, ben, _relay) = stranger_writes().await;
    ben.send(&anna, "Hallo");
    anna.wait_for(&ben, "the request", |m| !m.is_empty()).await;

    let thread = anna.thread_with(&ben);
    let text = anna
        .db
        .read_blocking(move |r| Ok(requests::card_text(r, thread)?))
        .expect("card of the request");
    let card = format::parse(&text).expect("a card");
    assert_eq!(card.card.vault, ben.member.vault);
    let contact_id = anna.learn_card(card).await;

    assert!(list(&anna, true).is_empty());
    let conversations = list(&anna, false);
    assert_eq!(conversations.len(), 1);
    assert_eq!(conversations[0].state, ConversationState::Active);
    assert_eq!(
        conversations[0].peer_contact_id.as_deref(),
        Some(contact_id.as_str())
    );

    // Now Anna can answer, and reading shows _read_ at Ben.
    anna.send(&ben, "Hallo Ben");
    ben.wait_for(&anna, "Anna's answer", |m| m.iter().any(|m| !m.own))
        .await;
    read_all(&anna, &ben);
    ben.wait_for(&anna, "read", |m| {
        state_of(m, "Hallo") == Some(DeliveryState::Read)
    })
    .await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_declined_request_is_gone_and_the_next_message_is_a_new_request() {
    let (anna, ben, _relay) = stranger_writes().await;
    ben.send(&anna, "erste");
    anna.wait_for(&ben, "the request", |m| !m.is_empty()).await;
    answer(&anna, &ben, Answer::Decline);
    assert!(list(&anna, true).is_empty());
    assert!(anna.messages_with(&ben).is_empty());

    ben.send(&anna, "zweite");
    let found = anna
        .wait_for(&ben, "the new request", |m| !m.is_empty())
        .await;
    assert_eq!(found.len(), 1, "only the new message: {found:?}");
    assert_eq!(found[0].text.as_deref(), Some("zweite"));
    assert_eq!(list(&anna, true).len(), 1);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_blocked_sender_is_listed_and_its_next_message_is_dropped_without_a_receipt() {
    let (anna, ben, _relay) = stranger_writes().await;
    ben.send(&anna, "erste");
    anna.wait_for(&ben, "the request", |m| !m.is_empty()).await;
    answer(&anna, &ben, Answer::Block);
    assert!(list(&anna, true).is_empty());
    let blocked = anna
        .db
        .read_blocking(|r| Ok(received::blocked(r)?))
        .expect("blocked");
    assert_eq!(blocked.len(), 1);
    assert_eq!(blocked[0].vault_pubkey, ben.me.vault_hex);

    ben.send(&anna, "zweite");
    ben.wait_for(&anna, "sent", |m| {
        state_of(m, "zweite") == Some(DeliveryState::Sent)
    })
    .await;
    tokio::time::sleep(Duration::from_secs(3)).await;
    assert!(anna.messages_with(&ben).is_empty(), "dropped without trace");
    assert_eq!(
        state_of(&ben.messages_with(&anna), "zweite"),
        Some(DeliveryState::Sent),
        "no delivery receipt to a blocked vault"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_deleted_contact_keeps_the_conversation_and_its_next_message_is_a_request() {
    let (anna, ben, _relay) = stranger_writes().await;
    anna.learn(&ben).await;
    ben.send(&anna, "als Kontakt");
    anna.wait_for(&ben, "the message", |m| !m.is_empty()).await;
    assert_eq!(list(&anna, false).len(), 1);

    let vault = ben.member.vault;
    anna.db
        .write_blocking(move |tx| Ok(received::delete_for_vault(tx, &vault)?))
        .expect("delete the card");
    assert_eq!(list(&anna, false).len(), 1, "the conversation stays");

    ben.send(&anna, "nach dem Löschen");
    anna.wait_for(&ben, "the second message", |m| m.len() == 2)
        .await;
    let requests = list(&anna, true);
    assert_eq!(requests.len(), 1, "{requests:?}");
    assert_eq!(anna.messages_with(&ben).len(), 2, "the history is kept");
}
