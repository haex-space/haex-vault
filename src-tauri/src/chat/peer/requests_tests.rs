//! Requests of strangers (spec 099 US1, US2, quickstart Q1–Q3): Ben has Anna's card, Anna has not
//! Ben's. Ben's messages leave one request in Anna's "Anfragen" with the name of the card they
//! carried and nothing else, and Ben gets no receipt. Accepting (reading the card in) sends Ben the
//! answer "accepted", and his waiting messages arrive. Declining blocks Ben with his card and sends
//! him "blocked": his messages fail with that reason, a new one waits, and nothing reaches Anna. A
//! deleted contact's next message is a request again.

use std::time::Duration;

use haex_crdt::rusqlite::params;

use crate::chat::peer::delivery::now_ms;
use crate::chat::peer::messages::{self, DeliveryState};
use crate::chat::peer::requests::{self, Answer};
use crate::chat::peer::store::{self, Conversation, ConversationState, ListFilter};
use crate::chat::peer::test_support::{mock_relay, ChatDevice};
use crate::contacts::card::{format, received};
use crate::storage::query::Query;

/// How long a test waits for something the other vault does.
const PATIENCE: Duration = Duration::from_secs(30);

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

/// Waits until `done` holds, polling; panics with `what` after [`PATIENCE`].
async fn eventually(what: &str, done: impl Fn() -> bool) {
    let deadline = tokio::time::Instant::now() + PATIENCE;
    while !done() {
        assert!(
            tokio::time::Instant::now() < deadline,
            "waited in vain for {what}"
        );
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
}

fn state_of(found: &[messages::PeerMessage], text: &str) -> Option<DeliveryState> {
    found
        .iter()
        .find(|m| m.text.as_deref() == Some(text))
        .and_then(|m| m.delivery)
}

fn reason_of(found: &[messages::PeerMessage], text: &str) -> Option<String> {
    found
        .iter()
        .find(|m| m.text.as_deref() == Some(text))
        .and_then(|m| m.failed_reason.clone())
}

fn rows_from(device: &ChatDevice, sender: &ChatDevice) -> (i64, i64) {
    let sender = sender.me.vault_hex.clone();
    device
        .db
        .read_blocking(move |r| {
            let messages = r
                .query_row(
                    "SELECT COUNT(*) FROM chat_peer_messages WHERE sender_vault = ?1",
                    params![sender],
                    |row| row.get(0),
                )?
                .unwrap_or_default();
            let attachments = r
                .query_row(
                    "SELECT COUNT(*) FROM chat_attachments a \
                     JOIN chat_peer_messages m ON m.id = a.message_id WHERE m.sender_vault = ?1",
                    params![sender],
                    |row| row.get(0),
                )?
                .unwrap_or_default();
            Ok((messages, attachments))
        })
        .expect("count")
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
async fn a_stranger_leaves_one_request_and_nothing_of_its_messages() {
    let (anna, ben, _relay) = stranger_writes().await;
    for text in ["Hallo, wir kennen uns vom Markt", "Bist du da?", "Hallo?"] {
        ben.send(&anna, text);
    }
    eventually("the request", || {
        list(&anna, true)
            .first()
            .is_some_and(|request| request.request_last_at.is_some())
    })
    .await;
    // Every knock has arrived once Ben's three messages are _sent_ and a moment passed.
    eventually("Ben's messages taken by the Nostr-Relay", || {
        ben.messages_with(&anna)
            .iter()
            .all(|m| m.delivery == Some(DeliveryState::Sent))
    })
    .await;
    tokio::time::sleep(Duration::from_secs(3)).await;

    let requests = list(&anna, true);
    assert_eq!(requests.len(), 1, "one request per vault: {requests:?}");
    assert_eq!(requests[0].state, ConversationState::Request);
    assert_eq!(requests[0].peer_contact_id, None);
    assert_eq!(
        requests[0].peer_card_name.as_deref(),
        Some(ben.card().card.name.as_str()),
        "the name comes from the carried card"
    );
    assert!(list(&anna, false).is_empty(), "not in the list itself");
    assert_eq!(rows_from(&anna, &ben), (0, 0), "nothing of the messages");
    assert!(anna.messages_with(&ben).is_empty());
    let thread = anna.thread_with(&ben);
    let (first, last): (Option<i64>, Option<i64>) = anna
        .db
        .read_blocking(move |r| {
            Ok(r.query_row(
                "SELECT request_first_at, request_last_at FROM chat_threads WHERE id = ?1",
                params![thread.to_string()],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )?
            .expect("thread"))
        })
        .expect("times");
    assert!(first.is_some() && first <= last, "{first:?} {last:?}");

    assert!(
        ben.messages_with(&anna)
            .iter()
            .all(|m| m.delivery == Some(DeliveryState::Sent)),
        "no receipt for a request (FR-005)"
    );
    assert_eq!(outbox_rows(&anna), 0, "nothing waits to go to Ben");

    // A later knock with a newer card keeps the newer one, an older card does not replace it.
    let older_card = ben.card();
    let _second_device = ben.link();
    let newer_card = ben.card();
    let expected_newest = newer_card.clone();
    let (thread, peer_vault) = (anna.thread_with(&ben), ben.me.vault_hex.clone());
    let became = anna
        .db
        .write_blocking(move |tx| {
            let newer = store::mark_request(tx, thread, &peer_vault, &newer_card, now_ms())?;
            let older = store::mark_request(tx, thread, &peer_vault, &older_card, now_ms())?;
            Ok((newer, older))
        })
        .expect("knock again");
    assert_eq!(became, (false, false), "it was a request already (FR-004)");
    let saved_card = anna
        .db
        .read_blocking(move |r| Ok(store::request_card(r, thread)?))
        .expect("read request card");
    assert_eq!(saved_card, Some(expected_newest));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn accepting_sends_the_answer_and_the_waiting_messages_arrive_in_order() {
    let (anna, ben, _relay) = stranger_writes().await;
    ben.send(&anna, "erste");
    ben.send(&anna, "zweite");
    eventually("the request", || !list(&anna, true).is_empty()).await;

    let thread = anna.thread_with(&ben);
    let text = anna
        .db
        .read_blocking(move |r| Ok(requests::card_text(r, thread)?))
        .expect("card of the request");
    let card = format::parse(&text).expect("a card");
    assert_eq!(card.card.vault, ben.member.vault);
    let contact_id = anna.learn_card(card).await;
    anna.session.kick.notify_one();

    assert!(list(&anna, true).is_empty());
    let conversations = list(&anna, false);
    assert_eq!(conversations.len(), 1);
    assert_eq!(conversations[0].state, ConversationState::Active);
    assert_eq!(
        conversations[0].peer_contact_id.as_deref(),
        Some(contact_id.as_str())
    );

    let found = anna
        .wait_for(&ben, "Ben's messages after the answer", |m| m.len() == 2)
        .await;
    let texts: Vec<_> = found.iter().map(|m| m.text.clone()).collect();
    assert_eq!(texts, [Some("erste".into()), Some("zweite".into())]);
    ben.wait_for(&anna, "delivered", |m| {
        state_of(m, "zweite") == Some(DeliveryState::Delivered)
    })
    .await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn declining_blocks_with_the_card_and_tells_the_sender_whose_next_message_waits() {
    let (anna, ben, _relay) = stranger_writes().await;
    ben.send(&anna, "erste");
    eventually("the request", || !list(&anna, true).is_empty()).await;
    let thread = anna.thread_with(&ben);
    anna.db
        .write_blocking(move |tx| Ok(requests::answer(tx, thread, Answer::Decline, now_ms())?))
        .expect("decline");
    anna.session.kick.notify_one();

    assert!(list(&anna, true).is_empty());
    let vault = ben.member.vault;
    let (blocked, kept) = anna
        .db
        .read_blocking(move |r| {
            Ok((
                received::is_blocked(r, &vault)?,
                received::blocked_card(r, &vault)?,
            ))
        })
        .expect("blocked");
    assert!(blocked);
    assert_eq!(
        kept.map(|card| card.card.vault),
        Some(vault),
        "with its card"
    );

    ben.wait_for(&anna, "the answer", |m| {
        reason_of(m, "erste").as_deref() == Some("blocked")
    })
    .await;
    assert_eq!(
        state_of(&ben.messages_with(&anna), "erste"),
        Some(DeliveryState::Failed)
    );
    let blocked_by_peer = list(&ben, false)
        .into_iter()
        .any(|conversation| conversation.blocked_by_peer);
    assert!(blocked_by_peer, "Ben's conversation says so");

    ben.send(&anna, "zweite");
    let waiting = ben.messages_with(&anna);
    assert_eq!(reason_of(&waiting, "zweite").as_deref(), Some("blocked"));
    tokio::time::sleep(Duration::from_secs(3)).await;
    assert!(anna.messages_with(&ben).is_empty(), "nothing reaches Anna");
    assert!(list(&anna, true).is_empty(), "and no new request");
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
    eventually("the request", || list(&anna, true).len() == 1).await;
    let found = anna.messages_with(&ben);
    assert_eq!(
        found.len(),
        1,
        "the history is kept, the knock adds nothing"
    );
    assert_eq!(found[0].text.as_deref(), Some("als Kontakt"));
}
