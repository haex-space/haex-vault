use serde_json::json;
use tokio_util::sync::CancellationToken;

use super::runner::{run_eval, EvalError, Which};
use super::testing::{calls, oracle, Scripted};
use super::{embedded_extension_tools, embedded_set, embedded_tools};
use crate::adapters::types::{ChatRole, Sampling, StreamChunk};

async fn run(adapter: &Scripted, which: Which, deterministic: bool) -> super::scoring::EvalReport {
    run_eval(
        adapter,
        "test-model",
        deterministic,
        &embedded_set(),
        &embedded_tools(),
        &embedded_extension_tools(),
        which,
        &CancellationToken::new(),
    )
    .await
    .unwrap()
}

#[tokio::test]
async fn a_model_that_does_what_the_set_expects_passes_every_sentence() {
    let report = run(&oracle(), Which::All, true).await;
    let ids: Vec<&str> = report.failures.iter().map(|f| f.id.as_str()).collect();
    assert!(ids.is_empty(), "{ids:?}");
    assert_eq!(report.total.pass, report.total.of);
    assert_eq!(report.total.of, embedded_set().sentences.len());
    assert!((report.reach_rate - 1.0).abs() < 1e-9);
    assert!(
        report.extra_steps >= 8,
        "the set has at least eight sentences outside the core offer"
    );
    assert_eq!(report.spurious_calls, 0);
    assert!(report.deterministic);
    assert_eq!(report.model, "test-model");
}

#[tokio::test]
async fn a_found_tool_is_offered_in_the_second_step_with_the_search_result() {
    let adapter = oracle();
    run(&adapter, Which::All, false).await;
    let seen = adapter.seen.lock().unwrap();
    let second = seen
        .iter()
        .find(|req| req.messages.len() == 3)
        .expect("a second step exists");
    assert!(matches!(second.messages[1].role, ChatRole::ToolCall { .. }));
    assert!(matches!(
        second.messages[2].role,
        ChatRole::ToolResult { .. }
    ));
    assert!(second.tools.iter().any(|t| t.name == "find_actions"));
    assert!(
        second.tools.len() <= 17,
        "core, search, ask_user and run_command plus at most five hits"
    );
    // Every first step offers the same core tools, the search, `ask_user` and `run_command`,
    // whatever the sentence; the tools of extensions that fit the message come on top, at most
    // three (spec 018 R6).
    let first_offers: Vec<std::collections::HashSet<String>> = seen
        .iter()
        .filter(|req| req.messages.len() == 1)
        .map(|req| req.tools.iter().map(|t| t.name.clone()).collect())
        .collect();
    let shared = first_offers
        .iter()
        .skip(1)
        .fold(first_offers[0].clone(), |shared, offer| {
            shared.intersection(offer).cloned().collect()
        });
    assert!(shared.len() <= 12, "{shared:?}");
    assert!(shared.contains("run_command") && shared.contains("find_actions"));
    for offer in &first_offers {
        let extra: Vec<&String> = offer.difference(&shared).collect();
        assert!(extra.len() <= 3, "{extra:?}");
        assert!(extra.iter().all(|name| name.starts_with("x_")), "{extra:?}");
    }
}

#[tokio::test]
async fn the_sampling_follows_the_flag() {
    for (deterministic, expected) in [(true, Sampling::Deterministic), (false, Sampling::Default)] {
        let adapter = oracle();
        run(&adapter, Which::SelfTest, deterministic).await;
        assert!(adapter
            .seen
            .lock()
            .unwrap()
            .iter()
            .all(|r| r.sampling == expected));
    }
}

#[tokio::test]
async fn the_self_test_asks_only_the_marked_sentences() {
    let adapter = oracle();
    let report = run(&adapter, Which::SelfTest, true).await;
    let marked = embedded_set()
        .sentences
        .iter()
        .filter(|s| s.self_test)
        .count();
    assert_eq!(report.total.of, marked);
    assert!((5..=6).contains(&marked));
    assert_eq!(
        adapter.seen.lock().unwrap().len(),
        marked,
        "all five are core or smalltalk: one step each"
    );
}

#[tokio::test]
async fn a_search_that_finds_nothing_is_not_found_and_costs_no_second_step() {
    let adapter = Scripted::new(|req| {
        if req.messages.len() == 1 {
            vec![calls(vec![(
                "find_actions",
                json!({ "query": "zzzz nothing" }),
            )])]
        } else {
            panic!("no second step expected when the tool was not found");
        }
    });
    let set = embedded_set();
    let report = run_eval(
        &adapter,
        "m",
        true,
        &set,
        &embedded_tools(),
        &embedded_extension_tools(),
        Which::All,
        &CancellationToken::new(),
    )
    .await
    .unwrap();
    let not_found = report
        .failures
        .iter()
        .filter(|f| serde_json::to_value(f.result).unwrap() == "not_found")
        .count();
    assert!(not_found >= 8);
    assert!(
        report.reach_rate < 1.0,
        "the sentences outside the core offer were not reached"
    );
}

#[tokio::test]
async fn a_model_that_only_talks_misses_and_one_that_always_calls_is_spurious() {
    let talker = Scripted::new(|_| {
        vec![StreamChunk::Delta {
            content: "I cannot do that.".to_owned(),
            reasoning: None,
        }]
    });
    let report = run(&talker, Which::All, true).await;
    assert_eq!(report.per_kind[&super::scoring::Kind::Smalltalk].pass, 6);
    assert_eq!(report.spurious_calls, 0);

    let eager = Scripted::new(|_| vec![calls(vec![("wm_state_get", json!({}))])]);
    let report = run(&eager, Which::All, true).await;
    assert_eq!(report.spurious_calls, 6);
}

