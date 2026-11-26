//! The one conversation list (spec 072 US1, FR-001, FR-002, FR-005, FR-008) over a real vault with
//! the production migrations.

use haex_crdt::rusqlite::params;

use super::*;
use crate::contacts::card::format::{self, CardDevice, ContactCard, SignedCard, VERSION};
use crate::contacts::model::{ContactInput, Head};
use crate::contacts::service::test_support::{card_vault, TestVault};
use crate::contacts::service::Caller;
use crate::sync::keys::{self, hex};
use crate::sync::signing;

const BEN_SECRET: [u8; 32] = [21; 32];

fn ben_card() -> SignedCard {
    let card = ContactCard {
        version: VERSION,
        vault: signing::xonly_public_key(&BEN_SECRET).expect("a key"),
        issued_at: 1,
        list_generation: 1,
        devices: vec![CardDevice {
            device_pubkey: [3; 32],
            endpoint_id: [4; 32],
            iroh_relay: None,
        }],
        nostr_relays: vec!["wss://relay.example.org".to_owned()],
        name: "Ben Becker".to_owned(),
        values: vec![],
    };
    format::sign(card, &BEN_SECRET).expect("sign").0
}

/// A vault with a contact "Ben" holding Ben's card; answers (own vault hex, Ben's vault hex,
/// contact id).
async fn with_ben(vault: &TestVault) -> (String, String, String) {
    let contact = vault
        .service
        .create(
            &Caller::User,
            &[],
            ContactInput {
                head: Head {
                    given_name: Some("Ben".to_owned()),
                    ..Head::default()
                },
                values: vec![],
                groups: vec![],
                targets: vec![],
            },
        )
        .await
        .expect("contact");
    let contact_id = contact.id.clone();
    let card = ben_card();
    let ben = hex(&card.card.vault);
    let id = contact_id.clone();
    vault
        .db
        .write_blocking(move |tx| Ok(received::save(tx, &id, &card)?))
        .expect("card");
    let own = vault
        .db
        .read_blocking(|r| keys::vault_pubkey(r))
        .expect("read")
        .expect("vault identity");
    (hex(&own), ben, contact_id)
}

fn agent_thread(vault: &TestVault, title: &str, updated_at: i64) -> Uuid {
    let id = Uuid::new_v4();
    let title = title.to_owned();
    vault
        .db
        .write_blocking(move |tx| {
            agent_threads::insert_thread(
                tx,
                &agent_threads::ChatThread {
                    id,
                    title,
                    last_provider_id: None,
                    last_model_id: None,
                    created_at: updated_at,
                    updated_at,
                    origin: None,
                },
            )?;
            Ok(())
        })
        .expect("agent thread");
    id
}

fn peer_message(vault: &TestVault, thread: Uuid, sender: &str, text: &str, sort_at: i64) {
    let (sender, text) = (sender.to_owned(), text.to_owned());
    vault
        .db
        .write_blocking(move |tx| {
            tx.execute(
                "INSERT INTO chat_peer_messages (id, thread_id, event_id, sender_vault, \
                   sender_device, kind, text, sent_at, received_at, sort_at, inserted_by) \
                 VALUES (?1, ?2, ?3, ?4, 'dev', 'text', ?5, ?6, ?6, ?6, 'me')",
                params![
                    Uuid::new_v4().to_string(),
                    thread.to_string(),
                    Uuid::new_v4().simple().to_string(),
                    sender,
                    text,
                    sort_at
                ],
            )?;
            Ok(())
        })
        .expect("message");
}

fn list_all(vault: &TestVault, own: &str, filter: ListFilter) -> Vec<Conversation> {
    let own = own.to_owned();
    vault
        .db
        .read_blocking(move |r| Ok(list(r, &own, filter)?))
        .expect("list")
}

fn open(vault: &TestVault, own: &str, peer: &str, now: i64) -> Uuid {
    let (own, peer) = (own.to_owned(), peer.to_owned());
    vault
        .db
        .write_blocking(move |tx| Ok(open_direct(tx, &own, &peer, now)?))
        .expect("open")
}

