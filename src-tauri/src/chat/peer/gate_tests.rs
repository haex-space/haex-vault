//! The answers to a sender (spec 099 research R5, R6, quickstart Q7): a noted answer becomes one
//! receipt on the outbox, a later answer replaces an earlier one, a blocked answer keeps its vault
//! reachable until a Nostr-Relay took it, and a received answer parks or resumes what waits.

use haex_crdt::rusqlite::params;

use super::*;
use crate::chat::peer::outbox::Entry;
use crate::chat::peer::test_support::ChatDevice;

const PEER: &str = "b2b2b2b2b2b2b2b2b2b2b2b2b2b2b2b2b2b2b2b2b2b2b2b2b2b2b2b2b2b2b2b2";

fn rows(device: &ChatDevice) -> Vec<Entry> {
    device
        .db
        .read_blocking(|r| Ok(outbox::due(r, i64::MAX - 1)?))
        .expect("due")
}

fn receipt_of(entry: &Entry) -> Receipt {
    serde_json::from_str(&entry.rumor.content).expect("a receipt")
}

#[test]
fn a_noted_answer_becomes_one_receipt_and_a_later_answer_replaces_it() {
    let anna = ChatDevice::genesis(&[]);
    anna.db
        .write_blocking(|tx| {
            write_notice(tx, PEER, Answer::Accepted, 1)?;
            tx.execute(
                "INSERT INTO haex_contacts_blocked_vaults (vault_pubkey, blocked_at) \
                 VALUES (?1, 'now')",
                params![PEER],
            )?;
            write_notice(tx, PEER, Answer::Blocked, 2)?;
            Ok(())
        })
        .expect("note");
    assert_eq!(
        anna.db.read_blocking(|r| Ok(notices(r)?)).expect("notes"),
        [(PEER.to_owned(), Answer::Blocked, 2)]
    );
    assert_eq!(
        anna.db
            .read_blocking(|r| Ok(blocked_notice_vaults(r)?))
            .expect("vaults"),
        [PEER.to_owned()],
        "noted: its devices stay reachable"
    );

    let me = anna.me.clone();
    let sent = anna
        .db
        .write_blocking(move |tx| Ok(send_due_notices(tx, &me, 3)?))
        .expect("send");
    assert_eq!(sent, 1);
    assert!(anna
        .db
        .read_blocking(|r| Ok(notices(r)?))
        .expect("notes")
        .is_empty());

    let rows = rows(&anna);
    assert_eq!(rows.len(), 1);
    assert!(is_blocked_notice(&rows[0].rumor));
    assert_eq!(rows[0].pending, [PEER.to_owned()]);
    let receipt = receipt_of(&rows[0]);
    assert_eq!(receipt.answer, Some(Answer::Blocked));
    assert_eq!(receipt.answer_sequence, Some(2));
    assert!(receipt.delivered.is_empty() && receipt.read.is_empty());
    assert_eq!(
        anna.db
            .read_blocking(|r| Ok(blocked_notice_vaults(r)?))
            .expect("vaults"),
        [PEER.to_owned()],
        "on the outbox: still reachable"
    );
}

#[test]
fn an_accepted_answer_is_no_blocked_notice() {
    let anna = ChatDevice::genesis(&[]);
    let me = anna.me.clone();
    anna.db
        .write_blocking(move |tx| {
            write_notice(tx, PEER, Answer::Accepted, 1)?;
            send_due_notices(tx, &me, 2)?;
            Ok(())
        })
        .expect("send");
    let rows = rows(&anna);
    assert_eq!(rows.len(), 1);
    assert!(!is_blocked_notice(&rows[0].rumor));
    assert!(anna
        .db
        .read_blocking(|r| Ok(blocked_notice_vaults(r)?))
        .expect("vaults")
        .is_empty());
}

#[test]
fn a_newer_answer_replaces_an_older_one_no_nostr_relay_took_yet() {
    let anna = ChatDevice::genesis(&[]);
    for (answer, at) in [(Answer::Accepted, 1), (Answer::Blocked, 2)] {
        let me = anna.me.clone();
        anna.db
            .write_blocking(move |tx| {
                if answer == Answer::Blocked {
                    tx.execute(
                        "INSERT INTO haex_contacts_blocked_vaults (vault_pubkey, blocked_at) \
                         VALUES (?1, 'now')",
                        params![PEER],
                    )?;
                } else {
                    tx.execute(
                        "DELETE FROM haex_contacts_blocked_vaults WHERE vault_pubkey = ?1",
                        params![PEER],
                    )?;
                }
                write_notice(tx, PEER, answer, at)?;
                send_due_notices(tx, &me, at)?;
                Ok(())
            })
            .expect("send");
    }
    let rows = rows(&anna);
    assert_eq!(rows.len(), 1, "the older answer is gone: {rows:?}");
    assert_eq!(receipt_of(&rows[0]).answer, Some(Answer::Blocked));
    assert_eq!(receipt_of(&rows[0]).answer_sequence, Some(2));
}

