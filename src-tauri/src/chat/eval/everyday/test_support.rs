//! Helpers the everyday tests share: requests, traces played through the dialog as the runner
//! records them, and their scores.

use std::collections::HashMap;

use serde_json::{json, Value};

use super::consent::{self, play, CallEnding, SafetyError};
use super::scoring::{self, effects, Effect, End, Outcome, Trace, TraceCall};
use super::world::{Answer, FixtureWorld};
use super::{embedded_mail_tools, embedded_world, load_set, Request, MAIL_TOOLS_SOURCE};
use crate::adapters::types::{ChatRequest, ChatRole};
use crate::chat::eval::embedded_tools;
use crate::chat::tools::permission::{decide, Decision, PermissionMode};
use crate::chat::tools::RiskClass;
use crate::chat::turn::DENIED_BY_USER;

pub(super) fn request(expect: Value) -> Request {
    request_in("de", expect)
}

pub(super) fn request_in(lang: &str, expect: Value) -> Request {
    serde_json::from_value(json!({
        "id": "r", "lang": lang, "category": "mail_tidy", "phrasing": "clean", "text": "x", "expect": expect
    }))
    .unwrap()
}

/// A trace whose calls the world answers, as the runner would.
pub(super) fn trace(world: &FixtureWorld, calls: &[(&str, Value)], text: &str, end: End) -> Trace {
    Trace {
        calls: calls
            .iter()
            .map(|(tool, args)| TraceCall {
                tool: (*tool).to_owned(),
                args: args.clone(),
                result: (*tool != "ask_user").then(|| world.answer(tool, args)),
                ending: CallEnding::Ran,
                unasked: None,
                risky: false,
            })
            .collect(),
        text: text.to_owned(),
        end,
        first_chunk_ms: Some(10),
    }
}

/// The trace as the runner records it: each call decided as in Auto mode and played in the dialog
/// (`consent.rs`); a stopped call gets the app's declined answer.
pub(super) fn dialog(request: &Request, trace: &Trace, effects: &HashMap<String, Effect>) -> Trace {
    let mut done: Vec<TraceCall> = Vec::new();
    for call in &trace.calls {
        let risk = match scoring::effect_of(effects, &call.tool) {
            Effect::Read => RiskClass::Safe,
            Effect::Change => RiskClass::Change,
            Effect::Destructive => RiskClass::Risky,
        };
        let decision = if call.tool == "ask_user" {
            Decision::Allow
        } else {
            decide(PermissionMode::Auto, risk)
        };
        let played = play(request, decision, &done, &call.tool, &call.args, effects);
        let mut call = call.clone();
        call.ending = played.ending;
        call.unasked = played.unasked;
        call.risky = risk == RiskClass::Risky;
        if played.ending == CallEnding::Stopped {
            call.result = Some(Answer {
                content: DENIED_BY_USER.to_owned(),
                is_error: true,
            });
        }
        done.push(call);
    }
    Trace {
        calls: done,
        ..trace.clone()
    }
}

pub(super) fn score(
    request: &Request,
    trace: &Trace,
    effects: &HashMap<String, Effect>,
    world: &FixtureWorld,
) -> Outcome {
    scoring::score(request, &dialog(request, trace, effects), effects, world)
}

pub(super) fn safety_errors(
    request: &Request,
    trace: &Trace,
    effects: &HashMap<String, Effect>,
) -> Vec<SafetyError> {
    consent::safety_errors(request, &dialog(request, trace, effects))
}

pub(super) fn endings(expect: Value, calls: &[(&str, Value)]) -> Vec<CallEnding> {
    let world = embedded_world();
    let effects = effects(&embedded_tools(), &embedded_mail_tools());
    let request = request(expect);
    dialog(&request, &trace(&world, calls, "", End::Answer), &effects)
        .calls
        .iter()
        .map(|c| c.ending)
        .collect()
}

pub(super) fn outcome(expect: Value, calls: &[(&str, Value)], text: &str, end: End) -> Outcome {
    outcome_in("de", expect, calls, text, end)
}

pub(super) fn outcome_in(
    lang: &str,
    expect: Value,
    calls: &[(&str, Value)],
    text: &str,
    end: End,
) -> Outcome {
    let world = embedded_world();
    let effects = effects(&embedded_tools(), &embedded_mail_tools());
    score(
        &request_in(lang, expect),
        &trace(&world, calls, text, end),
        &effects,
        &world,
    )
}

pub(super) fn trash_bernd() -> Value {
    json!({ "calls": [
        { "tool": "x_mail_find_messages" },
        { "tool": "x_mail_move_messages", "args": { "ids": { "fromResult": true }, "to": { "anyOf": ["trash", "Papierkorb"] } } }
    ] })
}

/// A set of one request with `text` that expects `expect`.
pub(super) fn request_with(text: &str, expect: Value) -> super::EverydaySet {
    load_set(
        &json!({
            "version": 1,
            "mailTools": MAIL_TOOLS_SOURCE,
            "requests": [{ "id": "r", "lang": "de", "category": "mail_tidy", "phrasing": "clean", "text": text, "expect": expect }]
        })
        .to_string(),
    )
    .unwrap()
}

/// The tool results a request carries, in order.
pub(super) fn tool_results(req: &ChatRequest) -> Vec<String> {
    req.messages
        .iter()
        .filter_map(|m| match &m.role {
            ChatRole::ToolResult { content, .. } => Some(content.clone()),
            _ => None,
        })
        .collect()
}

/// A set of one mail request with `text` that expects an honest refusal.
pub(super) fn one_request(text: &str) -> super::EverydaySet {
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
