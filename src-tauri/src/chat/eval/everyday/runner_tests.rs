use serde_json::{json, Value};
use tokio_util::sync::CancellationToken;

use super::consent::CallEnding;
use super::mail_names::MailNames;
use super::runner::{app_registry, run_everyday, Profile, MAX_STEPS};
use super::scoring::End;
use super::test_support::{concrete_args, one_request, request_with, tool_results, with_required};
use super::{
    embedded_mail_tools, embedded_set, embedded_world, load_set, ArgMatch, Expectation,
    MAIL_TOOLS_SOURCE,
};
use crate::adapters::types::{ChatMessage, ChatRole, StreamChunk};
use crate::chat::eval::embedded_tools;
use crate::chat::eval::testing::{calls, Scripted};
use crate::chat::turn::guard::{seen_text, turn_away};

#[tokio::test]
async fn a_chain_gets_the_world_s_answers_and_ends_with_the_text() {
    let model = Scripted::new(|req| match tool_results(req).len() {
        0 => vec![calls(vec![(
            "x_haexmail_find_messages",
            json!({ "from": "anna" }),
        )])],
        1 => vec![calls(vec![(
            "x_haexmail_get_message",
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
        Profile::Desktop,
        None,
        &CancellationToken::new(),
    )
    .await
    .unwrap();
    let trace = &runs[0].trace;
    assert_eq!(trace.end, End::Answer);
    assert_eq!(trace.calls.len(), 2);
    assert!(trace.calls.iter().all(|c| c.ending == CallEnding::Ran));
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
async fn the_system_prompt_carries_the_fixed_clock_and_the_request_s_language() {
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
        Profile::Desktop,
        None,
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
    assert!(prompt.contains("Always reply in German"), "{prompt}");
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
        Profile::Desktop,
        None,
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
        Profile::Desktop,
        None,
        &CancellationToken::new(),
    )
    .await
    .unwrap();
    assert_eq!(runs[0].trace.end, End::Question);
    assert_eq!(runs[0].trace.text, "Farbschema oder Helligkeit?");
}

#[tokio::test]
async fn the_calls_after_ask_user_in_the_same_batch_still_run_and_count() {
    // The first round reads, so the delete of the second round names an id the model has seen;
    // the request asks for the delete, so the played dialog agrees and the call runs.
    let model = Scripted::new(|req| match tool_results(req).len() {
        0 => vec![calls(vec![(
            "x_haexmail_find_messages",
            json!({ "from": "bernd" }),
        )])],
        _ => vec![calls(vec![
            (
                "ask_user",
                json!({ "question": "Wirklich löschen?", "options": ["Ja", "Nein"] }),
            ),
            (
                "x_haexmail_delete_messages",
                json!({ "messages": [{ "id": "msg-bernd-1", "subject": "Rechnung Fahrradreparatur" }] }),
            ),
        ])],
    });
    let expect = json!({ "calls": [
        { "tool": "x_mail_find_messages" },
        { "tool": "x_mail_delete_messages", "args": { "messages": { "fromResult": true } } }
    ] });
    let runs = run_everyday(
        &model,
        "m",
        true,
        &request_with("Lösch die Mail von Bernd.", expect),
        &embedded_tools(),
        &embedded_mail_tools(),
        &embedded_world(),
        Profile::Desktop,
        None,
        &CancellationToken::new(),
    )
    .await
    .unwrap();
    let trace = &runs[0].trace;
    assert_eq!(trace.end, End::Question);
    assert_eq!(trace.text, "Wirklich löschen?");
    assert_eq!(trace.calls.len(), 3);
    let delete = trace
        .calls
        .iter()
        .find(|c| c.tool == "x_mail_delete_messages")
        .expect("the delete call after ask_user is recorded");
    assert_eq!(delete.ending, CallEnding::Ran);
    assert!(!delete.result.as_ref().expect("an answer").is_error);
    assert_eq!(model.seen.lock().unwrap().len(), 2);
}

#[tokio::test]
async fn a_tool_the_request_called_stays_offered_after_another_search() {
    let model = Scripted::new(|req| match tool_results(req).len() {
        0 => vec![calls(vec![(
            "find_actions",
            json!({ "query": "Kontakt Telefonnummer" }),
        )])],
        1 => vec![calls(vec![("contacts_search", json!({ "query": "Anna" }))])],
        2 => vec![calls(vec![(
            "find_actions",
            json!({ "query": "Passwort" }),
        )])],
        _ => vec![StreamChunk::Delta {
            content: "ok".to_owned(),
            reasoning: None,
        }],
    });
    let runs = run_everyday(
        &model,
        "m",
        true,
        &one_request("Was ist die Handynummer von Anna?"),
        &embedded_tools(),
        &embedded_mail_tools(),
        &embedded_world(),
        Profile::Desktop,
        None,
        &CancellationToken::new(),
    )
    .await
    .unwrap();
    let second_search = runs[0].trace.calls[2].result.as_ref().unwrap();
    assert!(!second_search.content.contains("contacts_search"));
    let seen = model.seen.lock().unwrap();
    assert!(seen[3].tools.iter().any(|t| t.name == "contacts_search"));
}

#[tokio::test]
async fn an_action_the_step_did_not_offer_is_unknown_as_in_the_chat() {
    // `contacts_search` is registered but not offered before `find_actions` found it.
    let model = Scripted::new(|req| match tool_results(req).len() {
        0 => vec![calls(vec![("contacts_search", json!({ "query": "Anna" }))])],
        _ => vec![StreamChunk::Delta {
            content: "Keine Nummer gefunden.".to_owned(),
            reasoning: None,
        }],
    });
    let runs = run_everyday(
        &model,
        "m",
        true,
        &one_request("Was ist die Handynummer von Anna?"),
        &embedded_tools(),
        &embedded_mail_tools(),
        &embedded_world(),
        Profile::Desktop,
        None,
        &CancellationToken::new(),
    )
    .await
    .unwrap();
    let call = &runs[0].trace.calls[0];
    assert_eq!(call.ending, CallEnding::TurnedAway);
    assert!(call
        .result
        .as_ref()
        .unwrap()
        .content
        .starts_with("unknown tool: contacts_search"));
}

#[tokio::test]
async fn a_mail_tool_the_message_did_not_match_still_runs() {
    let model = Scripted::new(|req| match tool_results(req).len() {
        0 => vec![calls(vec![("x_haexmail_list_accounts", json!({}))])],
        _ => vec![StreamChunk::Delta {
            content: "ok".to_owned(),
            reasoning: None,
        }],
    });
    let runs = run_everyday(
        &model,
        "m",
        true,
        &one_request("Wie spät ist es?"),
        &embedded_tools(),
        &embedded_mail_tools(),
        &embedded_world(),
        Profile::Desktop,
        None,
        &CancellationToken::new(),
    )
    .await
    .unwrap();
    let seen = model.seen.lock().unwrap();
    assert!(!seen[0]
        .tools
        .iter()
        .any(|t| t.name == "x_haexmail_list_accounts"));
    let call = &runs[0].trace.calls[0];
    assert_eq!(call.ending, CallEnding::Ran);
    assert!(!call.result.as_ref().unwrap().is_error);
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
        Profile::Desktop,
        None,
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
        Profile::Desktop,
        None,
        &CancellationToken::new(),
    )
    .await
    .unwrap();
    let call = &runs[0].trace.calls[0];
    assert_eq!(call.ending, CallEnding::TurnedAway);
    let result = call.result.as_ref().unwrap();
    assert!(result.is_error);
    assert!(result.content.starts_with("unknown tool: x_alarm_set"));
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

/// A server that accepted the request and never answers.
struct Stalled;

#[async_trait::async_trait]
impl crate::adapters::ProviderAdapter for Stalled {
    async fn list_models(
        &self,
    ) -> Result<Vec<crate::adapters::ProviderModel>, crate::adapters::AdapterError> {
        Ok(Vec::new())
    }

    async fn stream_chat(
        &self,
        _req: crate::adapters::types::ChatRequest,
    ) -> Result<crate::adapters::types::AdapterStream, crate::adapters::AdapterError> {
        std::future::pending().await
    }
}

#[tokio::test]
async fn a_cancel_ends_a_stream_that_never_opens() {
    let cancel = CancellationToken::new();
    cancel.cancel();
    let result = run_everyday(
        &Stalled,
        "m",
        true,
        &one_request("Wie spät ist es?"),
        &embedded_tools(),
        &embedded_mail_tools(),
        &embedded_world(),
        Profile::Desktop,
        None,
        &cancel,
    )
    .await;
    assert!(matches!(
        result,
        Err(crate::chat::eval::runner::EvalError::Cancelled)
    ));
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
        Profile::Desktop,
        None,
        &CancellationToken::new(),
    )
    .await
    .unwrap();
    assert_eq!(runs.len(), 2);
    assert!(runs.iter().all(|r| r.trace.end == End::Error));
    assert!(runs[0].trace.text.contains("does not fit the context"));
}

/// Concrete arguments for an expected call: the ids of `fromResult` from the messages the previous
/// answer listed, the first alternative of `anyOf`; a relative day is left out (it names no id).
#[test]
fn the_guard_refuses_no_call_of_an_expected_chain() {
    // Research R6: every expected chain, played on the fixture world, passes the guard.
    let world = embedded_world();
    let mail = embedded_mail_tools();
    let names = MailNames::of(&mail);
    let registry = app_registry(&embedded_tools(), &mail, Profile::Desktop);
    let registry = registry.lock().unwrap();
    let mut checked = 0;
    for request in embedded_set().requests {
        let Expectation::Calls(chains) = &request.expect else {
            continue;
        };
        for chain in chains {
            let mut history = vec![ChatMessage {
                role: ChatRole::User,
                content: request.text.clone(),
                attachments: Vec::new(),
            }];
            let mut previous = Value::Null;
            for expected in chain {
                if expected
                    .args
                    .values()
                    .any(|m| matches!(m, ArgMatch::FromResult { .. }))
                {
                    let found = previous["messages"].as_array().map_or(0, Vec::len);
                    assert!(found > 0, "{}: no ids to take", request.id);
                }
                let tool = registry
                    .get(names.in_app(&expected.tool))
                    .expect("a registered tool");
                let args = with_required(
                    concrete_args(&expected.args, &previous, &world),
                    &tool.input_schema(),
                );
                let refusal = turn_away(&args, tool.as_ref(), &seen_text(&history), &registry, &[]);
                assert_eq!(refusal, None, "{} {} {args}", request.id, expected.tool);
                checked += 1;
                let answer = world.answer(&expected.tool, &args);
                previous = serde_json::from_str(&answer.content).unwrap_or(Value::Null);
                history.push(ChatMessage {
                    role: ChatRole::ToolResult {
                        call_id: String::new(),
                        content: answer.content,
                        is_error: answer.is_error,
                    },
                    content: String::new(),
                    attachments: Vec::new(),
                });
            }
        }
    }
    assert!(checked > 30, "{checked} calls checked");
}
