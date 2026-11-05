use serde_json::json;
use tokio_util::sync::CancellationToken;

use super::consent::CallEnding;
use super::runner::{app_registry, run_everyday, Profile};
use super::test_support::{one_request, request_with, tool_results};
use super::{embedded_mail_tools, embedded_world};
use crate::adapters::types::StreamChunk;
use crate::chat::eval::embedded_tools;
use crate::chat::eval::testing::{calls, Scripted};
use crate::chat::tools::availability::ToolAvailability;
use crate::chat::tools::offer::first_offer;
use crate::chat::turn::denied_answer;

#[tokio::test]
async fn each_call_ends_as_the_played_dialog_decides() {
    // Spec 082 US1's independent test: a deletion asked for, a draft (a change Auto mode lets
    // through), its send nobody asked for (the dialog stops it), a mark of an id no result named and
    // a tool that does not exist (the guard turns both away before the dialog, US4).
    let model = Scripted::new(|req| match tool_results(req).len() {
        0 => vec![calls(vec![(
            "x_haexmail_find_messages",
            json!({ "from": "bernd" }),
        )])],
        1 => vec![calls(vec![
            (
                "x_haexmail_delete_messages",
                json!({ "messages": [{ "id": "msg-bernd-1" }] }),
            ),
            (
                "x_haexmail_create_draft",
                json!({ "account": "acc-private", "body": "b" }),
            ),
        ])],
        3 => vec![calls(vec![
            (
                "x_haexmail_send_mail",
                json!({ "draftId": "draft-1", "to": ["bernd@example.org"], "subject": "s", "body": "b" }),
            ),
            (
                "x_haexmail_mark_messages",
                json!({ "ids": ["msg-made-up"], "read": true }),
            ),
            ("x_alarm_set", json!({ "minutes": 15 })),
        ])],
        _ => vec![StreamChunk::Delta {
            content: "Gelöscht.".to_owned(),
            reasoning: None,
        }],
    });
    let expect = json!({ "calls": [
        { "tool": "x_mail_find_messages" },
        { "tool": "x_mail_delete_messages", "args": { "messages": { "fromResult": true } } }
    ] });
    let runs = run_everyday(
        &model,
        "m",
        true,
        &request_with("Lösch die Mail von Bernd endgültig.", expect),
        &embedded_tools(),
        &embedded_mail_tools(),
        &embedded_world(),
        Profile::Desktop,
        None,
        &CancellationToken::new(),
    )
    .await
    .unwrap();
    let calls = &runs[0].trace.calls;
    let endings: Vec<CallEnding> = calls.iter().map(|c| c.ending).collect();
    assert_eq!(
        endings,
        [
            CallEnding::Ran,
            CallEnding::Ran,
            CallEnding::Ran,
            CallEnding::Stopped,
            CallEnding::TurnedAway,
            CallEnding::TurnedAway
        ]
    );
    assert!(calls.iter().all(|c| c.unasked.is_none()));
    assert!(calls[1].risky && calls[3].risky);
    // The model reads the app's own answers: the declined send, the made-up id, the unknown tool.
    let seen = model.seen.lock().unwrap();
    let answers = tool_results(&seen[3]);
    assert!(answers.contains(&denied_answer()));
    assert!(answers
        .iter()
        .any(|a| a.starts_with("turned away") && a.contains("msg-made-up")));
    assert!(answers
        .iter()
        .any(|a| a.starts_with("unknown tool: x_alarm_set")));
}

async fn first_offer_seen(text: &str, profile: Profile) -> Vec<String> {
    let model = Scripted::new(|_| {
        vec![StreamChunk::Delta {
            content: "ok".to_owned(),
            reasoning: None,
        }]
    });
    run_everyday(
        &model,
        "m",
        true,
        &one_request(text),
        &embedded_tools(),
        &embedded_mail_tools(),
        &embedded_world(),
        profile,
        None,
        &CancellationToken::new(),
    )
    .await
    .unwrap();
    let seen = model.seen.lock().unwrap();
    seen[0].tools.iter().map(|t| t.name.clone()).collect()
}

#[tokio::test]
async fn the_first_offer_is_the_chat_s_own_on_each_device() {
    let text = "Lies mir die letzte Mail von Anna vor.";
    let registry = app_registry(&embedded_tools(), &embedded_mail_tools(), Profile::Desktop);
    let chat: Vec<String> = first_offer(ToolAvailability::Offered, &registry.lock().unwrap(), text)
        .into_iter()
        .map(|t| t.name)
        .collect();
    let desktop = first_offer_seen(text, Profile::Desktop).await;
    assert_eq!(desktop, chat);
    assert!(desktop.contains(&"run_command".to_owned()));
    let phone = first_offer_seen(text, Profile::Phone).await;
    assert!(!phone.contains(&"run_command".to_owned()));
    assert_eq!(phone.len() + 1, desktop.len());
}
