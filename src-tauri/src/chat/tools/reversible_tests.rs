use std::collections::HashSet;
use std::sync::Mutex;
use std::time::Duration;

use serde_json::json;

use super::domains_tests::{mail, registry_with, Extension};
use super::offer::tool_spec;
use super::permission::PermissionMode;
use super::reversible::{
    applies, before_dialog, candidates, check, check_messages, check_prompt, parse_check, title_of,
    Candidate, Check, Verdict, MAX_CHECK_TOKENS,
};
use super::route::RouteCall;
use super::route_tests::{Broken, Silent};
use super::{RiskClass, Tool, ToolRegistry};
use crate::adapters::types::{ChatMessage, ChatRole, Sampling, StreamChunk, ToolSpec};
use crate::adapters::ProviderAdapter;
use crate::chat::eval::testing::Scripted;

const DELETE: &str = "x_mail_delete_messages";
const MOVE: &str = "x_mail_move_messages";

/// haex-mail as the check sees it: search, delete for good, move.
fn mail_registry() -> ToolRegistry {
    let mut tools = mail();
    tools.push(Extension {
        name: MOVE,
        title: "Move mail",
        risk: RiskClass::Change,
        example: None,
    });
    registry_with(tools)
}

fn offered(registry: &ToolRegistry) -> Vec<ToolSpec> {
    registry
        .iter()
        .map(|tool| tool_spec(tool.as_ref()))
        .collect()
}

fn delete_tool(registry: &ToolRegistry) -> std::sync::Arc<dyn Tool> {
    registry.get(DELETE).unwrap()
}

fn moving() -> Candidate {
    Candidate {
        name: MOVE.to_owned(),
        title: "Move mail".to_owned(),
    }
}

fn said(role: ChatRole, content: &str) -> ChatMessage {
    ChatMessage {
        role,
        content: content.to_owned(),
        attachments: Vec::new(),
    }
}

fn answering(reply: &'static str) -> Scripted {
    Scripted::new(move |_| {
        vec![StreamChunk::Delta {
            content: reply.to_owned(),
            reasoning: None,
        }]
    })
}

fn call(adapter: &dyn ProviderAdapter) -> RouteCall<'_> {
    RouteCall {
        adapter,
        model_id: "m",
        timeout: Duration::from_millis(300),
        sampling: Sampling::Deterministic,
    }
}

#[test]
fn the_candidates_are_the_offered_changing_tools_of_the_same_domain() {
    let registry = mail_registry();
    let tool = delete_tool(&registry);
    assert_eq!(
        candidates(tool.as_ref(), &registry, &offered(&registry)),
        [moving()]
    );
    // Not offered in this step: no candidate.
    let without_move: Vec<ToolSpec> = offered(&registry)
        .into_iter()
        .filter(|spec| spec.name != MOVE)
        .collect();
    assert!(candidates(tool.as_ref(), &registry, &without_move).is_empty());
    // A changing action of another domain never stands in for a mail delete.
    let all = candidates(tool.as_ref(), &registry, &offered(&registry));
    assert!(all.iter().all(|c| c.name.starts_with("x_mail_")), "{all:?}");
}

#[test]
fn an_action_takes_its_english_title_and_an_extension_tool_its_own() {
    let registry = mail_registry();
    assert_eq!(
        title_of(delete_tool(&registry).as_ref()),
        "Delete mail for good"
    );
    let action = registry
        .iter()
        .find(|tool| {
            tool.action_definition()
                .is_some_and(|def| !def.titles.en.is_empty())
        })
        .unwrap();
    assert_eq!(
        title_of(action.as_ref()),
        action.action_definition().unwrap().titles.en
    );
}

#[test]
fn only_a_final_tool_outside_plan_mode_is_checked() {
    let registry = mail_registry();
    let delete = delete_tool(&registry);
    assert!(applies(delete.as_ref(), PermissionMode::Auto));
    assert!(applies(delete.as_ref(), PermissionMode::Manual));
    assert!(!applies(delete.as_ref(), PermissionMode::Plan));
    let moving = registry.get(MOVE).unwrap();
    assert!(!applies(moving.as_ref(), PermissionMode::Auto));
}