#[tokio::test]
async fn agent_and_direct_conversations_share_one_list_newest_first() {
    let vault = card_vault();
    let (own, ben, contact_id) = with_ben(&vault).await;
    let old_agent = agent_thread(&vault, "Rezept", 10);
    let direct = open(&vault, &own, &ben, 20);
    let new_agent = agent_thread(&vault, "Reise", 30);

    let all = list_all(&vault, &own, ListFilter::default());
    let ids: Vec<_> = all.iter().map(|c| c.id.clone()).collect();
    assert_eq!(
        ids,
        [new_agent, direct, old_agent].map(|id| id.to_string()),
        "FR-001: newest activity first"
    );
    let entry = &all[1];
    assert_eq!(entry.kind, ConversationKind::Direct);
    assert_eq!(entry.peer_vault.as_deref(), Some(ben.as_str()));
    assert_eq!(entry.peer_contact_id.as_deref(), Some(contact_id.as_str()));
    assert_eq!(entry.peer_card_name.as_deref(), Some("Ben Becker"));
    assert_eq!(all[0].kind, ConversationKind::Agent);
    assert_eq!(all[0].title, "Reise");

    let agents = list_all(
        &vault,
        &own,
        ListFilter {
            kind: Some(ConversationKind::Agent),
            requests: false,
            blocked: false,
        },
    );
    assert_eq!(agents.len(), 2, "FR-002: filter by kind");
}

#[tokio::test]
async fn opening_a_direct_conversation_twice_gives_the_same_one() {
    let vault = card_vault();
    let (own, ben, _) = with_ben(&vault).await;
    let first = open(&vault, &own, &ben, 1);
    let second = open(&vault, &own, &ben, 2);
    assert_eq!(first, second);
    assert_eq!(
        first,
        ids::direct_thread_id(&ben, &own),
        "the same on Ben's side"
    );
    let members: i64 = vault
        .db
        .read_blocking(move |r| {
            Ok(r.query_row(
                "SELECT COUNT(*) FROM chat_members WHERE thread_id = ?1",
                params![first.to_string()],
                |row| row.get(0),
            )?
            .unwrap_or_default())
        })
        .expect("count");
    assert_eq!(members, 2);
}

#[tokio::test]
async fn a_direct_conversation_with_the_own_vault_is_refused() {
    let vault = card_vault();
    let (own, _, _) = with_ben(&vault).await;
    let own_again = own.clone();
    let result = vault
        .db
        .write_blocking(move |tx| Ok(open_direct(tx, &own, &own_again, 1)?));
    assert!(result.is_err());
}

#[tokio::test]
async fn unread_counts_only_others_messages_after_the_last_read() {
    let vault = card_vault();
    let (own, ben, _) = with_ben(&vault).await;
    let direct = open(&vault, &own, &ben, 1);
    peer_message(&vault, direct, &ben, "Hallo", 5);
    peer_message(&vault, direct, &own, "Hi", 6);
    peer_message(
        &vault,
        direct,
        &ben,
        "Kommst du heute Abend?\nIch koche.",
        7,
    );

    let entry = list_all(&vault, &own, ListFilter::default())
        .into_iter()
        .find(|c| c.id == direct.to_string())
        .expect("listed");
    assert_eq!(entry.unread, 2);
    assert_eq!(entry.last_text.as_deref(), Some("Kommst du heute Abend?"));
    assert_eq!(entry.last_at, 7);

    vault
        .db
        .write_blocking(move |tx| {
            tx.execute(
                "UPDATE chat_threads SET last_read_at = 6 WHERE id = ?1",
                params![direct.to_string()],
            )?;
            Ok(())
        })
        .expect("read up to 6");
    let entry = list_all(&vault, &own, ListFilter::default())
        .into_iter()
        .find(|c| c.id == direct.to_string())
        .expect("listed");
    assert_eq!(entry.unread, 1);
}

#[tokio::test]
async fn requests_have_their_own_area() {
    let vault = card_vault();
    let (own, ben, _) = with_ben(&vault).await;
    let direct = open(&vault, &own, &ben, 1);
    vault
        .db
        .write_blocking(move |tx| {
            tx.execute(
                "UPDATE chat_threads SET state = 'request' WHERE id = ?1",
                params![direct.to_string()],
            )?;
            Ok(())
        })
        .expect("request");
    assert!(list_all(&vault, &own, ListFilter::default()).is_empty());
    let requests = list_all(
        &vault,
        &own,
        ListFilter {
            kind: None,
            requests: true,
            blocked: false,
        },
    );
    assert_eq!(requests.len(), 1);
    assert_eq!(requests[0].state, ConversationState::Request);
}

