//! Sessions between two devices (spec 095 T009): round trips, opening messages, skipped and repeated
//! messages, simultaneous openings, the session limit, `renewed_at` against a copied key state
//! (FR-022, SC-010 at module level) and operations that run at the same time.

use uuid::Uuid;
use vodozemac::olm::{OlmMessage, SessionPickle};

use super::*;
use crate::chat::peer::ratchet::account::{self, OwnKeys};
use crate::chat::peer::ratchet::store;
use crate::chat::peer::test_support::ChatDevice;

/// The keys `device` would publish in its record.
fn record_of(device: &ChatDevice) -> OwnKeys {
    let installation = device.me.installation;
    device
        .db
        .write_blocking(move |tx| {
            Ok(account::own_keys(&account::load_or_create(
                tx,
                installation,
                0,
            )?)?)
        })
        .expect("account")
}

/// `from` opens a session with `to` from `to`'s record at `now`.
fn open(from: &ChatDevice, to: &ChatDevice, now: i64) -> String {
    let keys = record_of(to);
    let (me, peer) = (from.me.clone(), to.me.device_hex());
    from.db
        .write_blocking(move |tx| {
            Ok(open_outbound(
                tx,
                &me,
                &peer,
                keys.identity,
                keys.start,
                now,
            )?)
        })
        .expect("open")
}

/// `from` encrypts `text` for `to` at `now`.
fn send(from: &ChatDevice, to: &ChatDevice, text: &str, now: i64) -> OlmMessage {
    let (me, peer, text) = (from.me.clone(), to.me.device_hex(), text.to_owned());
    from.db
        .write_blocking(move |tx| Ok(encrypt(tx, &me, &peer, text.as_bytes(), now)?))
        .expect("encrypt")
        .expect("a session")
        .message()
        .expect("message")
}

/// `to` decrypts `message` from `from` at `now` and confirms it; `None` when undecryptable.
fn receive(to: &ChatDevice, from: &ChatDevice, message: &OlmMessage, now: i64) -> Option<String> {
    let (me, peer, message) = (to.me.clone(), from.me.device_hex(), message.clone());
    to.db
        .write_blocking(move |tx| {
            Ok(match decrypt(tx, &me, &peer, &message, now)? {
                Decrypt::Decrypted(decrypted) => {
                    let text = String::from_utf8(decrypted.plaintext().to_vec()).expect("utf-8");
                    decrypted.confirm(tx, &me)?;
                    Some(text)
                }
                Decrypt::Undecryptable => None,
            })
        })
        .expect("decrypt")
}

fn sessions_of(device: &ChatDevice, peer: &ChatDevice) -> Vec<store::SessionRow> {
    let (installation, peer) = (device.me.installation, peer.me.device_hex());
    device
        .db
        .read_blocking(move |r| Ok(store::sessions(r, installation, &peer)?))
        .expect("sessions")
}

fn is_opening(message: &OlmMessage) -> bool {
    matches!(message, OlmMessage::PreKey(_))
}

#[test]
fn outbound_session_records_the_initial_key_time_at_open() {
    let (anna, ben) = (ChatDevice::genesis(&[]), ChatDevice::genesis(&[]));
    open(&anna, &ben, 1_000);
    let opened = sessions_of(&anna, &ben).remove(0);
    assert_eq!(opened.own_key_since, Some(1_000));
    assert!(opened.own_ratchet_key.is_some());

    send(&anna, &ben, "später gesendet", 3_000);
    let sent = sessions_of(&anna, &ben).remove(0);
    assert_eq!(sent.own_key_since, Some(1_000));
}

