use serde_json::json;
use tokio_util::sync::CancellationToken;

use super::runner::{run_everyday, MAX_STEPS};
use super::scoring::End;
use super::{embedded_mail_tools, embedded_world, load_set, MAIL_TOOLS_SOURCE};
use crate::adapters::types::{ChatRole, StreamChunk};
use crate::chat::eval::embedded_tools;
use crate::chat::eval::testing::{calls, Scripted};

fn one_request(text: &str) -> super::EverydaySet {
    load_set(
        &json!({
            "version": 1,
            "mailTools": MAIL_TOOLS_SOURCE,
            "requests": [{ "id": "r", "lang": "de", "category": "mail_read", "phrasing": "clean", "text": text, "expect": "refusal" }]
        })
        .to_string(),
    )
    .unwrap()
}

fn tool_results(req: &crate::adapters::types::ChatRequest) -> Vec<String> {
    req.messages
        .iter()
        .filter_map(|m| match &m.role {
            ChatRole::ToolResult { content, .. } => Some(content.clone()),
            _ => None,
        })
        .collect()
}

#[tokio::test]
async fn a_chain_gets_the_world_s_answers_and_ends_with_the_text() {
    let model = Scripted::new(|req| match tool_results(req).len() {
        0 => vec![calls(vec![(
            "x_mail_find_messages",
            json!({ "from": "anna" }),
        )])],
        1 => vec![calls(vec![(
            "x_mail_get_message",
            json!({ "id": "msg-anna-2" }),
        )])],
        _ => vec![StreamChunk::Delta {
            content: "Anna fragt nach Donnerstag.".to_owned(),
            reasoning: None,
        }],
    });
    let runs = run_everyday(
        &model,
        "m",
        true,
        &one_request("Lies mir die letzte Mail von Anna vor."),
        &embedded_tools(),
        &embedded_mail_tools(),
        &embedded_world(),
        &CancellationToken::new(),
    )
    .await
    .unwrap();
    let trace = &runs[0].trace;
    assert_eq!(trace.end, End::Answer);
    assert_eq!(trace.calls.len(), 2);
    assert!(trace.calls.iter().all(|c| c.known));
    assert!(trace.calls[1]
        .result
        .as_ref()
        .unwrap()
        .content
        .contains("Café am Markt"));
    assert_eq!(trace.text, "Anna fragt nach Donnerstag.");
    let seen = model.seen.lock().unwrap();
    assert!(tool_results(&seen[1])[0].contains("msg-anna-2"));
}

#[tokio::test]
async fn the_system_prompt_carries_the_fixed_clock() {
    let model = Scripted::new(|_| {
        vec![StreamChunk::Delta {
            content: "9:41".to_owned(),
            reasoning: None,
        }]
    });
    run_everyday(
        &model,
        "m",
        true,
        &one_request("Wie spät ist es?"),
        &embedded_tools(),
        &embedded_mail_tools(),
        &embedded_world(),
        &CancellationToken::new(),
    )
    .await
    .unwrap();
    let seen = model.seen.lock().unwrap();
    let prompt = seen[0].system_prompt.as_deref().unwrap();
    assert!(
        prompt.contains("Wednesday, 2026-03-18 09:41 (UTC+01:00)"),
        "{prompt}"
    );
}

#[tokio::test]
async fn a_search_widens_the_next_offer() {
    let model = Scripted::new(|req| match tool_results(req).len() {
        0 => vec![calls(vec![(
            "find_actions",
            json!({ "query": "Kontakt Telefonnummer" }),
        )])],
        _ => vec![StreamChunk::Delta {
            content: "ok".to_owned(),
            reasoning: None,
        }],
    });
    run_everyday(
        &model,
        "m",
        true,
        &one_request("Was ist die Handynummer von Anna?"),
        &embedded_tools(),
        &embedded_mail_tools(),
        &embedded_world(),
        &CancellationToken::new(),
    )
    .await
    .unwrap();
    let seen = model.seen.lock().unwrap();
    let offered = |i: usize| seen[i].tools.iter().any(|t| t.name == "contacts_search");
    assert!(!offered(0));
    assert!(offered(1));
}

