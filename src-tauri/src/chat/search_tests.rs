//! The sidebar's message search (spec 076 US2, FR-011, FR-013) over a real vault with the production
//! migrations.

use haex_crdt::rusqlite::params;
use uuid::Uuid;

use super::*;
use crate::contacts::service::test_support::{test_vault, TestVault};
use crate::storage::chat_threads::{self as agent_threads, ChatThread};

/// A conversation row of `kind` in `state`.
fn thread(vault: &TestVault, kind: &str, state: &str) -> Uuid {
    let id = Uuid::new_v4();
    let (kind, state) = (kind.to_owned(), state.to_owned());
    vault
        .db
        .write_blocking(move |tx| {
            agent_threads::insert_thread(
                tx,
                &ChatThread {
                    id,
                    title: String::new(),
                    last_provider_id: None,
                    last_model_id: None,
                    created_at: 1,
                    updated_at: 1,
                    origin: None,
                },
            )?;
            tx.execute(
                "UPDATE chat_threads SET kind = ?2, state = ?3 WHERE id = ?1",
                params![id.to_string(), kind, state],
            )?;
            Ok(())
        })
        .expect("thread");
    id
}

fn peer_message(vault: &TestVault, thread: Uuid, text: &str, sort_at: i64) -> String {
    let id = Uuid::new_v4().to_string();
    let (row, text) = (id.clone(), text.to_owned());
    vault
        .db
        .write_blocking(move |tx| {
            tx.execute(
                "INSERT INTO chat_peer_messages (id, thread_id, event_id, sender_vault, \
                   sender_device, kind, text, sent_at, received_at, sort_at, inserted_by) \
                 VALUES (?1, ?2, ?3, 'peer', 'dev', 'text', ?4, ?5, ?5, ?5, 'me')",
                params![
                    row,
                    thread.to_string(),
                    Uuid::new_v4().simple().to_string(),
                    text,
                    sort_at
                ],
            )?;
            Ok(())
        })
        .expect("peer message");
    id
}

fn agent_message(vault: &TestVault, thread: Uuid, role: &str, text: &str, at: i64) -> String {
    let id = Uuid::new_v4().to_string();
    let (row, role, text) = (id.clone(), role.to_owned(), text.to_owned());
    vault
        .db
        .write_blocking(move |tx| {
            tx.execute(
                "INSERT INTO chat_messages (id, thread_id, role, content, created_at) \
                 VALUES (?1, ?2, ?3, ?4, ?5)",
                params![row, thread.to_string(), role, text, at],
            )?;
            Ok(())
        })
        .expect("agent message");
    id
}

fn find(vault: &TestVault, text: &str) -> SearchResult {
    let text = text.to_owned();
    vault
        .db
        .read_blocking(move |r| Ok(search(r, &text)?))
        .expect("search")
}

fn ids(result: &SearchResult) -> Vec<&str> {
    result
        .hits
        .iter()
        .map(|hit| hit.message_id.as_str())
        .collect()
}

#[test]
fn contact_group_and_notes_messages_are_found() {
    let vault = test_vault();
    let direct = thread(&vault, "direct", "active");
    let group = thread(&vault, "group", "active");
    let notes = thread(&vault, "note", "active");
    let a = peer_message(&vault, direct, "Treffen am Bahnhof", 1);
    let b = peer_message(&vault, group, "Bahnhof um acht", 2);
    let c = peer_message(&vault, notes, "Fahrkarte zum Bahnhof", 3);
    peer_message(&vault, direct, "something else", 4);

    let result = find(&vault, "bahnhof");

    assert_eq!(ids(&result), [c.as_str(), b.as_str(), a.as_str()]);
    let kinds: Vec<_> = result.hits.iter().map(|hit| hit.kind).collect();
    assert_eq!(
        kinds,
        [
            ConversationKind::Note,
            ConversationKind::Group,
            ConversationKind::Direct
        ]
    );
    assert_eq!(result.hits[2].thread_id, direct.to_string());
    assert!(!result.more);
}

#[test]
fn requests_are_not_searched() {
    let vault = test_vault();
    let request = thread(&vault, "direct", "request");
    peer_message(&vault, request, "Hallo, kennen wir uns?", 1);

    assert!(find(&vault, "kennen").hits.is_empty());
}

#[test]
fn agent_user_and_assistant_messages_are_found_but_no_tool_or_system_rows() {
    let vault = test_vault();
    let session = thread(&vault, "agent", "active");
    let user = agent_message(&vault, session, "user", "Wie wird das Wetter?", 1);
    let assistant = agent_message(&vault, session, "assistant", "Das Wetter wird gut.", 2);
    agent_message(&vault, session, "tool_call", "{\"q\":\"Wetter\"}", 3);
    agent_message(&vault, session, "tool_result", "Wetter: sonnig", 4);
    agent_message(&vault, session, "system", "Du kennst das Wetter.", 5);

    let result = find(&vault, "wetter");

    assert_eq!(ids(&result), [assistant.as_str(), user.as_str()]);
    assert!(result
        .hits
        .iter()
        .all(|hit| hit.kind == ConversationKind::Agent && hit.thread_id == session.to_string()));
}

#[test]
fn both_families_merge_newest_first() {
    let vault = test_vault();
    let direct = thread(&vault, "direct", "active");
    let session = thread(&vault, "agent", "active");
    let old = peer_message(&vault, direct, "Urlaub planen", 10);
    let middle = agent_message(&vault, session, "user", "Urlaub in Italien?", 20);
    let new = peer_message(&vault, direct, "Urlaub gebucht", 30);

    assert_eq!(
        ids(&find(&vault, "urlaub")),
        [new.as_str(), middle.as_str(), old.as_str()]
    );
}

