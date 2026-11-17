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
    let refused = client
        .send_event(&expired)
        .await
        .map_or(true, |sent| sent.success.is_empty());
    assert!(refused, "an expired wrap is refused");

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