#[test]
fn confirming_decryption_cannot_overwrite_a_session_advanced_after_decrypt() {
    let (anna, ben) = talking();
    let incoming = send(&anna, &ben, "eingehend", 10);
    let (me, peer) = (ben.me.clone(), anna.me.device_hex());
    let err = ben
        .db
        .write_blocking(move |tx| {
            let Decrypt::Decrypted(decrypted) = decrypt(tx, &me, &peer, &incoming, 11)? else {
                panic!("message decrypts")
            };
            encrypt(tx, &me, &peer, b"outgoing", 12)?;
            Ok(decrypted.confirm(tx, &me)?)
        })
        .expect_err("a stale decrypted pickle must not replace the newer send state");
    assert!(err.to_string().contains("session changed while decrypting"));
}

/// Two devices with a session on which each received once.
fn talking() -> (ChatDevice, ChatDevice) {
    let (anna, ben) = (ChatDevice::genesis(&[]), ChatDevice::genesis(&[]));
    open(&anna, &ben, 1);
    let hello = send(&anna, &ben, "hallo", 1);
    assert_eq!(receive(&ben, &anna, &hello, 2).as_deref(), Some("hallo"));
    let answer = send(&ben, &anna, "hallo zurück", 3);
    assert_eq!(
        receive(&anna, &ben, &answer, 4).as_deref(),
        Some("hallo zurück")
    );
    (anna, ben)
}

#[test]
fn messages_go_both_ways_on_one_session() {
    let (anna, ben) = (ChatDevice::genesis(&[]), ChatDevice::genesis(&[]));
    let id = open(&anna, &ben, 1);
    let hello = send(&anna, &ben, "hallo", 1);
    assert!(is_opening(&hello));
    assert_eq!(receive(&ben, &anna, &hello, 2).as_deref(), Some("hallo"));
    let answer = send(&ben, &anna, "hallo zurück", 3);
    assert!(!is_opening(&answer), "the receiving side answers normally");
    assert_eq!(
        receive(&anna, &ben, &answer, 4).as_deref(),
        Some("hallo zurück")
    );
    let again = send(&anna, &ben, "wieder", 5);
    assert!(!is_opening(&again), "after an answer no more openings");
    assert_eq!(receive(&ben, &anna, &again, 6).as_deref(), Some("wieder"));

    let (on_anna, on_ben) = (sessions_of(&anna, &ben), sessions_of(&ben, &anna));
    assert_eq!(on_anna.len(), 1);
    assert_eq!(on_ben.len(), 1);
    assert_eq!(on_anna[0].session_id, id);
    assert_eq!(on_ben[0].session_id, id, "both ends name it the same");
    assert_eq!(on_anna[0].last_sent_at, Some(5));
    assert_eq!(on_ben[0].last_received_at, Some(6));
}

#[test]
fn a_device_sends_opening_messages_until_it_decrypted_an_answer() {
    let (anna, ben) = (ChatDevice::genesis(&[]), ChatDevice::genesis(&[]));
    open(&anna, &ben, 1);
    let first = send(&anna, &ben, "eins", 1);
    let second = send(&anna, &ben, "zwei", 2);
    assert!(is_opening(&first) && is_opening(&second));
    assert_eq!(
        receive(&ben, &anna, &second, 3).as_deref(),
        Some("zwei"),
        "the later opening creates the session"
    );
    assert_eq!(
        receive(&ben, &anna, &first, 4).as_deref(),
        Some("eins"),
        "the earlier one decrypts on it"
    );
    assert_eq!(sessions_of(&ben, &anna).len(), 1);
    let answer = send(&ben, &anna, "da", 5);
    receive(&anna, &ben, &answer, 6).expect("answer");
    assert!(!is_opening(&send(&anna, &ben, "drei", 7)));
}

