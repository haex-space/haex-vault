use std::sync::Arc;

use async_trait::async_trait;
use serde_json::{json, Value};
use tokio_util::sync::CancellationToken;

use super::{check_ids, check_required, closest_tools, seen_text, turn_away, unknown_ids};
use crate::adapters::types::{ChatMessage, ChatRole, ToolSpec};
use crate::chat::tools::action_bridge::ActionBridge;
use crate::chat::tools::action_tool::{ActionTool, AgentActionDef};
use crate::chat::tools::find_actions::FindActionsTool;
use crate::chat::tools::offer::tool_spec;
use crate::chat::tools::{RiskClass, Tool, ToolOrigin, ToolRegistry, ToolResult};

fn action(action_id: &str, effect: &str, schema: Value) -> Arc<dyn Tool> {
    let def: AgentActionDef = serde_json::from_value(json!({
        "toolName": action_id.replace('.', "_"),
        "actionId": action_id,
        "description": "d",
        "inputSchema": schema,
        "effect": effect,
    }))
    .expect("a valid definition");
    Arc::new(ActionTool::new(def, ActionBridge::default()))
}

/// An extension's tool, as far as the guard looks at it.
struct Extension {
    name: &'static str,
    risk: RiskClass,
}

#[async_trait]
impl Tool for Extension {
    fn name(&self) -> &str {
        self.name
    }
    fn description(&self) -> &str {
        "d"
    }
    fn source(&self) -> &'static str {
        "haextension"
    }
    fn input_schema(&self) -> Value {
        json!({ "type": "object" })
    }
    fn risk_class(&self) -> RiskClass {
        self.risk
    }
    fn origin(&self) -> Option<ToolOrigin> {
        Some(ToolOrigin {
            extension_id: "mail".to_owned(),
            extension_name: "Mail".to_owned(),
            tool_title_de: String::new(),
            tool_title_en: String::new(),
            dev: false,
        })
    }
    async fn execute(&self, _input: Value, _cancel: CancellationToken) -> ToolResult {
        ToolResult::ok("{}")
    }
}

fn extension(name: &'static str, risk: RiskClass) -> Arc<dyn Tool> {
    Arc::new(Extension { name, risk })
}

fn message(role: ChatRole, content: &str) -> ChatMessage {
    ChatMessage {
        role,
        attachments: Vec::new(),
        content: content.to_owned(),
    }
}

fn result(content: &str) -> ChatMessage {
    message(
        ChatRole::ToolResult {
            call_id: "c".to_owned(),
            content: content.to_owned(),
            is_error: false,
        },
        "",
    )
}

fn delete_mail() -> Arc<dyn Tool> {
    extension("x_mail_delete_messages", RiskClass::Risky)
}