#[tokio::test]
async fn deleting_removes_the_conversation_with_its_rows_and_agent_commands_never_see_it() {
    let vault = card_vault();
    let (own, ben, _) = with_ben(&vault).await;
    let direct = open(&vault, &own, &ben, 1);
    peer_message(&vault, direct, &ben, "Hallo", 5);

    let (agent_view, agent_list) = vault
        .db
        .read_blocking(move |r| {
            Ok((
                agent_threads::get_thread(r, direct)?,
                agent_threads::list_threads(r)?,
            ))
        })
        .expect("agent view");
    assert!(
        agent_view.is_none(),
        "agent commands do not see contact chats"
    );
    assert!(agent_list.is_empty());

    let muted = vault
        .db
        .write_blocking(move |tx| Ok(set_muted(tx, direct, true)?))
        .expect("mute");
    assert!(muted);
    assert!(list_all(&vault, &own, ListFilter::default())[0].muted);

    let deleted = vault
        .db
        .write_blocking(move |tx| Ok(delete(tx, direct)?))
        .expect("delete");
    assert!(deleted);
    let left: i64 = vault
        .db
        .read_blocking(move |r| {
            Ok(r.query_row(
                "SELECT (SELECT COUNT(*) FROM chat_peer_messages WHERE thread_id = ?1) \
                      + (SELECT COUNT(*) FROM chat_members WHERE thread_id = ?1) \
                      + (SELECT COUNT(*) FROM chat_threads WHERE id = ?1)",
                params![direct.to_string()],
                |row| row.get(0),
            )?
            .unwrap_or_default())
        })
        .expect("count");
    assert_eq!(left, 0);
    let again = vault
        .db
        .write_blocking(move |tx| Ok(delete(tx, direct)?))
        .expect("delete again");
    assert!(!again);
}

/// Spec 095 T034: deleting a conversation ends sending its events from this device.
#[tokio::test]
async fn deleting_a_conversation_removes_its_outbox_rows_and_stored_wraps() {
    use crate::chat::peer::outbox::{self, Rumor};

    let vault = card_vault();
    let (own, ben, _) = with_ben(&vault).await;
    let direct = open(&vault, &own, &ben, 1);
    let other = open(&vault, &own, &hex(&[7; 32]), 1);
    let wrap = {
        let keys = keys::DeviceKeys::generate();
        crate::chat::peer::wrap::wrap(
            nostr::event::Kind::Custom(24122),
            "{}",
            &keys.device_secret,
            &keys.device_pubkey,
            1_000,
        )
        .expect("wrap")
    };
    vault
        .db
        .write_blocking(move |tx| {
            let rumor = Rumor {
                kind: 24120,
                content: "{}".into(),
            };
            for (id, thread) in [("gone", direct), ("kept", other)] {
                outbox::enqueue(tx, id, &thread.to_string(), None, &rumor, &[], &[], 1)?;
                outbox::store_sealed(tx, id, "d", &wrap)?;
            }
            Ok(())
        })
        .expect("outbox");

    vault
        .db
        .write_blocking(move |tx| Ok(delete(tx, direct)?))
        .expect("delete");
    let left: Vec<String> = vault
        .db
        .read_blocking(|r| {
            r.query_map(
                "SELECT id FROM chat_outbox_no_sync UNION ALL \
                 SELECT outbox_id FROM chat_outbox_sealed_no_sync",
                &[],
                |row| row.get(0),
            )
        })
        .expect("rows");
    assert_eq!(left, ["kept", "kept"]);
}

#[tokio::test]
async fn deleting_an_agent_conversation_goes_through_the_agent_storage() {
    let vault = card_vault();
    let (own, _, _) = with_ben(&vault).await;
    let agent = agent_thread(&vault, "Rezept", 1);
    let deleted = vault
        .db
        .write_blocking(move |tx| Ok(delete(tx, agent)?))
        .expect("delete");
    assert!(deleted);
    assert!(list_all(&vault, &own, ListFilter::default()).is_empty());
}

fn request_times(vault: &TestVault, thread: Uuid) -> (Option<i64>, Option<i64>) {
    vault
        .db
        .read_blocking(move |r| {
            Ok(r.query_row(
                "SELECT request_first_at, request_last_at FROM chat_threads WHERE id = ?1",
                params![thread.to_string()],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )?
            .expect("thread"))
        })
        .expect("times")
}

fn knock(vault: &TestVault, thread: Uuid, peer: &str, now: i64) -> bool {
    let peer = peer.to_owned();
    vault
        .db
        .write_blocking(move |tx| Ok(mark_request(tx, thread, &peer, &ben_card(), now)?))
        .expect("knock")
}

fn requests(vault: &TestVault, own: &str) -> Vec<Conversation> {
    list_all(
        vault,
        own,
        ListFilter {
            kind: None,
            requests: true,
            blocked: false,
        },
    )
}

#[tokio::test]
async fn a_request_keeps_its_first_knock_and_lists_its_latest() {
    let vault = card_vault();
    let (own, ben, _) = with_ben(&vault).await;
    let direct = open(&vault, &own, &ben, 1);
    assert_eq!(request_times(&vault, direct), (None, None));

    assert!(knock(&vault, direct, &ben, 10), "it becomes a request");
    assert_eq!(request_times(&vault, direct), (Some(10), Some(10)));
    assert!(!knock(&vault, direct, &ben, 20), "a request already");
    assert_eq!(
        request_times(&vault, direct),
        (Some(10), Some(20)),
        "the first knock stays, the latest moves"
    );

    let listed = requests(&vault, &own);
    assert_eq!(listed.len(), 1);
    assert_eq!(listed[0].request_last_at, Some(20));
    assert_eq!(listed[0].last_at, 20, "ordered by the latest knock");
}

