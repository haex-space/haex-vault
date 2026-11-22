//! Forward secrecy (spec 095 T015, quickstart scenario 1, SC-001, SC-002): someone who keeps every
//! wrap a Nostr-Relay saw and later takes a device's secret and all its ratchet rows opens the
//! wraps and seals, but decrypts no message whose keys the device erased.

use std::time::Duration;

use nostr::event::Kind;
use nostr::filter::Filter;
use vodozemac::olm::{Account, AccountPickle, OlmMessage, Session, SessionConfig, SessionPickle};

use crate::chat::peer::ratchet::wire::RatchetRumor;
use crate::chat::peer::ratchet::{account, store, RATCHET_KIND};
use crate::chat::peer::test_support::{mock_relay, two_vaults};
use crate::chat::peer::wrap;

const DAY_MS: i64 = 24 * 60 * 60 * 1000;

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn an_archived_wrap_still_opens_but_its_message_no_longer_decrypts() {
    let (_relay, url) = mock_relay().await;
    let (anna, ben) = two_vaults(std::slice::from_ref(&url)).await;
    anna.start().await;
    ben.start().await;
    for n in 0..3 {
        anna.send(&ben, &format!("an Ben {n}"));
        ben.send(&anna, &format!("an Anna {n}"));
    }
    ben.wait_for(&anna, "Anna's messages", |m| {
        m.iter().filter(|m| !m.own).count() == 3
    })
    .await;
    anna.wait_for(&ben, "Ben's messages", |m| {
        m.iter().filter(|m| !m.own).count() == 3
    })
    .await;
    ben.stop();

    // Two new start keys, each after a catch-up, then the vault closes and opens again.
    let installation = ben.me.installation;
    let made_at = || {
        ben.db
            .read_blocking(move |r| Ok(store::account(r, installation)?.expect("account")))
            .expect("account")
            .start_key_made_at
    };
    let first = made_at();
    let delivery = ben.delivery().await;
    for _ in 0..2 {
        ben.clock.advance(31 * DAY_MS);
        delivery.catch_up_and_keep_record().await.expect("catch-up");
    }
    assert!(made_at() >= first + 62 * DAY_MS, "renewed twice");
    drop(delivery);
    ben.start().await;
    ben.stop();

    // The archive: every wrap to Ben's device the relay kept.
    let client = nostr_sdk::client::Client::builder().build();
    client.add_relay(url.as_str()).await.expect("relay");
    client.connect().and_wait(Duration::from_secs(5)).await;
    let to_ben = Filter::new()
        .kind(Kind::GiftWrap)
        .pubkey(crate::sync::presence::public_key(&ben.me.keys.device_pubkey).expect("key"));
    let archive = client
        .fetch_events(to_ben)
        .timeout(Duration::from_secs(5))
        .await
        .expect("fetch");
    client.shutdown().await;
    assert!(archive.len() >= 3, "Anna's messages and receipts");

    // What is taken later: the device secret and every ratchet row of Ben's installation.
    let secret = ben.me.keys.device_secret.clone();
    let peer = anna.me.device_hex();
    let (account_pickle, session_pickles) = ben
        .db
        .read_blocking(move |r| {
            let (taken, _) = account::load(r, installation)?.expect("account");
            let sessions: Vec<String> = store::sessions(r, installation, &peer)?
                .into_iter()
                .map(|row| row.session.to_string())
                .collect();
            let pickle = serde_json::to_string(&taken.pickle()).expect("pickle");
            Ok((pickle, sessions))
        })
        .expect("rows");
    assert!(!session_pickles.is_empty());
    for wrap in archive {
        let opened = wrap::unwrap(&wrap, &secret).expect("the wrap and its seal still open");
        assert_eq!(opened.kind, RATCHET_KIND);
        let message = RatchetRumor::decode(&opened.content)
            .and_then(|rumor| rumor.message())
            .expect("a ratchet message");
        for pickle in &session_pickles {
            let pickle: SessionPickle = serde_json::from_str(pickle).expect("pickle");
            assert!(
                Session::from_pickle(pickle).decrypt(&message).is_err(),
                "a used message key is gone"
            );
        }
        if let OlmMessage::PreKey(opening) = &message {
            let mut taken = Account::from_pickle(
                serde_json::from_str::<AccountPickle>(&account_pickle).expect("pickle"),
            );
            assert!(
                taken
                    .create_inbound_session(
                        SessionConfig::version_1(),
                        opening.identity_key(),
                        opening,
                    )
                    .is_err(),
                "the start key of the opening is gone"
            );
        }
    }
}

/// SC-002: vodozemac does not hand out single message keys, so a state that holds exactly one
/// skipped key stands for it: that message decrypts once, an earlier one never.
#[test]
fn a_state_with_one_skipped_key_decrypts_that_message_once_and_no_earlier_one() {
    let anna = Account::new();
    let mut ben = Account::new();
    ben.generate_fallback_key();
    let start = ben.fallback_key().into_values().next().expect("start key");
    let mut to_ben = anna
        .create_outbound_session(SessionConfig::version_1(), ben.curve25519_key(), start)
        .expect("session");
    let [zero, one, two] =
        ["null", "eins", "zwei"].map(|text| to_ben.encrypt(text).expect("encrypt"));
    let OlmMessage::PreKey(opening) = &zero else {
        panic!("an opening message");
    };
    let mut at_ben = ben
        .create_inbound_session(SessionConfig::version_1(), anna.curve25519_key(), opening)
        .expect("inbound")
        .session;
    at_ben.decrypt(&two).expect("the third first");

    let mut taken = Session::from_pickle(at_ben.pickle());
    assert_eq!(taken.decrypt(&one).expect("the skipped one"), b"eins");
    assert!(taken.decrypt(&one).is_err(), "once");
    assert!(taken.decrypt(&zero).is_err(), "no earlier one");
}
