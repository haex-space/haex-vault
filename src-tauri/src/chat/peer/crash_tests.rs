//! Crash safety of sending and receiving on device sessions (spec 095 T017, quickstart scenario 9,
//! steps 1 and 2): a failure before, inside and after the transaction that saves a session. Inside
//! is a write transaction that fails after the step; after is a step that committed while what
//! follows it (publishing, the next fetch) never ran. Afterwards session and rows agree: a stored
//! wrap exists exactly when the session advanced for it, a stored message exists exactly when its
//! wrap is marked seen, and the conversation goes on.

use nostr::event::Event;

use super::receiving::{prepare, receive_one};
use super::sending::seal;
use crate::chat::peer::outbox::{self, Entry};
use crate::chat::peer::ratchet::store;
use crate::chat::peer::test_support::{two_vaults, ChatDevice};
use crate::error::HaexError;
use crate::storage::query::Query;

fn injected() -> HaexError {
    HaexError::InvalidInput {
        reason: "injected failure".into(),
    }
}

/// The pickles of `device`'s sessions with `peer`.
fn sessions(device: &ChatDevice, peer: &ChatDevice) -> Vec<String> {
    let (installation, peer) = (device.me.installation, peer.me.device_hex());
    device
        .db
        .read_blocking(move |r| {
            Ok(store::sessions(r, installation, &peer)?
                .into_iter()
                .map(|row| row.session.to_string())
                .collect())
        })
        .expect("sessions")
}

fn queued(device: &ChatDevice) -> Vec<Entry> {
    device
        .db
        .read_blocking(|r| Ok(outbox::due(r, i64::MAX - 1)?))
        .expect("outbox")
}

fn stored_wrap(device: &ChatDevice, entry: &Entry, peer: &ChatDevice) -> Option<Event> {
    let (id, peer) = (entry.id.clone(), peer.me.device_hex());
    device
        .db
        .read_blocking(move |r| Ok(outbox::sealed(r, &id, &peer)?))
        .expect("sealed")
        .map(|sealed| sealed.wrap)
}

/// Seals `entry` for `to` in a transaction that fails afterwards when `fail` is set.
fn seal_for(from: &ChatDevice, entry: &Entry, to: &ChatDevice, fail: bool) -> Option<Event> {
    let (me, entry, device, now) = (
        from.me.clone(),
        entry.clone(),
        to.me.keys.device_pubkey,
        from.clock.now(),
    );
    let result = from.db.write_blocking(move |tx| {
        let wrap = seal(tx, &me, &entry, &device, now)?;
        if fail {
            return Err(injected().into());
        }
        Ok(wrap)
    });
    if fail {
        assert!(result.is_err());
        return None;
    }
    result.expect("seal")
}

/// Receives `wrap` on `device` in one transaction that fails afterwards when `fail` is set;
/// answers whether a new message was stored.
fn receive_on(device: &ChatDevice, wrap: &Event, fail: bool) -> bool {
    let now = device.clock.now();
    let prepared = prepare(wrap, None, &device.me.keys.device_secret, now, 0);
    let me = device.me.clone();
    let result = device.db.write_blocking(move |tx| {
        let stored = receive_one(tx, &me, prepared, now)?;
        if fail {
            return Err(injected().into());
        }
        Ok(stored.is_some_and(|(stored, _)| stored.new))
    });
    if fail {
        assert!(result.is_err());
        return false;
    }
    result.expect("receive")
}

fn seen(device: &ChatDevice, wrap: &Event) -> bool {
    let id = wrap.id.to_hex();
    device
        .db
        .read_blocking(move |r| {
            Ok(r.query_row(
                "SELECT 1 FROM chat_wraps_seen_no_sync WHERE wrap_id = ?1",
                haex_crdt::rusqlite::params![id],
                |row| row.get::<_, i64>(0),
            )?
            .is_some())
        })
        .expect("seen")
}

#[tokio::test]
async fn a_failure_around_sealing_leaves_the_session_and_the_stored_wrap_in_step() {
    let (anna, ben) = two_vaults(&[]).await;
    anna.introduce(&ben);
    anna.send(&ben, "eins");
    let first = queued(&anna).remove(0);

    // Before: nothing ran, nothing is there.
    assert!(sessions(&anna, &ben).is_empty());
    assert!(stored_wrap(&anna, &first, &ben).is_none());

    // Inside: the transaction fails after sealing; neither the session nor the wrap stays.
    seal_for(&anna, &first, &ben, true);
    assert!(sessions(&anna, &ben).is_empty());
    assert!(stored_wrap(&anna, &first, &ben).is_none());

    // After: sealed and saved, but the vault closed before publishing. The next attempt finds the
    // same wrap and the session is not advanced a second time.
    let sealed = seal_for(&anna, &first, &ben, false).expect("a wrap");
    let advanced = sessions(&anna, &ben);
    assert_eq!(advanced.len(), 1);
    let again = seal_for(&anna, &first, &ben, false).expect("the stored wrap");
    assert_eq!(again.id, sealed.id);
    assert_eq!(sessions(&anna, &ben), advanced);

    // An existing session: a failure inside leaves it as it was.
    anna.send(&ben, "zwei");
    let second = queued(&anna)
        .into_iter()
        .find(|entry| entry.id != first.id)
        .expect("second row");
    seal_for(&anna, &second, &ben, true);
    assert_eq!(sessions(&anna, &ben), advanced);
    assert!(stored_wrap(&anna, &second, &ben).is_none());

    // The conversation goes on: both arrive once.
    let second = seal_for(&anna, &second, &ben, false).expect("a wrap");
    assert!(receive_on(&ben, &sealed, false));
    assert!(receive_on(&ben, &second, false));
    assert_eq!(ben.messages_with(&anna).len(), 2);
}

#[tokio::test]
async fn a_failure_around_receiving_leaves_the_session_and_the_message_in_step() {
    let (anna, ben) = two_vaults(&[]).await;
    anna.introduce(&ben);
    anna.send(&ben, "eins");
    let entry = queued(&anna).remove(0);
    let wrap = seal_for(&anna, &entry, &ben, false).expect("a wrap");

    // Before: the wrap was fetched but its transaction never ran.
    assert!(!seen(&ben, &wrap));

    // Inside: the transaction fails after storing; no message, not seen, no session.
    assert!(!receive_on(&ben, &wrap, true));
    assert!(ben.messages_with(&anna).is_empty());
    assert!(!seen(&ben, &wrap));
    assert!(sessions(&ben, &anna).is_empty());

    // After: stored and committed; the wrap comes again (the next catch-up) and stores nothing.
    assert!(receive_on(&ben, &wrap, false));
    let session = sessions(&ben, &anna);
    assert_eq!(session.len(), 1);
    assert!(seen(&ben, &wrap));
    assert!(!receive_on(&ben, &wrap, false));
    assert_eq!(ben.messages_with(&anna).len(), 1);
    assert_eq!(sessions(&ben, &anna), session, "not advanced by the copy");

    // An existing session: a failure inside leaves it as it was, and the conversation goes on.
    anna.send(&ben, "zwei");
    let second = queued(&anna)
        .into_iter()
        .find(|next| next.id != entry.id)
        .expect("second row");
    let second = seal_for(&anna, &second, &ben, false).expect("a wrap");
    assert!(!receive_on(&ben, &second, true));
    assert_eq!(sessions(&ben, &anna), session);
    assert!(receive_on(&ben, &second, false));
    assert_eq!(ben.messages_with(&anna).len(), 2);
}