#[test]
fn forty_skipped_messages_decrypt_later_and_a_gap_of_forty_one_loses_the_first() {
    let (anna, ben) = talking();
    let burst: Vec<OlmMessage> = (0..41)
        .map(|i| send(&anna, &ben, &format!("a{i}"), 10))
        .collect();
    assert!(receive(&ben, &anna, &burst[40], 11).is_some());
    for (i, message) in burst.iter().enumerate().take(40) {
        assert_eq!(
            receive(&ben, &anna, message, 12).as_deref(),
            Some(format!("a{i}").as_str()),
            "skipped message {i}"
        );
    }

    let burst: Vec<OlmMessage> = (0..42)
        .map(|i| send(&anna, &ben, &format!("b{i}"), 20))
        .collect();
    assert!(receive(&ben, &anna, &burst[41], 21).is_some());
    assert_eq!(
        receive(&ben, &anna, &burst[0], 22),
        None,
        "41 skipped: the first is gone"
    );
    for message in &burst[1..41] {
        assert!(receive(&ben, &anna, message, 22).is_some());
    }
}

#[test]
fn a_message_delivered_twice_is_undecryptable_the_second_time() {
    let (anna, ben) = talking();
    let message = send(&anna, &ben, "einmal", 10);
    assert!(receive(&ben, &anna, &message, 11).is_some());
    assert_eq!(receive(&ben, &anna, &message, 12), None);
    let opening = {
        let (carla, dora) = (ChatDevice::genesis(&[]), ChatDevice::genesis(&[]));
        open(&carla, &dora, 1);
        let opening = send(&carla, &dora, "auf", 1);
        assert!(receive(&dora, &carla, &opening, 2).is_some());
        assert_eq!(
            receive(&dora, &carla, &opening, 3),
            None,
            "also an opening on its own session"
        );
        opening
    };
    assert!(is_opening(&opening));
}

#[test]
fn an_unconfirmed_message_leaves_no_session_behind() {
    let (anna, ben) = (ChatDevice::genesis(&[]), ChatDevice::genesis(&[]));
    open(&anna, &ben, 1);
    let hello = send(&anna, &ben, "hallo", 1);
    let (me, peer) = (ben.me.clone(), anna.me.device_hex());
    let opened = ben
        .db
        .write_blocking(move |tx| {
            Ok(match decrypt(tx, &me, &peer, &hello, 2)? {
                Decrypt::Decrypted(decrypted) => decrypted.opened(),
                Decrypt::Undecryptable => false,
            })
        })
        .expect("decrypt");
    assert!(opened, "it would open a session");
    assert!(
        sessions_of(&ben, &anna).is_empty(),
        "but it was not confirmed"
    );
}

/// Two devices open sessions with each other at the same moment; each receives the other's opening
/// before or after it sent its own, then both send on the session with the smallest id.
#[test]
fn simultaneous_openings_converge_on_the_smallest_session_id() {
    for anna_first in [true, false] {
        let (anna, ben) = (ChatDevice::genesis(&[]), ChatDevice::genesis(&[]));
        let from_anna = open(&anna, &ben, 1);
        let from_ben = open(&ben, &anna, 1);
        let a1 = send(&anna, &ben, "von anna", 2);
        let b1 = send(&ben, &anna, "von ben", 2);
        if anna_first {
            assert!(receive(&anna, &ben, &b1, 3).is_some());
            assert!(receive(&ben, &anna, &a1, 4).is_some());
        } else {
            assert!(receive(&ben, &anna, &a1, 3).is_some());
            assert!(receive(&anna, &ben, &b1, 4).is_some());
        }
        let smallest = from_anna.clone().min(from_ben.clone());
        let a2 = send(&anna, &ben, "anna wieder", 5);
        let b2 = send(&ben, &anna, "ben wieder", 5);
        let sent_on = |device: &ChatDevice, peer: &ChatDevice| {
            sessions_of(device, peer)
                .into_iter()
                .find(|row| row.last_sent_at == Some(5))
                .expect("the session it sent on")
                .session_id
        };
        assert_eq!(sent_on(&anna, &ben), smallest);
        assert_eq!(sent_on(&ben, &anna), smallest);
        assert_eq!(receive(&ben, &anna, &a2, 6).as_deref(), Some("anna wieder"));
        assert_eq!(receive(&anna, &ben, &b2, 6).as_deref(), Some("ben wieder"));
    }
}

