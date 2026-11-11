//! Turns away calls holzi can tell are wrong before the permission dialog (spec 082 FR-011, FR-012,
//! research R6/R7): an id nobody named, a required argument left out, and a tool that does not
//! exist. The user is not asked. Every answer holzi writes for a failed call ends with what to do
//! next (FR-014).

use serde_json::Value;

use crate::adapters::types::{ChatMessage, ChatRole, ToolSpec};
use crate::chat::tools::action_tool::ActionEffect;
use crate::chat::tools::offer::words;
use crate::chat::tools::{RiskClass, Tool, ToolRegistry};

use super::plan_calls::callable;

/// The window tools work on app and tab ids, a small known set that changes no user data.
const WINDOW_DOMAIN: &str = "wm";
/// At most this many names in an answer to an unknown tool.
const MAX_CLOSEST: usize = 3;

/// What the model has seen and may take ids from: every user message and every successful tool
/// result of the conversation. Wider than the turn so that "and delete it" in the next turn keeps
/// the id the earlier turn found. An error result proves no id: it may only repeat the model's own
/// input, as a turned-away call's answer does. Of a user message only the words that look like an
/// id count (`id_like`), each as a whole: "the mail from Bernd" names a person, not the id `Bernd`,
/// and "bernd@firma.de" is no `bernd`.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Seen {
    /// The successful tool results, in which an id counts as a whole token ([`holds`]).
    results: String,
    /// The words of the user's messages that look like an id.
    said: Vec<String>,
}

impl Seen {
    /// Whether an earlier result or the user named `id`.
    pub fn names(&self, id: &str) -> bool {
        holds(&self.results, id) || self.said.iter().any(|word| word == id)
    }
}

impl From<&str> for Seen {
    /// Seen results alone.
    fn from(results: &str) -> Self {
        Self {
            results: results.to_owned(),
            said: Vec::new(),
        }
    }
}

/// What the model has seen in `messages` ([`Seen`]).
pub fn seen_text(messages: &[ChatMessage]) -> Seen {
    let mut seen = Seen::default();
    for message in messages {
        let text = match &message.role {
            ChatRole::User => {
                seen.said.extend(
                    message
                        .content
                        .split(apart)
                        .filter_map(id_like)
                        .map(str::to_owned),
                );
                continue;
            }
            ChatRole::ToolResult {
                content,
                is_error: false,
                ..
            } => content.as_str(),
            ChatRole::ToolResult { .. } | ChatRole::Assistant | ChatRole::ToolCall { .. } => {
                continue
            }
        };
        seen.results.push_str(text);
        seen.results.push('\n');
    }
    seen
}

/// What separates the words of the user's message: white space and list punctuation, so the ids
/// of "msg-42,msg-43" count one by one.
fn apart(c: char) -> bool {
    c.is_whitespace() || matches!(c, ',' | ';' | '(' | ')' | '[' | ']' | '{' | '}' | '<' | '>')
}

/// Characters that make a word of the user's message an id (spec 087 FR-020).
const ID_MARKS: &[char] = &['-', '_', '.', ':', '@', '/', '#'];

/// A word of the user's message without its surrounding quotes and punctuation, when it holds a
/// digit or an id mark inside; `None` for a plain word, a name or a word with an apostrophe.
fn id_like(word: &str) -> Option<&str> {
    let word = word
        .trim_start_matches(|c: char| !c.is_alphanumeric() && c != '/' && c != '#')
        .trim_end_matches(|c: char| !c.is_alphanumeric());
    let looks = word
        .chars()
        .any(|c| c.is_ascii_digit() || ID_MARKS.contains(&c));
    looks.then_some(word)
}

/// Whether the guard checks the ids of `tool`: an action or an extension tool that changes data,
/// outside the window domain.
fn checks_ids(tool: &dyn Tool) -> bool {
    match tool.action_definition() {
        Some(def) => def.effect != ActionEffect::Read && domain(&def.action_id) != WINDOW_DOMAIN,
        None => tool.origin().is_some() && tool.risk_class() != RiskClass::Safe,
    }
}