#[test]
fn the_prompt_names_the_tools_by_name_and_title_and_no_area() {
    let prompt = check_prompt(DELETE, "Delete mail for good", &[moving()]);
    assert_eq!(
        prompt,
        "The user's message is below. The assistant wants to call `x_mail_delete_messages` (Delete \
         mail for good), which cannot be undone.\nDid the user ask for exactly this, something \
         that cannot be undone? If yes, answer: final\nIf not, which of these tools does what the \
         user asked in a way that can be undone?\nx_mail_move_messages: Move mail\nAnswer with \
         one tool name, or: none. Nothing else."
    );
    // Spec 087 FR-019: only the tool lines name the area.
    let rules: String = prompt
        .lines()
        .filter(|line| !line.contains("x_mail_"))
        .collect();
    for word in ["mail", "trash", "papierkorb", "message"] {
        assert!(!rules.to_lowercase().contains(word), "{word}");
    }
}

#[test]
fn the_messages_are_the_users_with_the_last_question_before_the_latest() {
    let conversation = [
        said(ChatRole::User, "Lösch die Mail von Bernd"),
        said(
            ChatRole::ToolCall {
                id: "1".to_owned(),
                name: "x_mail_find_messages".to_owned(),
                input: json!({}),
            },
            "",
        ),
        said(
            ChatRole::ToolResult {
                call_id: "1".to_owned(),
                content: "[]".to_owned(),
                is_error: false,
            },
            "",
        ),
        said(ChatRole::Assistant, "Soll ich sie endgültig löschen?"),
        said(ChatRole::User, "Ja"),
    ];
    let (messages, chars) = check_messages(&conversation);
    let kept: Vec<(ChatRole, String)> = messages.into_iter().map(|m| (m.role, m.content)).collect();
    assert_eq!(
        kept,
        [
            (ChatRole::User, "Lösch die Mail von Bernd".to_owned()),
            (
                ChatRole::Assistant,
                "Soll ich sie endgültig löschen?".to_owned()
            ),
            (ChatRole::User, "Ja".to_owned()),
        ]
    );
    assert_eq!(chars, 24 + 31);
}

#[test]
fn an_answer_that_asks_nothing_and_the_current_turns_calls_are_left_out() {
    let conversation = [
        said(ChatRole::User, "Welche Mails habe ich?"),
        said(ChatRole::Assistant, "Du hast eine Mail von Bernd."),
        said(ChatRole::User, "Lösch sie"),
        said(
            ChatRole::ToolCall {
                id: "1".to_owned(),
                name: "x_mail_find_messages".to_owned(),
                input: json!({}),
            },
            "",
        ),
    ];
    let (messages, _) = check_messages(&conversation);
    let roles: Vec<ChatRole> = messages.into_iter().map(|m| m.role).collect();
    assert_eq!(roles, [ChatRole::User, ChatRole::User]);
}

#[test]
fn the_question_of_ask_user_counts_as_the_last_question() {
    let conversation = [
        said(ChatRole::User, "Lösch die Mail von Bernd"),
        said(
            ChatRole::ToolCall {
                id: "1".to_owned(),
                name: "ask_user".to_owned(),
                input: json!({ "question": "Endgültig oder in den Papierkorb?" }),
            },
            "",
        ),
        said(ChatRole::User, "Endgültig"),
    ];
    let (messages, _) = check_messages(&conversation);
    assert_eq!(messages[1].role, ChatRole::Assistant);
    assert_eq!(messages[1].content, "Endgültig oder in den Papierkorb?");
}

#[test]
fn a_reply_is_read_by_its_first_known_word() {
    let candidates = [moving()];
    let parse = |reply| parse_check(reply, &candidates);
    assert_eq!(parse("final"), Verdict::Final);
    assert_eq!(parse("Final."), Verdict::Final);
    assert_eq!(parse("x_mail_move_messages"), Verdict::Use(moving()));
    assert_eq!(parse("`move_messages`"), Verdict::Use(moving()));
    assert_eq!(parse("X_MAIL_MOVE_MESSAGES"), Verdict::Use(moving()));
    assert_eq!(parse("none"), Verdict::NoVerdict);
    assert_eq!(parse("none, x_mail_move_messages"), Verdict::NoVerdict);
    assert_eq!(parse("Ich weiß nicht."), Verdict::NoVerdict);
    assert_eq!(parse("x_mail_find_messages"), Verdict::NoVerdict);
    assert_eq!(parse(""), Verdict::NoVerdict);
}