#[test]
fn a_synced_unblock_drops_a_queued_blocked_notice_and_a_stale_local_note() {
    let anna = ChatDevice::genesis(&[]);
    anna.db
        .write_blocking(|tx| {
            tx.execute(
                "INSERT INTO haex_contacts_blocked_vaults (vault_pubkey, blocked_at) \
                 VALUES (?1, 'now')",
                params![PEER],
            )?;
            write_notice(tx, PEER, Answer::Blocked, 1)?;
            Ok(())
        })
        .expect("block");
    let me = anna.me.clone();
    anna.db
        .write_blocking(move |tx| Ok(send_due_notices(tx, &me, 2)?))
        .expect("queue blocked notice");
    assert_eq!(rows(&anna).len(), 1);

    // A second own device unblocks the peer and syncs the block-list removal. The old notice and
    // its outbox row are device-local, so the sender must reconcile them against this shared state.
    anna.db
        .write_blocking(|tx| {
            tx.execute(
                "DELETE FROM haex_contacts_blocked_vaults WHERE vault_pubkey = ?1",
                params![PEER],
            )?;
            Ok(())
        })
        .expect("synced unblock");
    anna.db
        .write_blocking(|tx| {
            drop_stale_notice_outbox(tx, i64::MAX)?;
            Ok(())
        })
        .expect("discard stale outbox notice");
    assert!(rows(&anna).is_empty(), "the old blocked answer is dropped");

    anna.db
        .write_blocking(|tx| {
            write_notice(tx, PEER, Answer::Blocked, 3)?;
            Ok(())
        })
        .expect("stale local note");
    let me = anna.me.clone();
    anna.db
        .write_blocking(move |tx| Ok(send_due_notices(tx, &me, 4)?))
        .expect("discard stale local note");
    assert!(rows(&anna).is_empty(), "the stale local note is not sent");
}

#[tokio::test(flavor = "multi_thread")]
async fn a_received_answer_parks_and_resumes_and_one_for_another_vault_changes_nothing() {
    let anna = ChatDevice::genesis(&[]);
    let ben = ChatDevice::genesis(&[]);
    ben.learn(&anna).await;
    let message = ben.send(&anna, "Hallo Anna");
    let ben_me = ben.me.clone();
    let anna_vault = anna.me.vault_hex.clone();
    let answer = |answer: Answer, thread: String, sequence: i64| Receipt {
        v: crate::chat::peer::envelope::VERSION,
        from: anna_vault.clone(),
        card: "card".into(),
        thread: To::Direct(thread),
        delivered: vec![],
        read: vec![],
        answer: Some(answer),
        answer_sequence: Some(sequence),
    };
    let sender_device = anna.me.device_hex();
    let apply = |receipt: Receipt, now: i64| {
        let me = ben_me.clone();
        let sender_device = sender_device.clone();
        ben.db
            .write_blocking(move |tx| Ok(apply_received(tx, &me, &receipt, &sender_device, now)?))
            .expect("apply");
    };
    let blocked = |ben: &ChatDevice| {
        let (own, peer) = (ben.me.vault_hex.clone(), anna.me.vault_hex.clone());
        ben.db
            .read_blocking(move |r| Ok(blocked_by_peer(r, &own, &peer)?))
            .expect("read")
    };
    let reason = |ben: &ChatDevice| {
        let id = ids::delivery_id(message, &anna.me.vault_hex).to_string();
        ben.db
            .read_blocking(move |r| {
                Ok(r.query_row(
                    "SELECT failed_reason FROM chat_deliveries WHERE id = ?1",
                    params![id],
                    |row| row.get::<_, Option<String>>(0),
                )?
                .flatten())
            })
            .expect("read")
    };

    // Quickstart Q7: an answer about another conversation is no answer to this vault.
    apply(answer(Answer::Blocked, PEER.to_owned(), 1), 10);
    assert!(!blocked(&ben));

    apply(answer(Answer::Blocked, ben.me.vault_hex.clone(), 2), 20);
    assert!(blocked(&ben));
    assert_eq!(reason(&ben).as_deref(), Some("blocked"));
    assert!(
        rows(&ben)
            .iter()
            .all(|row| !row.pending.contains(&anna.me.vault_hex)),
        "nothing is sent to Anna any more"
    );

    apply(answer(Answer::Accepted, ben.me.vault_hex.clone(), 3), 30);
    assert!(!blocked(&ben));
    apply(answer(Answer::Blocked, PEER.to_owned(), 1), 40);
    assert!(
        !blocked(&ben),
        "an older answer from the same device is ignored"
    );
    assert_eq!(reason(&ben), None);
    assert!(
        ben.db
            .read_blocking(|r| Ok(outbox::due(r, 30)?))
            .expect("due")
            .iter()
            .any(|row| row.pending.contains(&anna.me.vault_hex)),
        "due for Anna at once"
    );
}
