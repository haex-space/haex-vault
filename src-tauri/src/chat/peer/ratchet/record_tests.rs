//! Chat key records (spec 095 T011): the own record, and each reason a reader refuses one; a newer
//! record replaces, another identity key drops the sessions, an older record a Nostr-Relay hands
//! back is refused (quickstart scenario 7, step 4).

use nostr::event::{Event, EventBuilder, FinalizeEvent, Kind, Tag};
use nostr::key::Keys;
use nostr::types::Timestamp;

use super::*;
use crate::chat::peer::ratchet::session;
use crate::chat::peer::test_support::ChatDevice;

const DAY_MS: i64 = 24 * 60 * 60 * 1000;
const RELAY: &str = "wss://relay.example.org";

/// Anna and Ben, contacts of each other.
async fn anna_and_ben() -> (ChatDevice, ChatDevice) {
    let anna = ChatDevice::genesis(&[RELAY.to_owned()]);
    let ben = ChatDevice::genesis(&[RELAY.to_owned()]);
    anna.learn(&ben).await;
    ben.learn(&anna).await;
    (anna, ben)
}

/// `device`'s own record, its start key made at `made_at`, published at `now`.
fn record_of(device: &ChatDevice, made_at: i64, now: i64) -> Event {
    let me = device.me.clone();
    device
        .db
        .write_blocking(move |tx| {
            let account = account::load_or_create(tx, me.installation, made_at)?;
            Ok(own_event(&me, &account, made_at, now)?)
        })
        .expect("record")
}

fn accept_on(
    reader: &ChatDevice,
    event: &Event,
    now: i64,
) -> std::result::Result<RecordRow, Refused> {
    let (me, event) = (reader.me.clone(), event.clone());
    reader
        .db
        .write_blocking(move |tx| Ok(accept(tx, &me, &event, now)?))
        .expect("accept")
}

fn signed_by(device: &ChatDevice, kind: Kind, content: &str, at: i64) -> Event {
    let signer = presence::nostr_secret(&device.me.keys.device_secret).expect("secret");
    EventBuilder::new(kind, content)
        .custom_created_at(Timestamp::from_secs(secs(at)))
        .finalize(&Keys::new(signer))
        .expect("event")
}

#[tokio::test]
async fn the_own_record_carries_the_keys_and_ends_thirty_days_after_the_start_key() {
    let (anna, ben) = anna_and_ben().await;
    let event = record_of(&ben, 1_000 * DAY_MS, 1_005 * DAY_MS);
    assert_eq!(event.kind, KEY_RECORD_KIND);
    assert_eq!(
        event.pubkey.to_bytes(),
        ben.me.keys.device_pubkey,
        "signed by the device key"
    );
    assert_eq!(
        event.created_at.as_secs(),
        secs(1_005 * DAY_MS),
        "not backdated"
    );
    let until = secs(1_030 * DAY_MS);
    assert!(event
        .tags
        .iter()
        .any(|tag| *tag == Tag::expiration(Timestamp::from_secs(until))));

    let row = accept_on(&anna, &event, 1_006 * DAY_MS).expect("accepted");
    let installation = ben.me.installation;
    let keys = ben
        .db
        .read_blocking(move |r| {
            Ok(account::own_keys(
                &account::load(r, installation)?.expect("account").0,
            )?)
        })
        .expect("keys");
    assert_eq!(row.identity_key, keys.identity.to_base64());
    assert_eq!(row.start_key, keys.start.to_base64());
    assert_eq!(row.valid_until, millis(until));
    let peer = ben.me.device_hex();
    let stored = anna
        .db
        .read_blocking(move |r| Ok(store::record(r, &peer)?))
        .expect("read");
    assert_eq!(stored, Some(row));
}

#[tokio::test]
async fn a_reader_refuses_each_kind_of_bad_record() {
    let (anna, ben) = anna_and_ben().await;
    let now = 10 * DAY_MS;
    let good = record_of(&ben, now, now);

    assert_eq!(
        accept_on(&anna, &good, now + 31 * DAY_MS),
        Err(Refused::Expired),
        "past until by the reader's clock"
    );

    let stranger = ChatDevice::genesis(&[RELAY.to_owned()]);
    assert_eq!(
        accept_on(&anna, &record_of(&stranger, now, now), now),
        Err(Refused::UnknownDevice)
    );

    let content = good.content.clone();
    assert_eq!(
        accept_on(&anna, &signed_by(&ben, Kind::TextNote, &content, now), now),
        Err(Refused::WrongKind)
    );
    assert_eq!(
        accept_on(
            &anna,
            &signed_by(
                &ben,
                KEY_RECORD_KIND,
                &content.replace("\"v\":1", "\"v\":2"),
                now
            ),
            now
        ),
        Err(Refused::UnknownVersion)
    );
    assert_eq!(
        accept_on(
            &anna,
            &signed_by(&ben, KEY_RECORD_KIND, "{\"v\":1}", now),
            now
        ),
        Err(Refused::Malformed)
    );

    let mut json: serde_json::Value = serde_json::from_str(&good.as_json()).expect("json");
    json["content"] = serde_json::Value::String(content.replace("\"v\":1", "\"v\":1 "));
    let tampered: Event = serde_json::from_value(json).expect("an event");
    assert_eq!(accept_on(&anna, &tampered, now), Err(Refused::BadSignature));

    assert!(accept_on(&anna, &good, now).is_ok());
}

#[tokio::test]
async fn a_newer_record_replaces_and_an_older_one_handed_back_is_refused() {
    let (anna, ben) = anna_and_ben().await;
    let older = record_of(&ben, 0, DAY_MS);
    let newer = record_of(&ben, 0, 2 * DAY_MS);
    assert!(accept_on(&anna, &older, 2 * DAY_MS).is_ok());
    let row = accept_on(&anna, &newer, 2 * DAY_MS).expect("newer");
    assert_eq!(row.made_at, millis(secs(2 * DAY_MS)));
    assert_eq!(
        accept_on(&anna, &older, 3 * DAY_MS),
        Err(Refused::Older),
        "scenario 7, step 4"
    );
    assert!(
        accept_on(&anna, &newer, 3 * DAY_MS).is_ok(),
        "the same one again is fine"
    );
}

#[tokio::test]
async fn a_record_with_another_identity_key_drops_the_sessions_with_that_device() {
    let (anna, ben) = anna_and_ben().await;
    let first = accept_on(&anna, &record_of(&ben, 0, DAY_MS), DAY_MS).expect("first");
    let (me, peer) = (anna.me.clone(), ben.me.device_hex());
    let identity = vodozemac::Curve25519PublicKey::from_base64(&first.identity_key).expect("key");
    let start = vodozemac::Curve25519PublicKey::from_base64(&first.start_key).expect("key");
    anna.db
        .write_blocking(move |tx| {
            Ok(session::open_outbound(
                tx, &me, &peer, identity, start, DAY_MS,
            )?)
        })
        .expect("session");

    // Ben's device starts anew: a new account under the same device key.
    let mut reborn = vodozemac::olm::Account::new();
    reborn.generate_fallback_key();
    let event = own_event(&ben.me, &reborn, 2 * DAY_MS, 2 * DAY_MS).expect("record");
    let row = accept_on(&anna, &event, 2 * DAY_MS).expect("accepted");
    assert_ne!(row.identity_key, first.identity_key);
    let (installation, peer) = (anna.me.installation, ben.me.device_hex());
    let sessions = anna
        .db
        .read_blocking(move |r| Ok(store::sessions(r, installation, &peer)?))
        .expect("read");
    assert!(sessions.is_empty(), "the device started anew");
}