#[test]
fn a_turned_away_call_says_nothing_ran_and_names_the_tool() {
    let check = Check {
        tool: DELETE.to_owned(),
        verdict: Verdict::Use(moving()),
        ms: 1,
    };
    assert_eq!(
        check.turned_away().unwrap(),
        "turned away: `x_mail_delete_messages` cannot be undone and the user did not ask for \
         that; nothing ran and nothing changed. Do what the user asked with \
         `x_mail_move_messages` (Move mail) instead; do not say it was done before it ran."
    );
    for verdict in [Verdict::Final, Verdict::NoVerdict] {
        let check = Check {
            verdict,
            ..check.clone()
        };
        assert_eq!(check.turned_away(), None);
    }
}

#[tokio::test]
async fn the_check_asks_without_tools_and_reads_the_reply() {
    let adapter = answering("x_mail_move_messages");
    let conversation = [said(ChatRole::User, "Lösch die Mail von Bernd")];
    let done = check(
        &call(&adapter),
        &conversation,
        DELETE,
        "Delete mail for good",
        &[moving()],
    )
    .await;
    assert_eq!(done.verdict, Verdict::Use(moving()));
    assert_eq!(done.tool, DELETE);
    let seen = adapter.seen.lock().unwrap();
    assert_eq!(seen.len(), 1);
    assert!(seen[0].tools.is_empty());
    assert!(!seen[0].reasoning_requested);
    assert_eq!(seen[0].max_new_tokens, Some(MAX_CHECK_TOKENS));
    assert_eq!(seen[0].sampling, Sampling::Deterministic);
}

/// [`before_dialog`] for "Lösch die Mail von Bernd" proposing the final delete.
async fn ask(
    route: Option<&RouteCall<'_>>,
    mode: PermissionMode,
    registry: &Mutex<ToolRegistry>,
    offer: &[ToolSpec],
    checked: &mut HashSet<String>,
) -> Option<Check> {
    let delete = delete_tool(&registry.lock().unwrap());
    let conversation = [said(ChatRole::User, "Lösch die Mail von Bernd")];
    before_dialog(
        route,
        mode,
        delete.as_ref(),
        registry,
        offer,
        &conversation,
        checked,
    )
    .await
}

#[tokio::test]
async fn before_the_dialog_a_tool_is_checked_once_and_only_where_the_check_applies() {
    let registry = Mutex::new(mail_registry());
    let offer = offered(&registry.lock().unwrap());
    let adapter = answering("final");
    let route = Some(call(&adapter));
    let route = route.as_ref();
    let auto = PermissionMode::Auto;
    let mut checked = HashSet::new();
    // A hosted model, Plan mode, no candidate: no question.
    assert!(ask(None, auto, &registry, &offer, &mut checked)
        .await
        .is_none());
    let plan = PermissionMode::Plan;
    assert!(ask(route, plan, &registry, &offer, &mut checked)
        .await
        .is_none());
    assert!(ask(route, auto, &registry, &[], &mut checked)
        .await
        .is_none());
    assert!(adapter.seen.lock().unwrap().is_empty());
    // Once, then never again in the turn.
    let first = ask(route, auto, &registry, &offer, &mut checked).await;
    assert_eq!(first.unwrap().verdict, Verdict::Final);
    assert!(ask(route, auto, &registry, &offer, &mut checked)
        .await
        .is_none());
    assert_eq!(adapter.seen.lock().unwrap().len(), 1);
}

#[tokio::test]
async fn an_error_or_no_answer_in_time_is_no_verdict() {
    crate::test_log::install();
    let conversation = [said(ChatRole::User, "Lösch die Mail von Bernd")];
    for (adapter, outcome) in [
        (&Broken as &dyn ProviderAdapter, "failed"),
        (&Silent, "timed out"),
    ] {
        let done = check(&call(adapter), &conversation, DELETE, "Delete", &[moving()]).await;
        assert_eq!(done.verdict, Verdict::NoVerdict, "{outcome}");
        assert!(crate::test_log::all()
            .lines()
            .any(|line| line.contains(&format!("chat: check of {DELETE} {outcome}"))));
    }
}
