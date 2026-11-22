//! Where wraps go (spec 072 T035, T046, FR-016, FR-082, US2 scenario 8).

use crate::chat::peer::targets;
use crate::chat::peer::test_support::ChatDevice;
use crate::contacts::card::own_card;

const RELAY: &str = "wss://relay.example.org";

fn target(device: &ChatDevice, vault: &str) -> targets::Target {
    let (me, vault) = (device.me.clone(), vault.to_owned());
    device
        .db
        .read_blocking(move |r| Ok(targets::of(r, &me, &vault)?))
        .expect("target")
}

#[test]
fn the_subscription_listens_on_the_nostr_relays_of_the_own_card() {
    let anna = ChatDevice::genesis(&[RELAY.to_owned(), "wss://zwei.example.org".to_owned()]);
    let (listening, on_card) = anna
        .db
        .read_blocking(|r| {
            Ok((
                targets::own_relays(r)?,
                own_card::stored(r)?.expect("card").card.nostr_relays,
            ))
        })
        .expect("read");
    assert_eq!(listening, on_card, "FR-082");
    assert_eq!(listening.len(), 2);
}

#[test]
fn own_copies_go_to_listed_other_devices_and_never_to_removed_ones() {
    let anna = ChatDevice::genesis(&[RELAY.to_owned()]);
    let own = anna.me.vault_hex.clone();
    assert!(
        target(&anna, &own).devices.is_empty(),
        "a single device has no own copies to send"
    );

    let phone = anna.link();
    let to_own = target(&anna, &own);
    assert_eq!(to_own.devices, [phone.me.keys.device_pubkey]);
    assert_eq!(to_own.relays, [RELAY.to_owned()]);
    assert_eq!(
        target(&phone, &own).devices,
        [anna.me.keys.device_pubkey],
        "and the other way round"
    );

    let removed = phone.me.keys.device_pubkey;
    anna.member.issue_list(|mut list| {
        list.devices
            .retain(|device| device.device_pubkey != removed);
        list
    });
    assert!(
        target(&anna, &own).devices.is_empty(),
        "a removed device gets no wraps"
    );
}

#[tokio::test]
async fn a_contact_card_without_nostr_relays_falls_back_to_the_own_ones() {
    let anna = ChatDevice::genesis(&[RELAY.to_owned()]);
    let ben = ChatDevice::genesis(&[]);
    anna.learn(&ben).await;
    let to_ben = target(&anna, &ben.me.vault_hex);
    assert_eq!(to_ben.devices, [ben.me.keys.device_pubkey]);
    assert_eq!(to_ben.relays, [RELAY.to_owned()]);
    assert!(to_ben.own_relays_instead, "shown as a hint");

    let carla = ChatDevice::genesis(&["wss://carla.example.org".to_owned()]);
    anna.learn(&carla).await;
    let to_carla = target(&anna, &carla.me.vault_hex);
    assert_eq!(to_carla.relays, ["wss://carla.example.org".to_owned()]);
    assert!(!to_carla.own_relays_instead);
    assert!(
        target(&anna, &"c3".repeat(32)).devices.is_empty(),
        "a vault without a card has no devices"
    );
}

/// Spec 095 T003: how the in-process Nostr-Relay treats the times of a wrap decides how the tests
/// move their clock ([`crate::chat::peer::test_support::TestClock`]): a wrap dated in the future is
/// taken and served, one past its `expiration` is refused and never served.
#[tokio::test]
async fn the_test_relay_takes_future_wraps_and_refuses_expired_ones() {
    use std::time::Duration;

    use nostr::event::Kind;
    use nostr::filter::Filter;

    use crate::chat::peer::delivery::now_ms;
    use crate::chat::peer::envelope::CHAT_KIND;
    use crate::chat::peer::test_support::{mock_relay, TestClock};
    use crate::chat::peer::wrap;
    use crate::sync::keys::DeviceKeys;

    const DAY_MS: i64 = 24 * 60 * 60 * 1000;
    let (_relay, url) = mock_relay().await;
    let client = nostr_sdk::client::Client::builder().build();
    client.add_relay(url.as_str()).await.expect("relay");
    client.connect().and_wait(Duration::from_secs(5)).await;
    let sender = DeviceKeys::generate();
    let recipient = DeviceKeys::generate();
    let wrap_at = |at: i64| {
        wrap::wrap(
            CHAT_KIND,
            "{}",
            &sender.device_secret,
            &recipient.device_pubkey,
            at,
        )
        .expect("wrap")
    };

    let clock = TestClock::default();
    clock.advance(40 * DAY_MS);
    let future = wrap_at(clock.now());
    let sent = client.send_event(&future).await.expect("send");
    assert!(!sent.success.is_empty(), "a wrap dated ahead is taken");

    let expired = wrap_at(now_ms() - 31 * DAY_MS);
    let expired_result = client
        .send_event(&expired)
        .await
        .expect("the relay must return an explicit expiration rejection");
    assert!(
        expired_result.success.is_empty(),
        "an expired wrap is refused"
    );
    assert!(
        expired_result
            .failed
            .values()
            .any(|reason| reason.to_lowercase().contains("expir")),
        "the relay explicitly rejects the expired wrap: {:?}",
        expired_result.failed
    );

    let to_recipient = Filter::new()
        .kind(Kind::GiftWrap)
        .pubkey(crate::sync::presence::public_key(&recipient.device_pubkey).expect("key"));
    let served: Vec<_> = client
        .fetch_events(to_recipient)
        .timeout(Duration::from_secs(5))
        .await
        .expect("fetch")
        .into_iter()
        .map(|event| event.id)
        .collect();
    assert_eq!(served, [future.id], "only the one dated ahead is served");
    client.shutdown().await;
}

