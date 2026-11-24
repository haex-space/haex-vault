//! The own chat key record on the Nostr-Relays (spec 095 T012, T013): published where contacts look
//! for it, fetched and accepted by them, renewed after 30 days without the user, and left alone
//! while no Nostr-Relay is connected.

use std::sync::Arc;

use uuid::Uuid;
use vodozemac::olm::{Account, OlmMessage, SessionConfig};

use super::*;
use crate::chat::peer::ratchet::account::OwnKeys;
use crate::chat::peer::test_support::{mock_relay, two_vaults, ChatDevice};

const DAY_MS: i64 = 24 * 60 * 60 * 1000;

async fn delivery_of(device: &ChatDevice) -> Arc<Delivery> {
    device.delivery().await
}

fn account_row(device: &ChatDevice) -> Option<(i64, Option<i64>, Vec<String>)> {
    let installation = device.me.installation;
    device
        .db
        .read_blocking(move |r| {
            let Some(row) = store::account(r, installation)? else {
                return Ok(None);
            };
            Ok(Some((
                row.start_key_made_at,
                row.record_published_at,
                store::record_relays(r, installation)?,
            )))
        })
        .expect("read")
}

fn own_keys_of(device: &ChatDevice) -> OwnKeys {
    let installation = device.me.installation;
    device
        .db
        .read_blocking(move |r| {
            Ok(account::own_keys(
                &account::load(r, installation)?.expect("account").0,
            )?)
        })
        .expect("keys")
}

#[tokio::test]
async fn a_published_record_is_fetched_and_accepted_by_a_contact() {
    let (_relay, url) = mock_relay().await;
    let (anna, ben) = two_vaults(std::slice::from_ref(&url)).await;
    let anna_delivery = delivery_of(&anna).await;
    anna_delivery
        .catch_up_and_keep_record()
        .await
        .expect("record");
    let (_, published, relays) = account_row(&anna).expect("an account");
    assert!(published.is_some(), "a Nostr-Relay accepted it");
    assert_eq!(relays, std::slice::from_ref(&url));

    let ben_delivery = delivery_of(&ben).await;
    let row = ben_delivery
        .fetch_record(&anna.me.keys.device_pubkey, &[url])
        .await
        .expect("fetch")
        .expect("Anna's record");
    assert_eq!(row.identity_key, own_keys_of(&anna).identity.to_base64());
    assert_eq!(row.start_key, own_keys_of(&anna).start.to_base64());
}

/// Spec 072 US2 scenario 8: a card without Nostr-Relays; senders publish to their own relays then,
/// and the record has to be there too.
#[tokio::test]
async fn a_vault_whose_card_names_no_relay_publishes_where_its_contact_looks() {
    let (_relay, url) = mock_relay().await;
    let anna = ChatDevice::genesis(&[]);
    let ben = ChatDevice::genesis(std::slice::from_ref(&url));
    anna.learn(&ben).await;
    ben.learn(&anna).await;

    delivery_of(&anna)
        .await
        .catch_up_and_keep_record()
        .await
        .expect("record");
    let (_, _, relays) = account_row(&anna).expect("an account");
    assert_eq!(relays, std::slice::from_ref(&url), "on the contact's relay");

    let found = delivery_of(&ben)
        .await
        .fetch_record(&anna.me.keys.device_pubkey, &[url])
        .await
        .expect("fetch");
    assert!(found.is_some());
}

