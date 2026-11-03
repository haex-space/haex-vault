//! What counts as submitting on a website (spec 065 FR-011, research R5). Step 1 decides before a
//! click or an Enter from what the reading script reports about the target; step 2 holds every
//! request that may change something (`POST`, `PUT`, `PATCH`, `DELETE`) until the user allowed
//! that one request. When in doubt, it is submitting.
//!
//! The rules live here, not in the page script, so they are one implementation with unit tests; the
//! script only reports the facts ([`Facts`]).

use serde::{Deserialize, Serialize};
use serde_json::Value;
use ts_rs::TS;

use crate::mcp_server::redact::{cut, redact_text, SECRET_PLACEHOLDER};

/// What the reading script reports about an element, for step 1.
#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize)]
#[serde(default)]
pub struct Facts {
    /// `button`, `input`, `a`, …
    pub tag: String,
    /// The `type` attribute as the element has it, lowercase; empty when missing.
    #[serde(rename = "type")]
    pub kind: String,
    /// The element belongs to a form.
    pub form: bool,
}

/// Starts of words that name submitting (de, en). A word of a name that starts with one counts.
// ponytail: a word list is a heuristic and misses buttons named otherwise ("Weiter" on the last
// page of a checkout); step 2 holds the request such a button sends. Upgrade path: more languages
// with the window's languages.
const WORDS: &[&str] = &[
    // de
    "kauf",
    "bestell",
    "bezahl",
    "zahl",
    "send",
    "absend",
    "abschick",
    "lösch",
    "entfern",
    "veröffentlich",
    "post",
    "buch",
    "bestätig",
    "kündig",
    "überweis",
    // en
    "buy",
    "order",
    "pay",
    "checkout",
    "submit",
    "delete",
    "remove",
    "publish",
    "book",
    "confirm",
    "transfer",
    "unsubscribe",
];

/// Phrases that name submitting although none of their words does.
const PHRASES: &[&str] = &["cancel subscription", "check out"];

/// Whether `name` names submitting.
pub fn named_submitting(name: &str) -> bool {
    let name = name.to_lowercase();
    if PHRASES.iter().any(|phrase| name.contains(phrase)) {
        return true;
    }
    name.split(|c: char| !c.is_alphanumeric())
        .filter(|word| !word.is_empty())
        .any(|word| WORDS.iter().any(|start| word.starts_with(start)))
}

/// Step 1: whether a click on the element with `facts` and accessible name `name` submits.
pub fn click_submits(facts: &Facts, name: &str) -> bool {
    let submits_form = facts.form
        && match facts.tag.as_str() {
            "input" => matches!(facts.kind.as_str(), "submit" | "image"),
            // A button without a type submits its form.
            "button" => matches!(facts.kind.as_str(), "" | "submit"),
            _ => false,
        };
    submits_form || named_submitting(name)
}

/// Step 1 for an element of `kind` (as the reading script names it): typing into a field or
/// ticking a box is no click that submits; for everything else see [`click_submits`].
pub fn element_submits(kind: &str, facts: &Facts, name: &str) -> bool {
    !matches!(kind, "field" | "select" | "checkbox" | "radio") && click_submits(facts, name)
}

/// Whether a request with `method` may change something on a website (step 2).
pub fn changing(method: &str) -> bool {
    matches!(
        method.to_ascii_uppercase().as_str(),
        "POST" | "PUT" | "PATCH" | "DELETE"
    )
}

/// One field the dialog shows of what would be sent.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/types/bindings/")]
pub struct BrowserField {
    pub name: String,
    /// `•••` for secrets and password fields.
    pub value: String,
}

/// Starts of field names whose value is a secret.
const SECRET_NAMES: &[&str] = &[
    "pass",
    "pwd",
    "secret",
    "token",
    "otp",
    "pin",
    "cvc",
    "cvv",
    "csc",
    "card",
    "iban",
    "auth",
    "key",
    "credential",
    "private",
    "bearer",
    "ssn",
];

/// The most fields a dialog lists.
const MAX_FIELDS: usize = 30;

fn secret_name(name: &str) -> bool {
    let mut words = String::new();
    let mut previous_lower_or_digit = false;
    for character in name.chars() {
        if !character.is_alphanumeric() {
            words.push(' ');
            previous_lower_or_digit = false;
            continue;
        }
        if character.is_uppercase() && previous_lower_or_digit {
            words.push(' ');
        }
        words.extend(character.to_lowercase());
        previous_lower_or_digit = character.is_lowercase() || character.is_numeric();
    }
    words.split_whitespace().any(|word| {
        SECRET_NAMES
            .iter()
            .any(|stem| word == *stem || (stem.chars().count() >= 4 && word.contains(stem)))
    })
}

/// A field as the dialog may show it: a secret by its name or as the page says, cut to length.
pub fn shown(name: &str, value: &str, secret: bool) -> BrowserField {
    let value = if secret || secret_name(name) {
        SECRET_PLACEHOLDER.to_owned()
    } else {
        cut(&redact_text(value))
    };
    BrowserField {
        name: cut(name),
        value,
    }
}

/// `value` with every secret hidden, to show it whole: a member named like a secret becomes `•••`,
/// strings lose the credentials they carry.
fn hide_secrets(value: &Value) -> Value {
    match value {
        Value::Object(object) => Value::Object(
            object
                .iter()
                .map(|(name, value)| {
                    let shown = if secret_name(name) {
                        Value::String(SECRET_PLACEHOLDER.to_owned())
                    } else {
                        hide_secrets(value)
                    };
                    (name.clone(), shown)
                })
                .collect(),
        ),
        Value::Array(items) => Value::Array(items.iter().map(hide_secrets).collect()),
        Value::String(text) => Value::String(redact_text(text)),
        other => other.clone(),
    }
}

/// A JSON member as the dialog shows it: text as it is, anything nested as compact JSON with its
/// secrets hidden, cut to length.
fn json_field(name: &str, value: &Value) -> BrowserField {
    match value {
        Value::String(text) => shown(name, text, false),
        Value::Object(_) | Value::Array(_) => shown(name, &hide_secrets(value).to_string(), false),
        other => shown(name, &other.to_string(), false),
    }
}

/// Whether `body` is text a person can read: no control characters but line breaks and tabs.
fn readable(body: &str) -> bool {
    !body
        .chars()
        .any(|c| c.is_control() && !matches!(c, '\n' | '\r' | '\t'))
}

/// The fields of a held request's body: form fields, the top level of a JSON object, a JSON list
/// or any other text as one field without a name (the dialog names it); nothing when the body is
/// not text.
pub fn fields_of(body: Option<&str>) -> Vec<BrowserField> {
    let Some(body) = body.map(str::trim).filter(|body| !body.is_empty()) else {
        return Vec::new();
    };
    if !readable(body) {
        return Vec::new();
    }
    let fields: Vec<BrowserField> = match serde_json::from_str::<Value>(body) {
        Ok(Value::Object(object)) => object
            .iter()
            .map(|(name, value)| json_field(name, value))
            .collect(),
        Ok(list @ Value::Array(_)) => vec![json_field("", &list)],
        _ if body.contains('=') && !body.contains('\n') && !body.starts_with('{') => {
            url::form_urlencoded::parse(body.as_bytes())
                .map(|(name, value)| shown(&name, &value, false))
                .collect()
        }
        // Text of no known shape may carry a secret anywhere: shown only when nothing in it is named
        // like one.
        _ => vec![shown("", body, secret_name(body))],
    };
    fields.into_iter().take(MAX_FIELDS).collect()
}