/// The part of an action id before the first dot (`contacts.update` → `contacts`).
fn domain(action_id: &str) -> &str {
    action_id.split('.').next().unwrap_or(action_id)
}

pub(crate) fn is_id_property(name: &str) -> bool {
    name == "id" || name == "ids" || name.ends_with("Id") || name.ends_with("Ids")
}

pub(crate) fn push_values(value: &Value, out: &mut Vec<String>) {
    match value {
        Value::String(s) if !s.is_empty() => out.push(s.clone()),
        Value::Number(n) => out.push(n.to_string()),
        Value::Array(items) => items.iter().for_each(|item| push_values(item, out)),
        Value::Object(_) => collect_ids(value, out),
        _ => {}
    }
}

/// The values of every id property in `input`, at any depth (`messages[].id` included).
pub(crate) fn collect_ids(input: &Value, out: &mut Vec<String>) {
    match input {
        Value::Object(map) => {
            for (key, value) in map {
                if is_id_property(key) {
                    push_values(value, out);
                } else {
                    collect_ids(value, out);
                }
            }
        }
        Value::Array(items) => items.iter().for_each(|item| collect_ids(item, out)),
        _ => {}
    }
}

/// Every string in a schema: enums, descriptions and examples.
pub(crate) fn schema_strings(schema: &Value, out: &mut String) {
    match schema {
        Value::String(s) => {
            out.push_str(s);
            out.push('\n');
        }
        Value::Array(items) => items.iter().for_each(|item| schema_strings(item, out)),
        Value::Object(map) => map.values().for_each(|value| schema_strings(value, out)),
        _ => {}
    }
}

fn is_id_char(c: char) -> bool {
    c.is_alphanumeric() || c == '_' || c == '-'
}

/// Whether `text` holds `id` as a whole token, not as part of a longer id or number.
pub(crate) fn holds(text: &str, id: &str) -> bool {
    text.match_indices(id).any(|(start, _)| {
        let before = text[..start].chars().next_back();
        let after = text[start + id.len()..].chars().next();
        !before.is_some_and(is_id_char) && !after.is_some_and(is_id_char)
    })
}

/// The ids of a call of `tool` with `input` that neither `seen` nor the tool's own schema names.
pub fn unknown_ids(input: &Value, tool: &dyn Tool, seen: &Seen) -> Vec<String> {
    if !checks_ids(tool) {
        return Vec::new();
    }
    let mut ids = Vec::new();
    collect_ids(input, &mut ids);
    let mut schema = String::new();
    schema_strings(&tool.input_schema(), &mut schema);
    let mut unknown: Vec<String> = Vec::new();
    for id in ids {
        if !seen.names(&id) && !holds(&schema, &id) && !unknown.contains(&id) {
            unknown.push(id);
        }
    }
    unknown
}

/// The callable reading tools of the domain of `tool`: the same action prefix or extension.
fn readers(tool: &dyn Tool, registry: &ToolRegistry, offered: &[ToolSpec]) -> Vec<String> {
    let same_domain = |other: &dyn Tool| match (tool.action_definition(), other.action_definition())
    {
        (Some(def), Some(other_def)) => domain(&def.action_id) == domain(&other_def.action_id),
        (None, None) => match (tool.origin(), other.origin()) {
            (Some(a), Some(b)) => a.extension_id == b.extension_id,
            _ => false,
        },
        _ => false,
    };
    registry
        .iter()
        .filter(|other| {
            other.risk_class() == RiskClass::Safe
                && same_domain(other.as_ref())
                && callable(other.as_ref(), offered)
        })
        .map(|other| other.name().to_owned())
        .collect()
}

