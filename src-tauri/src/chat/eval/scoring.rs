//! Pure scoring of a model's tool calling (spec 032 US5, contracts/eval-format.md). No I/O: the
//! runner hands in what the model did for one sentence, this module says how that counts.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::adapters::types::ToolSpec;
use crate::chat::tools::action_tool::AgentActionDef;
use crate::chat::tools::ask_user::{AskUserTool, ASK_USER_TOOL_NAME};
use crate::chat::tools::Tool;

/// Language of an evaluation sentence.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Lang {
    De,
    En,
}

/// User-intent category of an evaluation sentence.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Kind {
    Read,
    Change,
    Smalltalk,
    /// Ambiguous instructions where the agent should ask instead of acting (spec 046).
    Clarify,
    /// A request for a tool of an installed extension (spec 018 FR-019): the expected tool is
    /// one of `extension_tools.json`, named `x_<slug>_<tool>`.
    Extension,
}

/// One expected tool call and the argument fields that matter for scoring.
#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct ExpectedCall {
    pub tool: String,
    #[serde(default = "empty_object")]
    pub args: Value,
}

fn empty_object() -> Value {
    Value::Object(Default::default())
}

/// Expected outcome for one sentence: no call or a complete unordered batch.
#[derive(Debug, Clone, PartialEq)]
pub enum Expect {
    None,
    Calls(Vec<ExpectedCall>),
}

impl<'de> Deserialize<'de> for Expect {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        #[serde(untagged)]
        enum Raw {
            Word(String),
            List(Vec<ExpectedCall>),
        }
        match Raw::deserialize(deserializer)? {
            Raw::Word(word) if word == "none" => Ok(Expect::None),
            Raw::Word(word) => Err(serde::de::Error::custom(format!(
                "expect must be \"none\" or a list, not \"{word}\""
            ))),
            Raw::List(calls) if calls.is_empty() => {
                Err(serde::de::Error::custom("expect must not be an empty list"))
            }
            Raw::List(calls) => Ok(Expect::Calls(calls)),
        }
    }
}

/// One sentence in the versioned evaluation set.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Sentence {
    pub id: String,
    pub lang: Lang,
    pub kind: Kind,
    pub text: String,
    pub expect: Expect,
    #[serde(default)]
    pub self_test: bool,
}

/// Versioned collection of sentences used by a measurement run.
#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct EvalSet {
    pub version: u32,
    pub sentences: Vec<Sentence>,
}

/// One tool call observed in the model's response.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ObservedCall {
    pub tool: String,
    pub args: Value,
}

/// What the runner saw for one sentence: the calls of the scored step (empty for a text answer)
/// and what the search did, if the sentence needed one.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Observed {
    pub calls: Vec<ObservedCall>,
    /// The sentence took an extra step to search for its tool.
    pub searched: bool,
    /// The expected tools were offered by the second step at the latest (core offer or search).
    pub reached: bool,
    /// What the model wrote in the scored step, to see what it said instead of a call.
    pub text: String,
    /// Every tool the sentence expects was in the offer of the first step (spec 018 SC-001): for
    /// an extension's tool that is the matched offer, which needs no model.
    pub first_offer_hit: bool,
    /// The model tried to run a command on the device in any step (spec 018 SC-003).
    pub ran_command: bool,
}

/// Classification assigned to one sentence after scoring.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum SentenceResult {
    Pass,
    WrongTool,
    BadArgs,
    Missed,
    Spurious,
    NotFound,
}

/// Whether `value` fits a schema of the action subset (`type`, `properties`, `required`, `items`,
/// `enum`). An object with declared `properties` is strict, like the runner in the frontend.
pub fn valid_against(schema: &Value, value: &Value) -> bool {
    let type_ok = match schema.get("type").and_then(Value::as_str) {
        Some("object") => value.is_object(),
        Some("array") => value.is_array(),
        Some("string") => value.is_string(),
        Some("boolean") => value.is_boolean(),
        Some("integer") => {
            value.is_i64()
                || value.is_u64()
                || value
                    .as_f64()
                    .is_some_and(|number| number.is_finite() && number.fract() == 0.0)
        }
        Some("number") => value.is_number(),
        _ => false,
    };
    if !type_ok {
        return false;
    }
    if let Some(allowed) = schema.get("enum").and_then(Value::as_array) {
        if !allowed.contains(value) {
            return false;
        }
    }
    if let (Some(items), Some(values)) = (schema.get("items"), value.as_array()) {
        if !values.iter().all(|item| valid_against(items, item)) {
            return false;
        }
    }
    if let (Some(object), Some(properties)) = (
        value.as_object(),
        schema.get("properties").and_then(Value::as_object),
    ) {
        if object.keys().any(|key| !properties.contains_key(key)) {
            return false;
        }
        let required = schema
            .get("required")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .filter_map(Value::as_str);
        for key in required {
            if !object.contains_key(key) {
                return false;
            }
        }
        for (key, child) in properties {
            if let Some(field) = object.get(key) {
                if !valid_against(child, field) {
                    return false;
                }
            }
        }
    }
    true
}

