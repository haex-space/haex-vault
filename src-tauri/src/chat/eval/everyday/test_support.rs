//! Helpers the everyday tests share: requests, traces played through the dialog as the runner
//! records them, and their scores.

use std::collections::HashMap;

use serde_json::{json, Map, Value};

use super::consent::{self, play, CallEnding, SafetyError};
use super::scoring::{self, effects, Effect, End, Outcome, ToolFacts, Trace, TraceCall};
use super::world::{Answer, FixtureWorld};
use super::{embedded_mail_tools, embedded_world, load_set, ArgMatch, Request, MAIL_TOOLS_SOURCE};
use crate::adapters::types::{ChatRequest, ChatRole};
use crate::chat::eval::embedded_tools;
use crate::chat::tools::permission::{decide, Decision, PermissionMode};
use crate::chat::tools::RiskClass;
use crate::chat::turn::denied_answer;

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
pub(super) fn dialog(
    request: &Request,
    trace: &Trace,
    effects: &HashMap<String, ToolFacts>,
) -> Trace {
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
                content: denied_answer(),
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
    effects: &HashMap<String, ToolFacts>,
    world: &FixtureWorld,
) -> Outcome {
    scoring::score(request, &dialog(request, trace, effects), effects, world)
}

pub(super) fn safety_errors(
    request: &Request,
    trace: &Trace,
    effects: &HashMap<String, ToolFacts>,
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

/// Concrete arguments for an expected call: what each matcher asks for, ids from the `previous`
/// result, days from the world's clock.
pub(super) fn concrete_args(
    args: &std::collections::BTreeMap<String, ArgMatch>,
    previous: &Value,
    world: &FixtureWorld,
) -> Value {
    let found: Vec<Value> = previous["messages"]
        .as_array()
        .map(|messages| messages.iter().map(|m| m["id"].clone()).collect())
        .unwrap_or_default();
    let mut out = Map::new();
    for (key, matcher) in args {
        let value = match matcher {
            ArgMatch::Exact(value) => value.clone(),
            ArgMatch::Contains { contains } => json!(contains),
            ArgMatch::AnyOf { any_of } => any_of[0].clone(),
            ArgMatch::RelativeDay { relative_day } => json!(world.day(*relative_day)),
            ArgMatch::FromResult { .. } if key == "messages" => {
                json!(found
                    .iter()
                    .map(|id| json!({ "id": id }))
                    .collect::<Vec<_>>())
            }
            ArgMatch::FromResult { .. } => json!(found),
        };
        out.insert(key.clone(), value);
    }
    Value::Object(out)
}

/// `args` with every required property the chain leaves out (a chain names only the arguments that
/// matter for scoring), so that the whole guard runs on a call a model would complete.
pub(super) fn with_required(mut args: Value, schema: &Value) -> Value {
    let required = schema["required"].as_array().cloned().unwrap_or_default();
    for name in required.iter().filter_map(Value::as_str) {
        if args.get(name).is_some_and(|v| !v.is_null()) {
            continue;
        }
        args[name] = match schema["properties"][name]["type"].as_str() {
            Some("array") => json!([]),
            Some("object") => json!({}),
            Some("boolean") => json!(false),
            Some("number" | "integer") => json!(1),
            _ => json!("placeholder"),
        };
    }
    args
}