#[tokio::test]
async fn the_peer_has_answered_once_it_wrote_or_a_message_was_delivered() {
    let vault = card_vault();
    let (own, ben, _) = with_ben(&vault).await;
    let direct = open(&vault, &own, &ben, 1);
    let answered = |vault: &TestVault| {
        list_all(vault, &own, ListFilter::default())
            .into_iter()
            .find(|c| c.id == direct.to_string())
            .expect("listed")
            .peer_answered
    };
    assert!(!answered(&vault), "nothing yet");

    peer_message(&vault, direct, &own, "Hallo Ben", 5);
    let recipient = ben.clone();
    vault
        .db
        .write_blocking(move |tx| {
            tx.execute(
                "INSERT INTO chat_deliveries (id, message_id, recipient_vault) \
                 SELECT ?1, id, ?2 FROM chat_peer_messages WHERE thread_id = ?3",
                params![Uuid::new_v4().to_string(), recipient, direct.to_string()],
            )?;
            Ok(())
        })
        .expect("delivery row");
    assert!(!answered(&vault), "an own message only sent is no answer");

    vault
        .db
        .write_blocking(|tx| {
            tx.execute("UPDATE chat_deliveries SET delivered_at = 6", params![])?;
            Ok(())
        })
        .expect("delivered");
    assert!(answered(&vault), "a delivered message is an answer");

    let other = card_vault();
    let (other_own, other_ben, _) = with_ben(&other).await;
    let other_direct = open(&other, &other_own, &other_ben, 1);
    peer_message(&other, other_direct, &other_ben, "Hallo", 5);
    assert!(
        list_all(&other, &other_own, ListFilter::default())[0].peer_answered,
        "a message of the peer is an answer"
    );
}

/// Spec 099 T034 (FR-015, research R15): the conversation of a blocked contact leaves the list and
/// the chat search, the list of blocked conversations has it with `peer_blocked`, and unblocking
/// brings it back without a write to the conversation.
#[tokio::test]
async fn a_blocked_peers_conversation_leaves_the_list_and_the_search_until_unblocked() {
    let vault = card_vault();
    let (own, ben, contact_id) = with_ben(&vault).await;
    let thread = open(&vault, &own, &ben, 10);
    peer_message(&vault, thread, &ben, "Treffen am Markt", 20);
    let found = |vault: &TestVault| {
        vault
            .db
            .read_blocking(|r| Ok(crate::chat::search::search(r, "Markt")?))
            .expect("search")
            .hits
            .len()
    };
    let blocked_list = |vault: &TestVault| {
        list_all(
            vault,
            &own,
            ListFilter {
                kind: None,
                requests: false,
                blocked: true,
            },
        )
    };
    assert_eq!(found(&vault), 1);
    assert!(blocked_list(&vault).is_empty());

    vault
        .service
        .block_contact(&Caller::User, contact_id)
        .await
        .expect("block");
    assert!(!list_all(&vault, &own, ListFilter::default())
        .iter()
        .any(|c| c.id == thread.to_string()));
    assert_eq!(found(&vault), 0);
    let blocked = blocked_list(&vault);
    assert_eq!(blocked.len(), 1);
    assert_eq!(blocked[0].id, thread.to_string());
    assert!(blocked[0].peer_blocked);

    vault
        .service
        .set_blocked(&Caller::User, ben, false)
        .await
        .expect("unblock");
    let back = list_all(&vault, &own, ListFilter::default());
    let entry = back
        .iter()
        .find(|c| c.id == thread.to_string())
        .expect("back in the list");
    assert!(!entry.peer_blocked);
    assert_eq!(found(&vault), 1);
}

/// Spec 099 T039: a request of a blocked vault never shows (an own device that had not seen the
/// decline yet may have made one).
#[tokio::test]
async fn a_request_of_a_blocked_vault_never_shows() {
    let vault = card_vault();
    let own = vault
        .db
        .read_blocking(|r| keys::vault_pubkey(r))
        .expect("read")
        .map(|own| hex(&own))
        .expect("vault identity");
    let ben = hex(&ben_card().card.vault);
    let thread = open(&vault, &own, &ben, 10);
    assert!(knock(&vault, thread, &ben, 20));
    assert_eq!(requests(&vault, &own).len(), 1);
    let blocked = ben.clone();
    vault
        .db
        .write_blocking(move |tx| Ok(received::block(tx, &blocked)?))
        .expect("block");
    assert!(requests(&vault, &own).is_empty());
}
