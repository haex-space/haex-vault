use serde_json::{json, Value};

use super::embedded_world;

fn ids(content: &str) -> Vec<String> {
    let value: Value = serde_json::from_str(content).unwrap();
    value["messages"]
        .as_array()
        .unwrap()
        .iter()
        .map(|m| m["id"].as_str().unwrap().to_owned())
        .collect()
}

#[test]
fn find_messages_matches_a_part_of_the_sender_newest_first() {
    let world = embedded_world();
    let answer = world.answer("x_mail_find_messages", &json!({ "from": "anna" }));
    assert!(!answer.is_error);
    assert_eq!(ids(&answer.content), ["msg-anna-2", "msg-anna-1"]);
}

#[test]
fn find_messages_filters_by_subject_text_day_and_unread() {
    let world = embedded_world();
    let find = |input: Value| ids(&world.answer("x_mail_find_messages", &input).content);
    assert_eq!(find(json!({ "subject": "fahrrad" })), ["msg-bernd-1"]);
    assert_eq!(find(json!({ "text": "Elektriker" })), ["msg-meier-1"]);
    // Free text covers the sender too, as IMAP's TEXT search does.
    assert_eq!(
        find(json!({ "text": "shop.de" })),
        ["msg-shop-1", "msg-shop-2"]
    );
    let yesterday = find(json!({ "since": "2026-03-17", "before": "2026-03-18" }));
    assert_eq!(
        yesterday,
        ["msg-mama-1", "msg-anna-2", "msg-it-1", "msg-bank-1"]
    );
    assert!(find(json!({ "unread": true }))
        .iter()
        .all(|id| id != "msg-bernd-1"));
    assert_eq!(find(json!({ "limit": 2 })).len(), 2);
    assert_eq!(find(json!({ "mailbox": "trash" })), ["msg-trash-1"]);
    assert_eq!(find(json!({ "mailbox": "Papierkorb" })), ["msg-trash-1"]);
}

#[test]
fn find_messages_pages_with_offset_and_says_whether_more_follow() {
    let world = embedded_world();
    let page = |input: Value| -> Value {
        serde_json::from_str(&world.answer("x_mail_find_messages", &input).content).unwrap()
    };
    let first = page(json!({ "from": "anna", "limit": 1 }));
    assert_eq!(first["messages"][0]["id"], "msg-anna-2");
    assert_eq!(first["total"], 2);
    assert_eq!(first["hasMore"], true);
    let second = page(json!({ "from": "anna", "limit": 1, "offset": 1 }));
    assert_eq!(second["messages"][0]["id"], "msg-anna-1");
    assert_eq!(second["hasMore"], false);
    let past = page(json!({ "from": "anna", "offset": 5 }));
    assert_eq!(past["messages"], json!([]));
    assert_eq!(past["hasMore"], false);
}

#[test]
fn get_message_returns_the_text_and_an_unknown_id_is_an_error() {
    let world = embedded_world();
    let answer = world.answer("x_mail_get_message", &json!({ "id": "msg-anna-2" }));
    assert!(answer.content.contains("Donnerstag um sieben"));
    assert!(
        world
            .answer("x_mail_get_message", &json!({ "id": "msg-0" }))
            .is_error
    );
}

#[test]
fn changes_need_known_ids_and_accounts() {
    let world = embedded_world();
    let moved = world.answer(
        "x_mail_move_messages",
        &json!({ "ids": ["msg-bernd-1"], "to": "trash" }),
    );
    assert!(!moved.is_error);
    assert!(
        world
            .answer(
                "x_mail_move_messages",
                &json!({ "ids": ["msg-x"], "to": "trash" })
            )
            .is_error
    );
    assert!(
        world
            .answer(
                "x_mail_create_draft",
                &json!({ "account": "acc-x", "body": "hi" })
            )
            .is_error
    );
    let draft = world.answer(
        "x_mail_create_draft",
        &json!({ "account": "acc-private", "replyTo": "msg-anna-2", "body": "Ich komme." }),
    );
    assert_eq!(draft.content, json!({ "draftId": "draft-1" }).to_string());
}

#[test]
fn contacts_passwords_and_files_are_found_by_a_part() {
    let world = embedded_world();
    assert!(world
        .answer("contacts_search", &json!({ "query": "huber" }))
        .content
        .contains("Karl-Liebknecht"));
    let passwords = world.answer("passwords_items_search", &json!({ "query": "netflix" }));
    assert!(passwords.content.contains("pw-netflix"));
    assert!(world
        .answer("files_search", &json!({ "query": "steuer" }))
        .content
        .contains("Steuererklärung 2025.pdf"));
}

#[test]
fn a_tool_without_handler_answers_ok() {
    let world = embedded_world();
    let answer = world.answer(
        "settings_appearance_setColorScheme",
        &json!({ "scheme": "dark" }),
    );
    assert_eq!(answer.content, json!({ "ok": true }).to_string());
    assert!(!answer.is_error);
}