#[tokio::test]
async fn the_start_key_is_renewed_after_thirty_days_and_the_previous_one_still_opens() {
    let (_relay, url) = mock_relay().await;
    let (anna, ben) = two_vaults(std::slice::from_ref(&url)).await;
    let delivery = delivery_of(&anna).await;
    delivery.catch_up_and_keep_record().await.expect("first");
    let (made_at, first_published, _) = account_row(&anna).expect("account");
    let first_keys = own_keys_of(&anna);

    anna.clock.advance(29 * DAY_MS);
    delivery.catch_up_and_keep_record().await.expect("day 29");
    let (still, published, _) = account_row(&anna).expect("account");
    assert_eq!(still, made_at, "no renewal before 30 days");
    assert_eq!(
        published, first_published,
        "and no publish without a reason"
    );

    anna.clock.advance(2 * DAY_MS);
    delivery.catch_up_and_keep_record().await.expect("day 31");
    let (renewed_at, published, _) = account_row(&anna).expect("account");
    assert!(
        renewed_at >= made_at + 31 * DAY_MS,
        "renewed by the delivery's clock"
    );
    assert!(
        published.is_some_and(|at| at >= renewed_at),
        "the new record went out"
    );
    let renewed = own_keys_of(&anna);
    assert_ne!(renewed.start, first_keys.start);
    assert_eq!(renewed.identity, first_keys.identity);

    let sender = Account::new();
    let mut session = sender
        .create_outbound_session(
            SessionConfig::version_1(),
            first_keys.identity,
            first_keys.start,
        )
        .expect("session");
    let OlmMessage::PreKey(opening) = session.encrypt("hallo").expect("encrypt") else {
        panic!("an opening message")
    };
    let installation = anna.me.installation;
    let opens = anna
        .db
        .read_blocking(move |r| {
            let (mut account, _) = account::load(r, installation)?.expect("account");
            Ok(account
                .create_inbound_session(
                    SessionConfig::version_1(),
                    sender.curve25519_key(),
                    &opening,
                )
                .is_ok())
        })
        .expect("read");
    assert!(opens, "the previous start key still opens a session");

    let fetched = delivery_of(&ben)
        .await
        .fetch_record(&anna.me.keys.device_pubkey, &[url])
        .await
        .expect("fetch")
        .expect("record");
    assert_eq!(
        fetched.start_key,
        renewed.start.to_base64(),
        "the relay has the new one"
    );
}

#[tokio::test]
async fn nothing_is_made_renewed_or_published_without_a_connected_relay() {
    let offline = "ws://127.0.0.1:9".to_owned();
    let anna = ChatDevice::genesis(&[offline]);
    let delivery = delivery_of(&anna).await;
    delivery.catch_up_and_keep_record().await.expect("no step");
    assert!(account_row(&anna).is_none(), "no account before a catch-up");

    let installation: Uuid = anna.me.installation;
    let made = anna.clock.now();
    anna.db
        .write_blocking(move |tx| Ok(account::load_or_create(tx, installation, made).map(|_| ())?))
        .expect("account");
    anna.clock.advance(31 * DAY_MS);
    delivery.catch_up_and_keep_record().await.expect("no step");
    let (made_at, published, _) = account_row(&anna).expect("account");
    assert_eq!(made_at, made, "no renewal before a catch-up with a relay");
    assert_eq!(published, None);
}

/// A catch-up skipped without a connected relay is owed, so the run loop makes it up on its next
/// outbox tick instead of waiting for the catch-up timer; one that ran is not.
#[tokio::test]
async fn a_catch_up_without_a_connected_relay_is_owed_until_one_runs() {
    let (_relay, url) = mock_relay().await;
    let anna = ChatDevice::genesis(&[url]);
    let ben = ChatDevice::genesis(&[]);
    anna.learn(&ben).await;
    anna.introduce(&ben);
    let closing = tokio_util::sync::CancellationToken::new();
    let (delivery, incoming, forward) = Delivery::connect(
        ben.db.clone(),
        ben.me.clone(),
        ben.clock.clock(),
        Vec::new(),
        closing.clone(),
    )
    .await
    .expect("offline delivery");
    let runner =
        tokio::spawn(Arc::clone(&delivery).run(Arc::clone(&ben.session), incoming, forward));
    tokio::time::timeout(std::time::Duration::from_secs(10), async {
        while !delivery.catch_up_owed() {
            tokio::time::sleep(std::time::Duration::from_millis(20)).await;
        }
    })
    .await
    .expect("initial catch-up skipped without a relay");

    anna.start().await;
    let text = "Stored while Ben had no listening relay";
    anna.send(&ben, text);
    anna.wait_for(&ben, "stored on the relay", |messages| {
        messages.iter().any(|message| {
            message.text.as_deref() == Some(text)
                && message.delivery == Some(super::super::messages::DeliveryState::Sent)
        })
    })
    .await;
    assert!(ben.messages_with(&anna).is_empty());

    ben.learn(&anna).await;
    ben.wait_within(
        std::time::Duration::from_secs(30),
        &anna,
        "archived message on an outbox tick",
        |messages| {
            messages
                .iter()
                .any(|message| message.text.as_deref() == Some(text))
        },
    )
    .await;
    closing.cancel();
    runner.await.expect("delivery stopped");
    assert!(!delivery.catch_up_owed());
}