/// Every field of `expected` is present in `actual` with the same value; objects match as a
/// subset, everything else exactly.
/// Checks the expected argument object as a recursive subset of the observed object.
fn contains_subset(actual: &Value, expected: &Value) -> bool {
    match (actual, expected) {
        (Value::Object(actual), Value::Object(expected)) => expected.iter().all(|(key, value)| {
            actual
                .get(key)
                .is_some_and(|found| contains_subset(found, value))
        }),
        _ => actual == expected,
    }
}

/// Sorts tool names so a complete batch can be compared without call-order significance.
fn names_of<'a>(names: impl Iterator<Item = &'a str>) -> Vec<&'a str> {
    let mut names: Vec<&str> = names.collect();
    names.sort_unstable();
    names
}

/// The schema of a built-in tool that is not an action of the catalog snapshot (`ask_user`).
fn builtin_schema(tool: &str) -> Option<Value> {
    (tool == ASK_USER_TOOL_NAME).then(|| AskUserTool.input_schema())
}

/// How one sentence counts (contracts/eval-format.md, "Bewertung je Satz"), for a sentence whose
/// tools are all actions of `defs`.
pub fn score(sentence: &Sentence, observed: &Observed, defs: &[AgentActionDef]) -> SentenceResult {
    score_with(sentence, observed, defs, &[])
}

/// [`score`] for a set that also has the tools of extensions: their arguments are checked against
/// the schemas of `extension_tools`.
pub fn score_with(
    sentence: &Sentence,
    observed: &Observed,
    defs: &[AgentActionDef],
    extension_tools: &[ToolSpec],
) -> SentenceResult {
    let expected = match &sentence.expect {
        Expect::None => {
            return if observed.calls.is_empty() {
                SentenceResult::Pass
            } else {
                SentenceResult::Spurious
            }
        }
        Expect::Calls(expected) => expected,
    };
    if observed.searched && !observed.reached {
        return SentenceResult::NotFound;
    }
    if observed.calls.is_empty() {
        return SentenceResult::Missed;
    }
    // The whole batch counts: a missing, a doubled and an extra call all make it wrong.
    let want = names_of(expected.iter().map(|call| call.tool.as_str()));
    let got = names_of(observed.calls.iter().map(|call| call.tool.as_str()));
    if want != got {
        return SentenceResult::WrongTool;
    }
    let mut unmatched: Vec<&ExpectedCall> = expected.iter().collect();
    for call in &observed.calls {
        let schema = defs
            .iter()
            .find(|def| def.tool_name == call.tool)
            .map(|def| def.input_schema.clone())
            .or_else(|| {
                extension_tools
                    .iter()
                    .find(|tool| tool.name == call.tool)
                    .map(|tool| tool.input_schema.clone())
            })
            .or_else(|| builtin_schema(&call.tool));
        if !schema.is_some_and(|schema| valid_against(&schema, &call.args)) {
            return SentenceResult::BadArgs;
        }
        // Calls of one tool are matched to expectations of that tool in any order.
        let position = unmatched
            .iter()
            .position(|e| e.tool == call.tool && contains_subset(&call.args, &e.args));
        match position {
            Some(position) => {
                unmatched.remove(position);
            }
            None => return SentenceResult::BadArgs,
        }
    }
    SentenceResult::Pass
}

/// A pass count and its denominator, together with the calculated rate.
#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub struct Rate {
    pub pass: usize,
    pub of: usize,
    pub rate: f64,
}

impl Rate {
    /// Builds a rate without producing `NaN` for an empty group.
    fn new(pass: usize, of: usize) -> Self {
        let rate = if of == 0 {
            0.0
        } else {
            pass as f64 / of as f64
        };
        Self { pass, of, rate }
    }
}

/// A failed sentence and the first observed call, when one exists.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Failure {
    pub id: String,
    pub result: SentenceResult,
    /// The first call the model made, if it made any.
    pub got: Option<ObservedCall>,
    /// The start of what the model wrote in the scored step, if anything.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub text: Option<String>,
}

/// How much of an answer the report keeps.
const TEXT_EXCERPT_CHARS: usize = 300;