// Receiving on device sessions (spec 095 T016).

mod receiving {
    use nostr::event::{Event, Kind};
    use nostr::types::Timestamp;

    use super::super::sending::seal;
    use crate::chat::peer::envelope::{self, To, CHAT_KIND, RECEIPT_KIND};
    use crate::chat::peer::outbox::{self, Entry, Rumor};
    use crate::chat::peer::ratchet::{store, RATCHET_KIND};
    use crate::chat::peer::send::own_card_text;
    use crate::chat::peer::test_support::{two_vaults, ChatDevice};
    use crate::chat::peer::wrap;
    use crate::sync::gift_wrap::{self, WrapShape};

    const DAY_MS: i64 = 24 * 60 * 60 * 1000;

    /// The rows `from` queued, sealed for `to`'s device on the session as sending does.
    fn sealed_for(from: &ChatDevice, to: &ChatDevice) -> Vec<Event> {
        let (me, device, now) = (from.me.clone(), to.me.keys.device_pubkey, from.clock.now());
        from.db
            .write_blocking(move |tx| {
                let mut wraps = Vec::new();
                for entry in outbox::due(tx, i64::MAX - 1)? {
                    wraps.extend(seal(tx, &me, &entry, &device, now)?);
                }
                Ok(wraps)
            })
            .expect("seal")
    }

    /// `rumor` of `kind` sealed for `to` on `from`'s session, as an outbox row of its own.
    fn sealed_rumor(from: &ChatDevice, to: &ChatDevice, kind: u16, content: String) -> Event {
        let (me, device, now) = (from.me.clone(), to.me.keys.device_pubkey, from.clock.now());
        let entry = Entry {
            id: envelope::new_event_id(),
            thread_id: "t".into(),
            message_id: None,
            rumor: Rumor { kind, content },
            pending: Vec::new(),
            awaiting: Vec::new(),
            attempts: 0,
            created_at: now,
        };
        from.db
            .write_blocking(move |tx| Ok(seal(tx, &me, &entry, &device, now)?))
            .expect("seal")
            .expect("a record of the recipient")
    }

    async fn receive(device: &ChatDevice, wraps: Vec<Event>) -> usize {
        device
            .delivery()
            .await
            .receive_batch(wraps.into_iter().map(|wrap| (wrap, None)).collect())
            .await
            .expect("batch")
            .len()
    }

    fn sessions_with(device: &ChatDevice, peer: &ChatDevice) -> usize {
        let (installation, peer) = (device.me.installation, peer.me.device_hex());
        device
            .db
            .read_blocking(move |r| Ok(store::sessions(r, installation, &peer)?.len()))
            .expect("sessions")
    }

    /// The content of a ratchet wrap to `to`.
    fn ratchet_content(to: &ChatDevice, wrap: &Event) -> String {
        let opened = wrap::unwrap(wrap, &to.me.keys.device_secret).expect("open");
        assert_eq!(opened.kind, RATCHET_KIND);
        opened.content
    }

    #[tokio::test]
    async fn a_batch_of_sixty_messages_in_reverse_order_is_stored_whole_and_in_order() {
        let (anna, ben) = two_vaults(&[]).await;
        anna.introduce(&ben);
        let texts: Vec<String> = (0..60).map(|n| format!("Nachricht {n}")).collect();
        for text in &texts {
            anna.clock.advance(1_000);
            anna.send(&ben, text);
        }
        let mut wraps = sealed_for(&anna, &ben);
        assert_eq!(wraps.len(), 60);
        wraps.reverse();
        assert_eq!(receive(&ben, wraps).await, 60);
        let stored: Vec<String> = ben
            .messages_with(&anna)
            .into_iter()
            .filter_map(|m| m.text)
            .collect();
        assert_eq!(stored, texts);
    }

