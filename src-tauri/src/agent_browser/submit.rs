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

use super::personal::{body_carries_personal, form_carries_personal, FormField};
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

/// Starts of words that name an action that cannot be taken back or costs money (de, en): buying,
/// paying, ordering, booking, transferring, deleting, removing, publishing, cancelling a contract.
/// A word of a name that starts with one counts. Plain sending, confirming and logging out are no
/// part of it (spec 065 FR-011, amended 2026-11-08): they ask only when personal data goes with
/// them (`personal`).
// ponytail: a word list is a heuristic and misses buttons named otherwise ("Weiter" on the last
// page of a checkout); step 2 holds the request such a button sends. Upgrade path: more languages
// with the window's languages.
const WORDS: &[&str] = &[
    // de
    "kauf",
    "bestell",
    "bezahl",
    "zahl",
    "lösch",
    "entfern",
    "veröffentlich",
    "post",
    "buch",
    "kündig",
    "überweis",
    // en
    "buy",
    "order",
    "pay",
    "checkout",
    "delete",
    "remove",
    "publish",
    "book",
    "transfer",
    "donat",
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

/// Whether the element with `facts` submits its form when it is clicked.
fn submits_form(facts: &Facts) -> bool {
    facts.form
        && match facts.tag.as_str() {
            "input" => matches!(facts.kind.as_str(), "submit" | "image"),
            // A button without a type submits its form.
            "button" => matches!(facts.kind.as_str(), "" | "submit"),
            _ => false,
        }
}

/// Whether a click on the element with `facts` and accessible name `name` may submit: the mark
/// `submits` in the reading of a page, which only warns that a question may follow. Whether it
/// does is [`element_asks`].
pub fn click_submits(facts: &Facts, name: &str) -> bool {
    submits_form(facts) || named_submitting(name)
}

/// Step 1 for an element of `kind` (as the reading script names it): typing into a field or
/// ticking a box is no click that submits; for everything else see [`click_submits`].
pub fn element_submits(kind: &str, facts: &Facts, name: &str) -> bool {
    !matches!(kind, "field" | "select" | "checkbox" | "radio") && click_submits(facts, name)
}

/// Step 1: whether a click on an element of `kind` asks the user (spec 065 FR-011, amended
/// 2026-11-08): when its name is that of an action that costs money or cannot be undone, or when
/// it belongs to a form that carries personal data (`fields` of that form). Typing and ticking
/// never ask here.
pub fn element_asks(kind: &str, facts: &Facts, name: &str, fields: &[FormField]) -> bool {
    !matches!(kind, "field" | "select" | "checkbox" | "radio")
        && (named_submitting(name) || (facts.form && form_carries_personal(fields)))
}

/// Path segments that name a purchase, payment, transfer, donation, subscription or deletion,
/// whatever the body says. Hyphenated and underscored segments are checked by their parts too.
const SENSITIVE_PATHS: &[&str] = &[
    "checkout",
    "payment",
    "payments",
    "pay",
    "purchase",
    "purchases",
    "buy",
    "order",
    "orders",
    "register",
    "registration",
    "registrations",
    "signup",
    "sign-up",
    "sign_up",
    "registrieren",
    "registrierung",
    "bestellen",
    "bestellung",
    "kasse",
    "bezahlen",
    "kaufen",
    "delete",
    "loeschen",
    "löschen",
    "publish",
    "cancel",
    "cancellation",
    "unsubscribe",
    "subscribe",
    "subscription",
    "transfer",
    "transfers",
    "donate",
    "donation",
    "donations",
    "charge",
];

fn path_is_sensitive(url: &str) -> bool {
    url::Url::parse(url).is_ok_and(|url| {
        url.path_segments().is_some_and(|mut segments| {
            segments.any(|segment| {
                let segment = percent_encoding::percent_decode_str(segment)
                    .decode_utf8_lossy()
                    .to_lowercase();
                SENSITIVE_PATHS.contains(&segment.as_str())
                    || segment
                        .split(['-', '_', '/'])
                        .any(|part| SENSITIVE_PATHS.contains(&part))
            })
        })
    })
}

fn query_is_sensitive(url: &str) -> bool {
    url::Url::parse(url).is_ok_and(|url| {
        url.query_pairs()
            .any(|(name, value)| named_submitting(&name) || named_submitting(&value))
    })
}

/// Step 2: whether a held request asks the user (spec 065 FR-011, amended 2026-11-08). One that
/// cannot change anything goes. One that may goes too unless it deletes, goes to an address that
/// names a purchase, payment, transfer, donation, subscription or deletion, carries personal data
/// in its body, or has a body
/// that is no text and so cannot be told.
pub fn request_asks(method: &str, url: &str, body: Option<&str>) -> bool {
    request_asks_with_body(method, url, body, body.is_some())
}

/// Like [`request_asks`], preserving the distinction between no body and an opaque body.
pub fn request_asks_with_body(method: &str, url: &str, body: Option<&str>, has_body: bool) -> bool {
    if !changing(method) {
        return false;
    }
    if method.eq_ignore_ascii_case("DELETE")
        || path_is_sensitive(url)
        || query_is_sensitive(url)
        || (has_body && body.is_none())
    {
        return true;
    }
    match body.map(str::trim).filter(|body| !body.is_empty()) {
        None => false,
        Some(body) => !readable(body) || body_carries_personal(body),
    }
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

pub(super) fn secret_name(name: &str) -> bool {
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

/// Whether `value` is a word that names a secret, as the name of a name/value pair does
/// (`{"name": "password", "value": "…"}`, a form sent as JSON).
fn names_secret(value: &Value) -> bool {
    matches!(value, Value::String(text)
        if !text.contains(|c: char| c.is_whitespace() || c == '/') && secret_name(text))
}

/// `value` with every secret hidden, to show it whole: a member named like a secret becomes `•••`,
/// and so does every other member of an object that names a secret in a value (a name/value pair);
/// strings lose the credentials they carry.
fn hide_secrets(value: &Value) -> Value {
    match value {
        Value::Object(object) => {
            let pair = object.values().any(names_secret);
            Value::Object(
                object
                    .iter()
                    .map(|(name, value)| {
                        let shown = if secret_name(name) || (pair && !names_secret(value)) {
                            Value::String(SECRET_PLACEHOLDER.to_owned())
                        } else {
                            hide_secrets(value)
                        };
                        (name.clone(), shown)
                    })
                    .collect(),
            )
        }
        Value::Array(items) => Value::Array(items.iter().map(hide_secrets).collect()),
        Value::String(text) => Value::String(redact_text(text)),
        other => other.clone(),
    }
}

/// A member of a JSON body whose secrets are hidden, as the dialog shows it: text as it is,
/// anything nested as compact JSON, cut to length.
fn json_field(name: &str, value: &Value) -> BrowserField {
    match value {
        Value::String(text) => shown(name, text, false),
        other => shown(name, &other.to_string(), false),
    }
}

/// Whether `body` is text a person can read: no control characters but line breaks and tabs.
pub(super) fn readable(body: &str) -> bool {
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
        Ok(object @ Value::Object(_)) => match hide_secrets(&object) {
            Value::Object(hidden) => hidden
                .iter()
                .map(|(name, value)| json_field(name, value))
                .collect(),
            _ => Vec::new(),
        },
        Ok(list @ Value::Array(_)) => vec![json_field("", &hide_secrets(&list))],
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