/// Aggregate result of one evaluation run.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EvalReport {
    pub set_version: u32,
    pub model: String,
    /// The sampler was deterministic. Cloud models cannot be, so their runs differ more.
    pub deterministic: bool,
    pub total: Rate,
    pub per_lang: BTreeMap<Lang, Rate>,
    pub per_kind: BTreeMap<Kind, Rate>,
    /// Of the sentences that expect an action, the share whose tool was offered by the second
    /// step at the latest (spec SC-005).
    pub reach_rate: f64,
    /// Sentences that needed one extra search step.
    pub extra_steps: usize,
    /// Sentences that expected no call and got one.
    pub spurious_calls: usize,
    /// Of the sentences of the kind `extension`, those whose tool was in the first step's offer
    /// (spec 018 SC-001).
    pub offered: Rate,
    /// Of the sentences of the kind `extension`, those in which the model called the expected
    /// tool (spec 018 SC-002); the arguments are judged by `total`, not here.
    pub called: Rate,
    /// Sentences, of any kind, in which the model tried to run a command on the device (spec 018
    /// SC-003).
    pub run_command_calls: usize,
    /// One entry per sentence of the kind `extension`.
    pub extension_sentences: Vec<ExtensionOutcome>,
    pub failures: Vec<Failure>,
}

/// What happened to the tool of one extension sentence.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExtensionOutcome {
    pub id: String,
    pub offered: bool,
    pub called: bool,
    pub ran_command: bool,
}

/// Whether every tool a sentence expects is among the calls of the scored step.
fn called_all(sentence: &Sentence, observed: &Observed) -> bool {
    match &sentence.expect {
        Expect::None => false,
        Expect::Calls(expected) => expected
            .iter()
            .all(|call| observed.calls.iter().any(|seen| seen.tool == call.tool)),
    }
}

/// One scored sentence, as the runner collects it before report aggregation.
pub struct Scored<'a> {
    pub sentence: &'a Sentence,
    pub observed: Observed,
    pub result: SentenceResult,
}

/// Builds the report: rates overall, per language and per kind, the reach of the search and the
/// failures.
pub fn report(
    set_version: u32,
    model: &str,
    deterministic: bool,
    scored: &[Scored<'_>],
) -> EvalReport {
    let passed = |s: &Scored<'_>| s.result == SentenceResult::Pass;
    let mut per_lang: BTreeMap<Lang, (usize, usize)> = BTreeMap::new();
    let mut per_kind: BTreeMap<Kind, (usize, usize)> = BTreeMap::new();
    for item in scored {
        let lang = per_lang.entry(item.sentence.lang).or_default();
        lang.1 += 1;
        let kind = per_kind.entry(item.sentence.kind).or_default();
        kind.1 += 1;
        if passed(item) {
            lang.0 += 1;
            kind.0 += 1;
        }
    }
    let acting: Vec<&Scored<'_>> = scored
        .iter()
        .filter(|s| matches!(s.sentence.expect, Expect::Calls(_)))
        .collect();
    let reached = acting.iter().filter(|s| s.observed.reached).count();
    let extension: Vec<&Scored<'_>> = scored
        .iter()
        .filter(|s| s.sentence.kind == Kind::Extension)
        .collect();
    let extension_sentences: Vec<ExtensionOutcome> = extension
        .iter()
        .map(|s| ExtensionOutcome {
            id: s.sentence.id.clone(),
            offered: s.observed.first_offer_hit,
            called: called_all(s.sentence, &s.observed),
            ran_command: s.observed.ran_command,
        })
        .collect();
    EvalReport {
        set_version,
        model: model.to_owned(),
        deterministic,
        total: Rate::new(scored.iter().filter(|s| passed(s)).count(), scored.len()),
        per_lang: per_lang
            .into_iter()
            .map(|(k, (p, o))| (k, Rate::new(p, o)))
            .collect(),
        per_kind: per_kind
            .into_iter()
            .map(|(k, (p, o))| (k, Rate::new(p, o)))
            .collect(),
        reach_rate: if acting.is_empty() {
            1.0
        } else {
            reached as f64 / acting.len() as f64
        },
        extra_steps: scored.iter().filter(|s| s.observed.searched).count(),
        spurious_calls: scored
            .iter()
            .filter(|s| s.result == SentenceResult::Spurious)
            .count(),
        offered: Rate::new(
            extension_sentences.iter().filter(|e| e.offered).count(),
            extension_sentences.len(),
        ),
        called: Rate::new(
            extension_sentences.iter().filter(|e| e.called).count(),
            extension_sentences.len(),
        ),
        run_command_calls: scored.iter().filter(|s| s.observed.ran_command).count(),
        extension_sentences,
        failures: scored
            .iter()
            .filter(|s| !passed(s))
            .map(|s| Failure {
                id: s.sentence.id.clone(),
                result: s.result,
                got: s.observed.calls.first().cloned(),
                text: Some(s.observed.text.trim())
                    .filter(|text| !text.is_empty())
                    .map(|text| text.chars().take(TEXT_EXCERPT_CHARS).collect()),
            })
            .collect(),
    }
}