    #[tokio::test]
    async fn an_event_or_receipt_directly_in_a_seal_is_the_earlier_format_and_dropped() {
        let (anna, ben) = two_vaults(&[]).await;
        anna.send(&ben, "im alten Format");
        let entry = anna
            .db
            .read_blocking(|r| Ok(outbox::due(r, i64::MAX - 1)?))
            .expect("outbox")
            .remove(0);
        let earlier = |kind: Kind| {
            wrap::wrap(
                kind,
                &entry.rumor.content,
                &anna.me.keys.device_secret,
                &ben.me.keys.device_pubkey,
                anna.clock.now(),
            )
            .expect("wrap")
        };
        let wraps = vec![earlier(CHAT_KIND), earlier(RECEIPT_KIND)];
        assert_eq!(receive(&ben, wraps).await, 0);
        assert!(ben.messages_with(&anna).is_empty());
    }

    #[tokio::test]
    async fn a_wrap_past_its_expiration_or_without_one_is_dropped_before_it_is_opened() {
        let (anna, ben) = two_vaults(&[]).await;
        anna.introduce(&ben);
        anna.send(&ben, "zu spät");
        let fresh = sealed_for(&anna, &ben).remove(0);
        let content = ratchet_content(&ben, &fresh);
        let expired = wrap::wrap(
            RATCHET_KIND,
            &content,
            &anna.me.keys.device_secret,
            &ben.me.keys.device_pubkey,
            ben.clock.now() - 31 * DAY_MS,
        )
        .expect("wrap");
        let endless = gift_wrap::wrap_shaped_at(
            WrapShape {
                expires_after_secs: None,
                ..wrap::CHAT_WRAP
            },
            RATCHET_KIND,
            &anna.me.keys.device_secret,
            &content,
            &crate::sync::presence::public_key(&ben.me.keys.device_pubkey).expect("key"),
            Timestamp::now(),
        )
        .expect("wrap");
        assert_eq!(receive(&ben, vec![expired, endless]).await, 0);
        assert_eq!(sessions_with(&ben, &anna), 0, "nothing was opened");

        assert_eq!(
            receive(&ben, vec![fresh]).await,
            1,
            "the message key was not used by the dropped ones"
        );
    }

    #[tokio::test]
    async fn a_keep_alive_counts_only_from_a_device_on_a_stored_card() {
        let (anna, ben) = two_vaults(&[]).await;
        let carla = ChatDevice::genesis(&[]);
        carla.introduce(&ben);
        let stranger = sealed_rumor(&carla, &ben, 0, String::new());
        assert_eq!(receive(&ben, vec![stranger]).await, 0);
        assert_eq!(sessions_with(&ben, &carla), 0, "no session with a stranger");

        anna.introduce(&ben);
        let contact = sealed_rumor(&anna, &ben, 0, String::new());
        receive(&ben, vec![contact]).await;
        assert_eq!(
            sessions_with(&ben, &anna),
            1,
            "a contact's keep-alive opens it"
        );
    }

    #[tokio::test]
    async fn an_opening_whose_event_fails_the_receiver_checks_leaves_no_session() {
        let (anna, ben) = two_vaults(&[]).await;
        anna.introduce(&ben);
        let card = anna
            .db
            .read_blocking(|r| Ok(own_card_text(r)?))
            .expect("card");
        let elsewhere = envelope::message(
            "nicht für Ben",
            Vec::new(),
            &anna.me.vault_hex,
            &card,
            To::Direct("c3".repeat(32)),
            anna.clock.now(),
        )
        .expect("envelope");
        let content = envelope::encode(&elsewhere).expect("encode");
        let wrap = sealed_rumor(&anna, &ben, CHAT_KIND.as_u16(), content);
        assert_eq!(receive(&ben, vec![wrap]).await, 0);
        assert!(ben.messages_with(&anna).is_empty());
        assert_eq!(sessions_with(&ben, &anna), 0);
    }

    #[tokio::test]
    async fn an_event_sealed_by_another_device_than_its_session_is_dropped() {
        let (anna, ben) = two_vaults(&[]).await;
        let carla = ChatDevice::genesis(&[]);
        ben.learn(&carla).await;
        anna.introduce(&ben);
        anna.send(&ben, "von Anna");
        let original = sealed_for(&anna, &ben).remove(0);
        let resealed = wrap::wrap(
            RATCHET_KIND,
            &ratchet_content(&ben, &original),
            &carla.me.keys.device_secret,
            &ben.me.keys.device_pubkey,
            carla.clock.now(),
        )
        .expect("wrap");
        assert_eq!(receive(&ben, vec![resealed]).await, 0);
        assert_eq!(sessions_with(&ben, &carla), 0);
        assert_eq!(sessions_with(&ben, &anna), 0);

        assert_eq!(
            receive(&ben, vec![original]).await,
            1,
            "Anna's own wrap opens it"
        );
    }
}