#[test]
fn wildcard_characters_match_literally() {
    let vault = test_vault();
    let direct = thread(&vault, "direct", "active");
    let percent = peer_message(&vault, direct, "50% Rabatt", 1);
    let underscore = peer_message(&vault, direct, "datei_name.txt", 2);
    let backslash = peer_message(&vault, direct, r"C:\Users", 3);
    peer_message(&vault, direct, "50 Euro Rabatt", 4);
    peer_message(&vault, direct, "dateixname", 5);

    assert_eq!(ids(&find(&vault, "0% r")), [percent.as_str()]);
    assert_eq!(ids(&find(&vault, "i_n")), [underscore.as_str()]);
    assert_eq!(ids(&find(&vault, r":\u")), [backslash.as_str()]);
}

#[test]
fn case_is_ignored_beyond_ascii() {
    let vault = test_vault();
    let direct = thread(&vault, "direct", "active");
    let id = peer_message(&vault, direct, "ÄPFEL UND ÖL", 1);

    assert_eq!(ids(&find(&vault, "äpfel")), [id.as_str()]);
    assert_eq!(ids(&find(&vault, "Öl")), [id.as_str()]);
}

#[test]
fn fewer_than_two_characters_search_nothing() {
    let vault = test_vault();
    let direct = thread(&vault, "direct", "active");
    peer_message(&vault, direct, "a ä b", 1);

    assert_eq!(find(&vault, "a"), SearchResult::default());
    assert_eq!(find(&vault, "  ä  "), SearchResult::default());
    assert_eq!(find(&vault, ""), SearchResult::default());
}

#[test]
fn no_match_is_an_empty_result() {
    let vault = test_vault();
    let direct = thread(&vault, "direct", "active");
    peer_message(&vault, direct, "Guten Morgen", 1);

    assert_eq!(find(&vault, "abend"), SearchResult::default());
}

#[test]
fn more_than_fifty_matches_list_the_newest_fifty() {
    let vault = test_vault();
    let direct = thread(&vault, "direct", "active");
    let ids_in_order: Vec<String> = (0..51)
        .map(|n| peer_message(&vault, direct, &format!("Nachricht {n}"), n))
        .collect();

    let result = find(&vault, "nachricht");

    assert_eq!(result.hits.len(), MAX_HITS);
    assert!(result.more);
    assert_eq!(result.hits[0].message_id, ids_in_order[50]);
    assert_eq!(result.hits[49].message_id, ids_in_order[1]);
}

#[test]
fn exactly_fifty_matches_have_no_more() {
    let vault = test_vault();
    let direct = thread(&vault, "direct", "active");
    for n in 0..50 {
        peer_message(&vault, direct, &format!("Nachricht {n}"), n);
    }

    let result = find(&vault, "nachricht");

    assert_eq!(result.hits.len(), MAX_HITS);
    assert!(!result.more);
}

#[test]
fn the_snippet_is_one_line_cut_on_whole_characters_with_ellipses() {
    let needle: Vec<char> = fold("ziel");
    let before = "ü".repeat(60);
    let after = "😀".repeat(100);
    let body = format!("{before}\n\n Ziel  {after}");

    let snippet = snippet(&body, &needle).expect("a match");

    assert!(snippet.starts_with('…') && snippet.ends_with('…'));
    let inner: String = snippet.trim_matches('…').to_owned();
    assert!(inner.contains(" Ziel "), "{inner}");
    assert!(!inner.contains('\n'));
    assert_eq!(inner.chars().take_while(|c| *c == 'ü').count(), BEFORE - 1);
    assert_eq!(inner.chars().count(), BEFORE + AFTER);
}

#[test]
fn a_short_text_has_no_ellipses() {
    assert_eq!(
        snippet("Hallo Welt", &fold("welt")).as_deref(),
        Some("Hallo Welt")
    );
    assert_eq!(snippet("Hallo Welt", &fold("mond")), None);
}

#[test]
fn whitespace_in_the_text_reads_as_in_the_message() {
    let vault = test_vault();
    let direct = thread(&vault, "direct", "active");
    let hit = peer_message(&vault, direct, "Treffen\nam  Montag", 10);
    assert_eq!(
        ids(&find(&vault, "treffen  am\tmontag")),
        vec![hit.as_str()]
    );
}

#[test]
fn an_old_match_behind_many_newer_rows_is_found() {
    let vault = test_vault();
    let direct = thread(&vault, "direct", "active");
    let old = peer_message(&vault, direct, "der alte Treffpunkt", 0);
    let thread_id = direct.to_string();
    vault
        .db
        .write_blocking(move |tx| {
            for n in 1..=1_200_i64 {
                tx.execute(
                    "INSERT INTO chat_peer_messages (id, thread_id, event_id, sender_vault, \
                       sender_device, kind, text, sent_at, received_at, sort_at, inserted_by) \
                     VALUES (?1, ?2, ?3, 'peer', 'dev', 'text', 'nichts', ?4, ?4, ?4, 'me')",
                    params![
                        Uuid::new_v4().to_string(),
                        thread_id,
                        Uuid::new_v4().simple().to_string(),
                        n
                    ],
                )?;
            }
            Ok(())
        })
        .expect("newer messages");

    assert_eq!(ids(&find(&vault, "treffpunkt")), [old.as_str()]);
}