#[tokio::test]
async fn ask_user_ends_the_request_with_its_question() {
    let model = Scripted::new(|_| {
        vec![calls(vec![(
            "ask_user",
            json!({ "question": "Farbschema oder Helligkeit?", "options": ["Farbschema", "Helligkeit"] }),
        )])]
    });
    let runs = run_everyday(
        &model,
        "m",
        true,
        &one_request("Mach es dunkler."),
        &embedded_tools(),
        &embedded_mail_tools(),
        &embedded_world(),
        &CancellationToken::new(),
    )
    .await
    .unwrap();
    assert_eq!(runs[0].trace.end, End::Question);
    assert_eq!(runs[0].trace.text, "Farbschema oder Helligkeit?");
}

#[tokio::test]
async fn a_model_that_keeps_calling_stops_at_the_step_limit() {
    let model = Scripted::new(|_| vec![calls(vec![("settings_get", json!({}))])]);
    let runs = run_everyday(
        &model,
        "m",
        true,
        &one_request("Wie sind meine Einstellungen?"),
        &embedded_tools(),
        &embedded_mail_tools(),
        &embedded_world(),
        &CancellationToken::new(),
    )
    .await
    .unwrap();
    assert_eq!(runs[0].trace.end, End::StepLimit);
    assert_eq!(runs[0].trace.calls.len(), MAX_STEPS);
    assert_eq!(model.seen.lock().unwrap().len(), MAX_STEPS);
}

#[tokio::test]
async fn an_unknown_tool_gets_the_chat_s_error_and_is_marked() {
    let model = Scripted::new(|req| match tool_results(req).len() {
        0 => vec![calls(vec![("x_alarm_set", json!({ "minutes": 15 }))])],
        _ => vec![StreamChunk::Delta {
            content: "Erledigt.".to_owned(),
            reasoning: None,
        }],
    });
    let runs = run_everyday(
        &model,
        "m",
        true,
        &one_request("Stell mir einen Wecker in 15 Minuten."),
        &embedded_tools(),
        &embedded_mail_tools(),
        &embedded_world(),
        &CancellationToken::new(),
    )
    .await
    .unwrap();
    let call = &runs[0].trace.calls[0];
    assert!(!call.known);
    let result = call.result.as_ref().unwrap();
    assert!(result.is_error);
    assert_eq!(result.content, "unknown tool: x_alarm_set");
}

struct Failing;

#[async_trait::async_trait]
impl crate::adapters::ProviderAdapter for Failing {
    async fn list_models(
        &self,
    ) -> Result<Vec<crate::adapters::ProviderModel>, crate::adapters::AdapterError> {
        Ok(Vec::new())
    }

    async fn stream_chat(
        &self,
        _req: crate::adapters::types::ChatRequest,
    ) -> Result<crate::adapters::types::AdapterStream, crate::adapters::AdapterError> {
        Err(crate::adapters::AdapterError::Parse {
            reason: "the prompt does not fit the context".to_owned(),
        })
    }
}

#[tokio::test]
async fn a_model_error_ends_only_its_request() {
    let set = load_set(
        &json!({
            "version": 1,
            "mailTools": MAIL_TOOLS_SOURCE,
            "requests": [
                { "id": "a", "lang": "de", "category": "no_tool", "phrasing": "clean", "text": "x", "expect": "refusal" },
                { "id": "b", "lang": "de", "category": "no_tool", "phrasing": "clean", "text": "y", "expect": "refusal" }
            ]
        })
        .to_string(),
    )
    .unwrap();
    let runs = run_everyday(
        &Failing,
        "m",
        true,
        &set,
        &embedded_tools(),
        &embedded_mail_tools(),
        &embedded_world(),
        &CancellationToken::new(),
    )
    .await
    .unwrap();
    assert_eq!(runs.len(), 2);
    assert!(runs.iter().all(|r| r.trace.end == End::Error));
    assert!(runs[0].trace.text.contains("does not fit the context"));
}