#[tokio::test]
async fn cancelling_stops_the_run() {
    let cancel = CancellationToken::new();
    cancel.cancel();
    let result = run_eval(
        &oracle(),
        "m",
        true,
        &embedded_set(),
        &embedded_tools(),
        &embedded_extension_tools(),
        Which::All,
        &cancel,
    )
    .await;
    assert!(matches!(result, Err(EvalError::Cancelled)));
}

#[tokio::test]
async fn every_step_carries_the_tool_instruction_of_a_chat_turn() {
    let adapter = oracle();
    run(&adapter, Which::All, true).await;
    let seen = adapter.seen.lock().unwrap();
    assert!(!seen.is_empty());
    assert!(seen.iter().all(|req| {
        req.system_prompt.as_deref().is_some_and(|prompt| {
            prompt.starts_with(crate::chat::tools::prompt::TOOL_INSTRUCTION)
                && prompt.contains("call ask_user")
        })
    }));
    assert!(seen
        .iter()
        .all(|req| req.tools.iter().any(|tool| tool.name == "ask_user")));
}

#[tokio::test]
async fn a_failure_keeps_the_start_of_what_the_model_said_instead_of_a_call() {
    let talker = Scripted::new(|_| {
        vec![StreamChunk::Delta {
            content: "x".repeat(500),
            reasoning: None,
        }]
    });
    let report = run(&talker, Which::All, true).await;
    let missed = report
        .failures
        .iter()
        .find(|f| f.id == "read-tabs-de-1")
        .expect("a model that only talks misses it");
    assert_eq!(missed.text.as_deref().map(str::len), Some(300));
    let json = serde_json::to_value(&report).unwrap();
    assert!(json["failures"][0]["text"].is_string());
}

#[tokio::test]
async fn the_model_the_set_was_written_for_also_passes_the_extension_sentences() {
    let report = run(&oracle(), Which::All, true).await;
    assert!(report.failures.is_empty(), "{:?}", report.failures);
    let extension = embedded_set()
        .sentences
        .iter()
        .filter(|s| s.kind == super::scoring::Kind::Extension)
        .count();
    assert_eq!(report.called.of, extension);
    assert_eq!(report.called.pass, extension);
    assert_eq!(report.run_command_calls, 0);
    assert_eq!(report.extension_sentences.len(), extension);
}

/// Spec 018 SC-001 asks for nine of ten. No model takes part in this measure: the matched offer
/// reads only the message and the texts of the tools. The sentences were written before the first
/// measurement and are not tuned to it; the two that miss say "Kalender" and "grocery list" where
/// the tool texts say "Termine" and "shopping list", which a word match cannot bridge
/// (`specs/018-extension-agent-tools/eval-results.md`). This test holds the level reached so far;
/// SC-001 itself is not met until the matcher or the examples change.
#[tokio::test]
async fn the_matched_offer_holds_the_level_measured_so_far() {
    let talker = Scripted::new(|_| Vec::new());
    let report = run(&talker, Which::All, true).await;
    let missed: Vec<&str> = report
        .extension_sentences
        .iter()
        .filter(|e| !e.offered)
        .map(|e| e.id.as_str())
        .collect();
    assert!(report.offered.of >= 10);
    assert_eq!(
        missed,
        ["extension-calendar-de-1", "extension-shopping-en-1"],
        "offered {}/{}",
        report.offered.pass,
        report.offered.of
    );
}

#[tokio::test]
async fn a_model_that_reaches_for_the_shell_is_counted_and_never_runs_anything() {
    let shell = Scripted::new(|_| {
        vec![calls(vec![(
            "run_command",
            json!({ "command": "mail -H" }),
        )])]
    });
    let report = run(&shell, Which::All, true).await;
    assert_eq!(report.run_command_calls, report.total.of);
    assert_eq!(report.called.pass, 0);
    assert!(report.extension_sentences.iter().all(|e| e.ran_command));
}

#[tokio::test]
async fn a_tool_that_the_first_step_did_not_offer_is_found_by_the_search() {
    // A sentence about a tool the matched offer misses: the model searches, the search reads the
    // extension tools too, and the second step offers the hit.
    let set: super::scoring::EvalSet = serde_json::from_value(json!({
        "version": 4,
        "sentences": [{
            "id": "x", "lang": "en", "kind": "extension", "text": "Hmm.",
            "expect": [{ "tool": "x_flashcards_cards_due", "args": {} }],
        }]
    }))
    .unwrap();
    let adapter = Scripted::new(|req| {
        if req.messages.len() == 1 {
            vec![calls(vec![(
                "find_actions",
                json!({ "query": "flashcards due" }),
            )])]
        } else {
            vec![calls(vec![("x_flashcards_cards_due", json!({}))])]
        }
    });
    let report = run_eval(
        &adapter,
        "m",
        true,
        &set,
        &embedded_tools(),
        &embedded_extension_tools(),
        Which::All,
        &CancellationToken::new(),
    )
    .await
    .unwrap();
    assert_eq!(report.total.pass, 1, "{:?}", report.failures);
    assert_eq!(report.offered.pass, 0, "not offered in the first step");
    assert_eq!(report.called.pass, 1);
    assert_eq!(report.extra_steps, 1);
}