/// Why holzi did not run a call, for the sentence on what to do next (FR-014, research R7).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Failure<'a> {
    /// Ids nobody named; the reading tools of the domain find the right ones.
    UnknownIds { readers: &'a [String] },
    /// A required argument left out, with its description from the schema.
    MissingArgument {
        name: &'a str,
        description: Option<&'a str>,
    },
    /// A tool that does not exist, with the offered tools whose names come closest.
    UnknownTool { closest: &'a [String] },
    /// The user declined the call in the dialog.
    Denied,
}

/// One sentence on what the model should do after `failure`.
pub fn next_step(failure: Failure<'_>) -> String {
    match failure {
        Failure::UnknownIds { readers: [] } => {
            "Search for the right one with find_actions first; never ask the user for an id."
                .to_owned()
        }
        Failure::UnknownIds { readers } => format!(
            "Find the right one first with {}; never ask the user for an id.",
            readers.join(" or ")
        ),
        Failure::MissingArgument {
            name,
            description: Some(description),
        } => format!("Call it again with `{name}`: {description}"),
        Failure::MissingArgument { name, .. } => format!("Call it again with `{name}`."),
        Failure::UnknownTool { closest: [] } => {
            "Call only the offered tools; find_actions finds holzi's other actions.".to_owned()
        }
        Failure::UnknownTool { closest } => format!(
            "Call one of the closest offered tools instead: {}.",
            closest.join(", ")
        ),
        Failure::Denied => "Do not propose it again: answer with what you have, or ask the user \
                            how to go on."
            .to_owned(),
    }
}

/// The answer to a call of `tool` with ids nobody named, or `None` when every id is known.
pub fn check_ids(
    input: &Value,
    tool: &dyn Tool,
    seen: &Seen,
    registry: &ToolRegistry,
    offered: &[ToolSpec],
) -> Option<String> {
    let unknown = unknown_ids(input, tool, seen);
    if unknown.is_empty() {
        return None;
    }
    let readers = readers(tool, registry, offered);
    Some(format!(
        "turned away: no earlier result and nothing the user said names the id {}. {}",
        unknown.join(", "),
        next_step(Failure::UnknownIds { readers: &readers })
    ))
}

/// The answer to a call of `tool` that leaves out a required argument (a top-level `required`
/// property that is absent or null), or `None`.
pub fn check_required(input: &Value, tool: &dyn Tool) -> Option<String> {
    let schema = tool.input_schema();
    let missing = schema["required"]
        .as_array()?
        .iter()
        .filter_map(Value::as_str)
        .find(|name| input.get(*name).is_none_or(Value::is_null))?;
    let description = schema["properties"][missing]["description"].as_str();
    Some(format!(
        "turned away: the required argument `{missing}` is missing. {}",
        next_step(Failure::MissingArgument {
            name: missing,
            description,
        })
    ))
}

/// The guard before the dialog: the answer to a call holzi turns away, or `None` to go on.
pub fn turn_away(
    input: &Value,
    tool: &dyn Tool,
    seen: &Seen,
    registry: &ToolRegistry,
    offered: &[ToolSpec],
) -> Option<String> {
    check_required(input, tool).or_else(|| check_ids(input, tool, seen, registry, offered))
}

/// Up to three offered tools whose names share the most words with `name`, best first.
pub fn closest_tools(name: &str, offered: &[ToolSpec]) -> Vec<String> {
    let wanted = words(name);
    let mut scored: Vec<(usize, usize, &str)> = offered
        .iter()
        .enumerate()
        .filter_map(|(position, spec)| {
            let shared = words(&spec.name)
                .iter()
                .filter(|word| wanted.contains(word))
                .count();
            (shared > 0).then_some((shared, position, spec.name.as_str()))
        })
        .collect();
    scored.sort_by(|a, b| b.0.cmp(&a.0).then(a.1.cmp(&b.1)));
    scored
        .into_iter()
        .take(MAX_CLOSEST)
        .map(|(_, _, name)| name.to_owned())
        .collect()
}

#[cfg(test)]
#[path = "guard_tests.rs"]
mod tests;
