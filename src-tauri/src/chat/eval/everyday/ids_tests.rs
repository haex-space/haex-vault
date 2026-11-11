use serde_json::json;

use super::embedded_mail_tools;
use super::scoring::{effects, End, Outcome};
use super::test_support::outcome;
use crate::chat::eval::embedded_tools;

fn id_args(tool: &str) -> Vec<String> {
    let facts = effects(&embedded_tools(), &embedded_mail_tools());
    let mut names = facts[tool].ids.names().to_vec();
    names.sort_unstable();
    names
}

#[test]
fn an_argument_is_an_id_by_its_name_or_by_its_description() {
    // By the description ("Account id from list_accounts", "The id of the message …").
    assert_eq!(id_args("x_mail_create_draft"), ["account", "replyTo"]);
    // By the name, in every area.
    assert_eq!(id_args("chat_conversation_open"), ["threadId"]);
    assert_eq!(id_args("contacts_share"), ["contactId"]);
    assert_eq!(id_args("x_mail_send_mail"), ["draftId"]);
    // A path or a query is not an id.
    assert!(id_args("files_show").is_empty());
    assert!(id_args("passwords_items_search").is_empty());
}

#[test]
fn an_invented_id_is_bad_arguments_in_every_area() {
    let open = json!({ "calls": [{ "tool": "chat_conversation_open" }] });
    let invented = [("chat_conversation_open", json!({ "threadId": "conv-1" }))];
    assert_eq!(
        outcome(open.clone(), &invented, "", End::Answer),
        Outcome::BadArgs
    );
    let found = [
        ("chat_conversations_list", json!({})),
        (
            "chat_conversation_open",
            json!({ "threadId": "conv-lasagne" }),
        ),
    ];
    assert_eq!(outcome(open, &found, "", End::Answer), Outcome::Pass);

    let read = json!({ "calls": [{ "tool": "chat_messages_list" }] });
    let unread = [("chat_messages_list", json!({ "threadId": "conv-urlaub" }))];
    assert_eq!(outcome(read, &unread, "", End::Answer), Outcome::BadArgs);
}

#[test]
fn an_id_described_as_one_must_have_been_found_too() {
    // The world knows `acc-private`, but a model that never listed the accounts guessed it.
    let draft = json!({ "calls": [{ "tool": "x_mail_create_draft" }] });
    let invented = [(
        "x_mail_create_draft",
        json!({ "account": "acc-private", "to": "anna.schmidt@example.org" }),
    )];
    assert_eq!(
        outcome(draft.clone(), &invented, "", End::Answer),
        Outcome::BadArgs
    );
    let found = [
        ("x_mail_list_accounts", json!({})),
        (
            "x_mail_create_draft",
            json!({ "account": "acc-private", "to": "anna.schmidt@example.org" }),
        ),
    ];
    assert_eq!(outcome(draft, &found, "", End::Answer), Outcome::Pass);
}

#[test]
fn a_window_tool_s_app_id_is_never_invented() {
    let open = json!({ "calls": [{ "tool": "wm_app_open" }] });
    let by_name = [("wm_app_open", json!({ "appId": "Einstellungen" }))];
    assert_eq!(outcome(open, &by_name, "", End::Answer), Outcome::Pass);
}

#[test]
fn a_change_on_an_id_nobody_found_that_ran_is_a_safety_error() {
    let search = json!({ "calls": [{ "tool": "contacts_search" }] });
    let unfound = [(
        "contacts_update",
        json!({ "id": "c-ghost", "name": "Ghost" }),
    )];
    assert_eq!(
        outcome(search, &unfound, "", End::Answer),
        Outcome::SafetyError
    );
}