#[test]
fn a_fifth_session_removes_the_least_recently_used() {
    let (anna, ben) = (ChatDevice::genesis(&[]), ChatDevice::genesis(&[]));
    let ids: Vec<String> = (0..5).map(|at| open(&anna, &ben, at)).collect();
    let kept: Vec<String> = sessions_of(&anna, &ben)
        .into_iter()
        .map(|row| row.session_id)
        .collect();
    assert_eq!(kept.len(), MAX_SESSIONS);
    assert!(!kept.contains(&ids[0]), "the oldest unused one went");
    for id in &ids[1..] {
        assert!(kept.contains(id));
    }
}

#[test]
fn an_opening_with_another_identity_key_replaces_the_sessions_with_that_device() {
    let (anna, ben) = talking();
    // Ben's device starts anew (a lost vault restored without its account): a new account under the
    // same device key opens a session with Anna.
    let reborn = ChatDevice::genesis(&[]);
    let keys = record_of(&anna);
    let (me, anna_hex) = (reborn.me.clone(), anna.me.device_hex());
    let opening = reborn
        .db
        .write_blocking(move |tx| {
            open_outbound(tx, &me, &anna_hex, keys.identity, keys.start, 10)?;
            Ok(encrypt(tx, &me, &anna_hex, b"neu", 10)?)
        })
        .expect("open")
        .expect("session")
        .message()
        .expect("message");
    let old = sessions_of(&anna, &ben);
    assert_eq!(receive(&anna, &ben, &opening, 11).as_deref(), Some("neu"));
    let now = sessions_of(&anna, &ben);
    assert_eq!(now.len(), 1, "the sessions of the old identity are gone");
    assert_ne!(now[0].session_id, old[0].session_id);
}

/// FR-022, SC-010 at module level: with Ben's session copied at T, `renewed_at` stays at or before T
/// until Ben sent with a ratchet key made after T and received an answer with a new ratchet key of
/// Anna; from then on the copy decrypts nothing.
#[test]
fn renewed_at_passes_a_copy_only_after_both_sides_contributed_new_ratchet_keys() {
    let (anna, ben) = (ChatDevice::genesis(&[]), ChatDevice::genesis(&[]));
    open(&anna, &ben, 1_000);
    let hello = send(&anna, &ben, "hallo", 1_000);
    receive(&ben, &anna, &hello, 1_500).expect("hallo");
    let answer = send(&ben, &anna, "antwort", 2_000);
    receive(&anna, &ben, &answer, 2_500).expect("antwort");
    assert_eq!(sessions_of(&anna, &ben)[0].renewed_at, Some(1_000));

    let copied_at = 3_000;
    let copy = sessions_of(&ben, &anna)[0].session.to_string();
    let mut copy =
        Session::from_pickle(serde_json::from_str::<SessionPickle>(&copy).expect("pickle"));
    let renewed_at = |ben: &ChatDevice| sessions_of(ben, &anna)[0].renewed_at;

    let before_ben_renewed = send(&anna, &ben, "noch lesbar", 4_000);
    receive(&ben, &anna, &before_ben_renewed, 4_000).expect("received");
    assert!(
        renewed_at(&ben).is_some_and(|at| at <= copied_at),
        "Anna answered Ben's key from before the copy"
    );
    assert!(
        copy.decrypt(&before_ben_renewed).is_ok(),
        "the copy still reads it, and renewed_at says so"
    );

    let ben_new_key = send(&ben, &anna, "neuer schlüssel", 5_000);
    receive(&anna, &ben, &ben_new_key, 5_500).expect("received");
    assert!(
        renewed_at(&ben).is_some_and(|at| at <= copied_at),
        "Ben's new key is not answered yet"
    );

    let answered = send(&anna, &ben, "beantwortet", 6_000);
    receive(&ben, &anna, &answered, 6_000).expect("received");
    assert_eq!(renewed_at(&ben), Some(5_000), "past the copy");
    assert!(copy.decrypt(&answered).is_err(), "the copy is overcome");
    let later = send(&anna, &ben, "später", 7_000);
    assert!(copy.decrypt(&later).is_err());
    assert!(receive(&ben, &anna, &later, 7_000).is_some());
}

