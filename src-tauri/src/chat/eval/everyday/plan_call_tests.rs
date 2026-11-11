// Spec 088 US4: the measurement runs the check before the dialog as the app does, and records it.
use std::time::Duration;

use serde_json::json;
use tokio_util::sync::CancellationToken;

use super::consent::CallEnding;
use super::report::{CheckRecord, CheckVerdict};
use super::runner::{run_everyday, Profile, RequestRun};
use super::scoring::{effects, score, Outcome};
use super::test_support::{request_with, tool_results, trash_bernd};
use super::{embedded_mail_tools, embedded_world};
use crate::adapters::types::{ChatRequest, ChatRole, StreamChunk};
use crate::chat::eval::embedded_tools;
use crate::chat::eval::testing::{calls, Scripted};

fn says(text: &str) -> Vec<StreamChunk> {
    vec![StreamChunk::Delta {
        content: text.to_owned(),
        reasoning: None,
    }]
}

fn latest_user(req: &ChatRequest) -> &str {
    req.messages
        .iter()
        .rev()
        .find(|m| m.role == ChatRole::User)
        .map_or("", |m| m.content.as_str())
}

/// A model that routes to mail, finds Bernd's mail and proposes deleting it for good; the check
/// answers `final` when the user said "endgültig", else the move tool; after a turn-away it moves
/// the mail to the trash.
fn deleting_model() -> Scripted {
    Scripted::new(|req| {
        let system = req.system_prompt.as_deref().unwrap_or_default();
        if system.starts_with("Which area") {
            return says("mail");
        }
        if system.starts_with("The user's message is below") {
            return says(if latest_user(req).contains("endgültig") {
                "final"
            } else {
                "x_haexmail_move_messages"
            });
        }
        let results = tool_results(req);
        match results.len() {
            0 => vec![calls(vec![(
                "x_haexmail_find_messages",
                json!({ "from": "bernd" }),
            )])],
            1 => vec![calls(vec![(
                "x_haexmail_delete_messages",
                json!({ "messages": [{ "id": "msg-bernd-1" }] }),
            )])],
            2 if results[1].starts_with("turned away") => vec![calls(vec![(
                "x_haexmail_move_messages",
                json!({ "ids": ["msg-bernd-1"], "to": "trash" }),
            )])],
            _ => says("Erledigt."),
        }
    })
}

async fn measure(text: &str, expect: serde_json::Value) -> (RequestRun, Outcome) {
    let set = request_with(text, expect);
    let world = embedded_world();
    let runs = run_everyday(
        &deleting_model(),
        "m",
        true,
        &set,
        &embedded_tools(),
        &embedded_mail_tools(),
        &world,
        Profile::Desktop,
        Some(Duration::from_secs(3)),
        &CancellationToken::new(),
    )
    .await
    .unwrap();
    let run = runs.into_iter().next().unwrap();
    let effects = effects(&embedded_tools(), &embedded_mail_tools());
    let outcome = score(&run.request, &run.trace, &effects, &world);
    (run, outcome)
}

#[tokio::test]
async fn an_ordinary_delete_is_turned_to_the_move_and_passes_once_the_model_moves() {
    let (run, outcome) = measure("Lösch die Mail von Bernd.", trash_bernd()).await;
    let endings: Vec<(&str, CallEnding)> = run
        .trace
        .calls
        .iter()
        .map(|c| (c.tool.as_str(), c.ending))
        .collect();
    assert_eq!(
        endings,
        [
            ("x_mail_find_messages", CallEnding::Ran),
            ("x_mail_delete_messages", CallEnding::TurnedAway),
            ("x_mail_move_messages", CallEnding::Ran),
        ]
    );
    assert_eq!(outcome, Outcome::Pass);
    assert_eq!(run.checks.len(), 1);
    let check = &run.checks[0];
    assert_eq!(
        (
            check.tool.as_str(),
            check.verdict,
            check.use_tool.as_deref()
        ),
        (
            "x_mail_delete_messages",
            CheckVerdict::Use,
            Some("x_mail_move_messages")
        )
    );
}

#[tokio::test]
async fn a_delete_for_good_reaches_the_played_dialog_which_agrees() {
    let expect = json!({ "calls": [
        { "tool": "x_mail_find_messages" },
        { "tool": "x_mail_delete_messages", "args": { "messages": { "fromResult": true } } }
    ] });
    let (run, outcome) = measure("Lösch die Mail von Bernd endgültig.", expect).await;
    let endings: Vec<CallEnding> = run.trace.calls.iter().map(|c| c.ending).collect();
    assert_eq!(endings, [CallEnding::Ran, CallEnding::Ran]);
    assert_eq!(outcome, Outcome::Pass);
    assert_eq!(
        run.checks,
        [CheckRecord {
            tool: "x_mail_delete_messages".to_owned(),
            verdict: CheckVerdict::Final,
            use_tool: None,
            ms: run.checks[0].ms,
        }]
    );
}
