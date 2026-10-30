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

/// The fields of a held request's body: form fields, the top level of a JSON object, or nothing
/// when the body is something else.
pub fn fields_of(body: Option<&str>) -> Vec<BrowserField> {
    let Some(body) = body.map(str::trim).filter(|body| !body.is_empty()) else {
        return Vec::new();
    };
    let fields: Vec<BrowserField> = if body.starts_with('{') {
        match serde_json::from_str::<Value>(body) {
            Ok(Value::Object(object)) => object
                .iter()
                .map(|(name, value)| {
                    let value = match value {
                        Value::String(text) => text.clone(),
                        Value::Array(_) | Value::Object(_) => SECRET_PLACEHOLDER.to_owned(),
                        other => other.to_string(),
                    };
                    shown(name, &value, false)
                })
                .collect(),
            _ => Vec::new(),
        }
    } else if body.contains('=') && !body.contains('\n') {
        url::form_urlencoded::parse(body.as_bytes())
            .map(|(name, value)| shown(&name, &value, false))
            .collect()
    } else {
        Vec::new()
    };
    fields.into_iter().take(MAX_FIELDS).collect()
}