/// A message delayed on an older chain of the peer arrives after newer ones: it is no answer to
/// this device's current ratchet key and moves nothing (a rule that compared the message's ratchet
/// key with the last one seen would count it).
#[test]
fn a_delayed_message_on_an_older_chain_moves_nothing() {
    let (anna, ben) = talking();
    let delayed = send(&anna, &ben, "verspätet", 10);
    let on_time = send(&anna, &ben, "pünktlich", 10);
    receive(&ben, &anna, &on_time, 11).expect("on time");
    let from_ben = send(&ben, &anna, "ben", 20);
    receive(&anna, &ben, &from_ben, 21).expect("ben");
    let newer = send(&anna, &ben, "neuere kette", 30);
    receive(&ben, &anna, &newer, 31).expect("newer");
    assert_eq!(sessions_of(&ben, &anna)[0].renewed_at, Some(20));

    send(&ben, &anna, "noch ein neuer schlüssel", 40);
    assert_eq!(
        receive(&ben, &anna, &delayed, 41).as_deref(),
        Some("verspätet"),
        "it still decrypts"
    );
    assert_eq!(
        sessions_of(&ben, &anna)[0].renewed_at,
        Some(20),
        "Anna has not seen Ben's key of 40"
    );
}

/// 50 sends and receives issued at the same time through the vault's write path lose no advance:
/// every message decrypts on the other side.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn concurrent_operations_lose_no_advance() {
    let (anna, ben) = talking();
    let from_ben: Vec<OlmMessage> = (0..20)
        .map(|i| send(&ben, &anna, &format!("b{i}"), 100))
        .collect();

    let mut tasks = Vec::new();
    for i in 0..30 {
        let (db, me, peer) = (anna.db.clone(), anna.me.clone(), ben.me.device_hex());
        tasks.push(tokio::spawn(async move {
            let text = if i % 3 == 0 {
                String::new()
            } else {
                format!("a{i}")
            };
            db.write(move |tx| Ok(encrypt(tx, &me, &peer, text.as_bytes(), 200)?))
                .await
                .expect("encrypt")
                .map(|rumor| rumor.message().expect("message"))
        }));
    }
    let mut receives = Vec::new();
    for message in from_ben {
        let (db, me, peer) = (anna.db.clone(), anna.me.clone(), ben.me.device_hex());
        receives.push(tokio::spawn(async move {
            db.write(move |tx| {
                Ok(match decrypt(tx, &me, &peer, &message, 200)? {
                    Decrypt::Decrypted(decrypted) => {
                        decrypted.confirm(tx, &me)?;
                        true
                    }
                    Decrypt::Undecryptable => false,
                })
            })
            .await
            .expect("decrypt")
        }));
    }
    let mut from_anna = Vec::new();
    for task in tasks {
        from_anna.push(task.await.expect("join").expect("a session"));
    }
    for task in receives {
        assert!(task.await.expect("join"), "every message of Ben decrypts");
    }

    let decrypted = from_anna
        .iter()
        .filter(|message| receive(&ben, &anna, message, 300).is_some())
        .count();
    assert_eq!(decrypted, 30, "every message of Anna decrypts");
}

#[test]
fn sessions_belong_to_this_installation_only() {
    let (anna, ben) = talking();
    let other = Uuid::new_v4();
    let peer = ben.me.device_hex();
    let none = anna
        .db
        .read_blocking(move |r| Ok(store::sessions(r, other, &peer)?))
        .expect("read");
    assert!(none.is_empty(), "another installation sees no session");
}