#[test]
fn an_id_from_an_earlier_result_or_the_user_s_message_passes() {
    let seen = seen_text(&[
        message(ChatRole::User, "Lösche die Mail mit Betreff Rechnung-7"),
        result(r#"{"messages":[{"id":"msg-bernd-1","subject":"Hallo"}]}"#),
    ]);
    let tool = delete_mail();
    let found = json!({ "messages": [{ "id": "msg-bernd-1" }] });
    assert!(unknown_ids(&found, tool.as_ref(), &seen).is_empty());
    let named = json!({ "ids": ["Rechnung-7"] });
    assert!(unknown_ids(&named, tool.as_ref(), &seen).is_empty());
}

#[test]
fn the_model_s_own_text_and_calls_do_not_count_as_seen() {
    let seen = seen_text(&[
        message(ChatRole::User, "Lösche Bernds Mail"),
        message(ChatRole::Assistant, "Ich lösche msg-42."),
        message(
            ChatRole::ToolCall {
                id: "c".to_owned(),
                name: "x".to_owned(),
                input: json!({ "id": "msg-42" }),
            },
            "",
        ),
    ]);
    let ids = unknown_ids(&json!({ "id": "msg-42" }), delete_mail().as_ref(), &seen);
    assert_eq!(ids, ["msg-42"]);
}

#[test]
fn an_error_result_proves_no_id() {
    // A turned-away call's answer repeats the made-up id; the second try must not pass with it.
    let refusal = message(
        ChatRole::ToolResult {
            call_id: "c".to_owned(),
            content:
                "turned away: no earlier result and nothing the user said names the id mail_id."
                    .to_owned(),
            is_error: true,
        },
        "",
    );
    let seen = seen_text(&[message(ChatRole::User, "Lösche Bernds Mail"), refusal]);
    let ids = unknown_ids(&json!({ "id": "mail_id" }), delete_mail().as_ref(), &seen);
    assert_eq!(ids, ["mail_id"]);
}

#[test]
fn an_id_the_tool_s_schema_names_passes() {
    let schema = json!({
        "type": "object",
        "properties": {
            "folderId": { "type": "string", "enum": ["inbox", "archive"] },
            "labelId": { "type": "string", "description": "Use starred for flagged mail." },
            "tagId": { "type": "string", "examples": ["work"] }
        }
    });
    let tool = action("mail.label", "write", schema);
    let input = json!({ "folderId": "archive", "labelId": "starred", "tagId": "work" });
    assert!(unknown_ids(&input, tool.as_ref(), "").is_empty());
}

#[test]
fn a_made_up_id_is_refused_on_a_write_and_a_destructive_tool() {
    let input = json!({ "id": "12345" });
    for effect in ["write", "destructive"] {
        let tool = action("contacts.update", effect, json!({ "type": "object" }));
        assert_eq!(unknown_ids(&input, tool.as_ref(), "Anna 123456"), ["12345"]);
    }
    let tool = extension("x_mail_move_messages", RiskClass::Change);
    assert_eq!(unknown_ids(&input, tool.as_ref(), ""), ["12345"]);
}

#[test]
fn a_reading_tool_is_not_checked() {
    let input = json!({ "id": "12345" });
    let tool = action("contacts.get", "read", json!({ "type": "object" }));
    assert!(unknown_ids(&input, tool.as_ref(), "").is_empty());
    let tool = extension("x_mail_get_message", RiskClass::Safe);
    assert!(unknown_ids(&input, tool.as_ref(), "").is_empty());
}

#[test]
fn a_window_tool_is_not_checked() {
    let tool = action("wm.tab.close", "destructive", json!({ "type": "object" }));
    assert!(unknown_ids(&json!({ "tabId": "tab-9" }), tool.as_ref(), "").is_empty());
}

#[test]
fn every_id_property_name_is_checked() {
    let tool = delete_mail();
    let seen = "known-1 known-2";
    for input in [
        json!({ "id": "x1" }),
        json!({ "ids": ["x1"] }),
        json!({ "messageId": "x1" }),
        json!({ "messageIds": ["x1"] }),
        json!({ "messageIds": ["known-1", "x1", "known-2"] }),
        json!({ "messages": [{ "id": "known-1" }, { "id": "x1" }] }),
    ] {
        assert_eq!(unknown_ids(&input, tool.as_ref(), seen), ["x1"], "{input}");
    }
    // Other properties are not ids.
    assert!(unknown_ids(&json!({ "subject": "x1" }), tool.as_ref(), seen).is_empty());
}

#[test]
fn an_id_must_be_a_whole_token_of_what_was_seen() {
    let tool = delete_mail();
    let seen = r#"{"id":"msg-123","date":"2026-11-04"}"#;
    assert_eq!(
        unknown_ids(&json!({ "id": "123" }), tool.as_ref(), seen),
        ["123"]
    );
    assert_eq!(
        unknown_ids(&json!({ "id": "msg" }), tool.as_ref(), seen),
        ["msg"]
    );
    assert!(unknown_ids(&json!({ "id": "msg-123" }), tool.as_ref(), seen).is_empty());
}

#[test]
fn a_refusal_names_the_ids_and_the_reading_tools_of_the_same_domain() {
    let mut registry = ToolRegistry::new();
    for tool in [
        delete_mail(),
        extension("x_mail_find_messages", RiskClass::Safe),
        extension("x_mail_list_mailboxes", RiskClass::Safe),
        extension("x_mail_move_messages", RiskClass::Change),
    ] {
        registry.register(tool);
    }
    let contacts = action("contacts.search", "read", json!({ "type": "object" }));
    registry.register(contacts);
    let answer = check_ids(
        &json!({ "messages": [{ "id": "12345" }] }),
        delete_mail().as_ref(),
        "",
        &registry,
        &[],
    )
    .expect("a refusal");
    assert!(answer.contains("12345"), "{answer}");
    assert!(
        answer.contains("x_mail_find_messages or x_mail_list_mailboxes"),
        "{answer}"
    );
    assert!(!answer.contains("contacts_search"), "{answer}");
}

#[test]
fn an_action_s_readers_are_only_the_offered_ones_else_the_search() {
    let mut registry = ToolRegistry::new();
    let update = action("contacts.update", "write", json!({ "type": "object" }));
    let search = action("contacts.search", "read", json!({ "type": "object" }));
    registry.register(update.clone());
    registry.register(search.clone());
    let input = json!({ "id": "c-9" });
    let not_offered = check_ids(&input, update.as_ref(), "", &registry, &[]).unwrap();
    assert!(not_offered.contains("find_actions"), "{not_offered}");
    let offered = [tool_spec(search.as_ref())];
    let answer = check_ids(&input, update.as_ref(), "", &registry, &offered).unwrap();
    assert!(answer.contains("contacts_search"), "{answer}");
}

fn spec(name: &str) -> ToolSpec {
    ToolSpec {
        name: name.to_owned(),
        description: String::new(),
        input_schema: json!({ "type": "object" }),
    }
}

#[test]
fn an_unknown_tool_gets_up_to_three_offered_names_by_shared_words() {
    let offered: Vec<ToolSpec> = [
        "wm_apps_list",
        "x_mail_find_messages",
        "x_mail_get_message",
        "x_mail_list_mailboxes",
        "x_mail_list_accounts",
        "find_actions",
    ]
    .into_iter()
    .map(spec)
    .collect();
    assert_eq!(
        closest_tools("x_mail_list_messages", &offered),
        // Three words each; the offer's order breaks the tie.
        [
            "x_mail_find_messages",
            "x_mail_list_mailboxes",
            "x_mail_list_accounts"
        ]
    );
    assert_eq!(
        closest_tools("find_messages", &offered)[0],
        "x_mail_find_messages"
    );
    assert!(closest_tools("calendar_add", &offered).is_empty());
}

// Spec 082 US5: every answer holzi writes for a failed call ends with what to do next.

fn draft_tool() -> Arc<dyn Tool> {
    action(
        "notes.create",
        "write",
        json!({
            "type": "object",
            "properties": {
                "title": { "type": "string", "description": "The note's title, a few words." },
                "body": { "type": "string" }
            },
            "required": ["title", "body"]
        }),
    )
}

#[test]
fn a_missing_required_argument_names_it_and_its_description() {
    let tool = draft_tool();
    let answer = check_required(&json!({ "body": "x" }), tool.as_ref()).expect("turned away");
    assert!(answer.contains("`title`"), "{answer}");
    assert!(
        answer.contains("The note's title, a few words."),
        "{answer}"
    );
    // Null is as good as missing; without a description the name is enough.
    let answer = check_required(&json!({ "title": "t", "body": null }), tool.as_ref()).unwrap();
    assert!(answer.ends_with("Call it again with `body`."), "{answer}");
    assert_eq!(
        check_required(&json!({ "title": "t", "body": "b" }), tool.as_ref()),
        None
    );
}

#[test]
fn the_guard_names_a_missing_argument_before_an_unknown_id() {
    let tool = action(
        "contacts.update",
        "write",
        json!({ "type": "object", "required": ["name"] }),
    );
    let answer = turn_away(
        &json!({ "id": "12345" }),
        tool.as_ref(),
        "",
        &ToolRegistry::new(),
        &[],
    )
    .unwrap();
    assert!(answer.contains("`name`"), "{answer}");
}

#[test]
fn an_unknown_id_names_the_reading_tools_and_not_the_user() {
    let mut registry = ToolRegistry::new();
    registry.register(delete_mail());
    registry.register(extension("x_mail_find_messages", RiskClass::Safe));
    let answer = check_ids(
        &json!({ "ids": ["12345"] }),
        delete_mail().as_ref(),
        "",
        &registry,
        &[],
    )
    .unwrap();
    assert!(
        answer.ends_with(
            "Find the right one first with x_mail_find_messages; never ask the user for an id."
        ),
        "{answer}"
    );
}

#[tokio::test]
async fn an_empty_search_says_to_try_other_words_once_or_tell_the_user() {
    let search = FindActionsTool::new(Vec::new());
    let result = search
        .execute(
            json!({ "query": "kalender termin" }),
            CancellationToken::new(),
        )
        .await;
    assert!(!result.is_error);
    assert!(
        result.content.contains("Try other keywords once"),
        "{}",
        result.content
    );
    assert!(
        result.content.contains("tell the user"),
        "{}",
        result.content
    );
}
