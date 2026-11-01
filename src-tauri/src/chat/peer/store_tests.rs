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
