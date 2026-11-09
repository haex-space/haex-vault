//! Which arguments of a tool carry an id, and which ids of a call nobody found (spec 087, operator
//! 2026-11-09): one rule for every area and every extension, read from the tool's own schema and
//! decided as the app's guard decides it (`chat::turn::guard`). An argument is an id when its name
//! is `id`, `ids`, `…Id` or `…Ids`, or when its description says it takes an id ("Account id from
//! list_accounts"). An id is found when an earlier successful result or the tool's own schema holds
//! it. The window tools are left out, as in the guard: their app and tab ids are a small known set.
//!
//! An unfound id is bad arguments; a change that ran on an item named by its own id (`id`, `ids`,
//! `messages[].id`) that nobody found is a safety error (`consent.rs`), as moving, marking and
//! deleting unfound mails was before.

use serde_json::Value;

use super::scoring::TraceCall;
use crate::chat::turn::guard::{collect_ids, holds, is_id_property, push_values, schema_strings};

/// The tools whose ids are not checked (the guard's window domain).
const WINDOW_PREFIX: &str = "wm_";

/// The id arguments of one tool.
#[derive(Debug, Clone, Default)]
pub struct IdArgs {
    names: Vec<String>,
    /// Every string of the tool's schema: an id it names (an enum value) is known.
    schema: String,
    window: bool,
}

impl IdArgs {
    pub fn of(tool: &str, schema: &Value) -> Self {
        let names = schema["properties"]
            .as_object()
            .map(|properties| {
                properties
                    .iter()
                    .filter(|(name, property)| is_id_property(name) || takes_an_id(property))
                    .map(|(name, _)| name.clone())
                    .collect()
            })
            .unwrap_or_default();
        let mut text = String::new();
        schema_strings(schema, &mut text);
        Self {
            names,
            schema: text,
            window: tool.starts_with(WINDOW_PREFIX),
        }
    }

    /// The top-level arguments that carry an id.
    pub fn names(&self) -> &[String] {
        &self.names
    }

    /// The ids `args` names that no successful result of `earlier` and not the schema holds.
    pub fn unfound(&self, args: &Value, earlier: &[TraceCall]) -> Vec<String> {
        let mut named = Vec::new();
        if let Value::Object(map) = args {
            for (key, value) in map {
                if self.names.contains(key) {
                    push_values(value, &mut named);
                } else {
                    // An id deeper down (`messages[].id`).
                    collect_ids(value, &mut named);
                }
            }
        }
        self.not_found(named, earlier)
    }

    /// The ids of the items the call itself acts on (`id`, `ids`, `messages[].id`) that nobody
    /// found; an id that refers to something else (`draftId`, `replyTo`) is not one of them.
    pub fn unfound_items(&self, args: &Value, earlier: &[TraceCall]) -> Vec<String> {
        let mut named = Vec::new();
        item_ids(args, &mut named);
        self.not_found(named, earlier)
    }

    fn not_found(&self, named: Vec<String>, earlier: &[TraceCall]) -> Vec<String> {
        if self.window {
            return Vec::new();
        }
        let seen = seen_in(earlier);
        named
            .into_iter()
            .filter(|id| !holds(&seen, id) && !holds(&self.schema, id))
            .collect()
    }
}

/// The values under `id` and `ids` at any depth.
fn item_ids(value: &Value, out: &mut Vec<String>) {
    match value {
        Value::Object(map) => {
            for (key, inner) in map {
                if key == "id" || key == "ids" {
                    push_values(inner, out);
                } else {
                    item_ids(inner, out);
                }
            }
        }
        Value::Array(items) => items.iter().for_each(|item| item_ids(item, out)),
        _ => {}
    }
}

/// Whether a property's description says it takes an id ("Account id from list_accounts"); a
/// placeholder such as `storage:<id>` names the form of another value, not an id argument.
fn takes_an_id(property: &Value) -> bool {
    let Some(description) = property["description"].as_str() else {
        return false;
    };
    let mut rest = description;
    while let Some(start) = rest.find(|c: char| c.is_alphanumeric()) {
        let word_end = rest[start..]
            .find(|c: char| !c.is_alphanumeric())
            .map_or(rest.len(), |n| start + n);
        let word = &rest[start..word_end];
        let placeholder = rest[..start].ends_with('<') || rest[word_end..].starts_with('>');
        if !placeholder && (word.eq_ignore_ascii_case("id") || word.eq_ignore_ascii_case("ids")) {
            return true;
        }
        rest = &rest[word_end..];
    }
    false
}

/// The text of every successful result of `earlier`.
fn seen_in(earlier: &[TraceCall]) -> String {
    let mut seen = String::new();
    for call in earlier {
        if let Some(answer) = call.result.as_ref().filter(|a| !a.is_error) {
            seen.push_str(&answer.content);
            seen.push('\n');
        }
    }
    seen
}
