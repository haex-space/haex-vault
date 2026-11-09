//! What counts as personal data in what a page sends (spec 065 FR-011, amended 2026-11-08): the
//! user is asked before data of the person leaves for a website (an account is created, an
//! address, financial data or credentials are sent), not before every form. A search, a filter, a
//! comment preview or a newsletter checkbox send nothing of that kind and run without a question.
//!
//! The rules are word lists over the names the page gives its fields, the `type` and
//! `autocomplete` it declares for them, and a few shapes of value. They are a heuristic: a page
//! can name an address field `x1`. What a heuristic misses is not lost to the user's attention
//! entirely: a request that may change something is still checked at step 2 by its body (see
//! `request_asks` in `submit.rs`), which reads the same names.

use serde_json::Value;

use super::submit::secret_name;

/// One field of a form as the reading script reports it: name, value, whether the page or holzi
/// made it a secret, and what it declares about its kind (its `type` and `autocomplete`).
pub type FormField = (String, String, bool, String);

/// Parts of a field name that name personal data, wherever in the name they are (the name has its
/// words joined, `billing_street` and `billingStreet` are both `billingstreet`).
const NAME_PARTS: &[&str] = &[
    // contact
    "email",
    "phone",
    "telefon",
    "telephone",
    // address
    "street",
    "strasse",
    "straße",
    "zipcode",
    "postcode",
    "postalcode",
    "address",
    "adresse",
    "anschrift",
    // person
    "firstname",
    "lastname",
    "fullname",
    "username",
    "givenname",
    "familyname",
    "surname",
    "vorname",
    "nachname",
    "birthday",
    "birthdate",
    "dateofbirth",
    "geburt",
    "passport",
    "taxid",
    "steuer",
    // money
    "iban",
    "swift",
    "creditcard",
    "cardnumber",
    "cardholder",
    "kreditkarte",
    "kontonummer",
    "bankaccount",
    // credentials
    "cookie",
    "sessionid",
    "password",
    "passwort",
    "passwd",
];

/// Whole words of a field name that name personal data; too short to match inside other words.
const NAME_WORDS: &[&str] = &[
    "name", "mail", "tel", "mobile", "handy", "plz", "zip", "city", "stadt", "bic", "dob", "bday",
    "cc", "konto", "username",
];

/// What a field may declare as its `type` and as the tokens of `autocomplete`, by start.
const HINT_STARTS: &[&str] = &[
    "email",
    "tel",
    "password",
    "new-password",
    "current-password",
    "one-time-code",
    "cc-",
    "address-",
    "street-address",
    "postal-code",
    "country",
    "bday",
    "given-name",
    "family-name",
    "additional-name",
    "honorific",
    "name",
    "username",
];

/// The lowercase words of `text`, split where it has no letter or digit and where camelCase turns
/// upper.
pub(super) fn words(text: &str) -> Vec<String> {
    let mut found = Vec::new();
    let mut word = String::new();
    let mut previous_lower = false;
    for character in text.chars() {
        if !character.is_alphanumeric() {
            if !word.is_empty() {
                found.push(std::mem::take(&mut word));
            }
            previous_lower = false;
            continue;
        }
        if character.is_uppercase() && previous_lower && !word.is_empty() {
            found.push(std::mem::take(&mut word));
        }
        word.extend(character.to_lowercase());
        previous_lower = character.is_lowercase() || character.is_numeric();
    }
    if !word.is_empty() {
        found.push(word);
    }
    found
}

/// Whether the name of a field names personal data or a secret.
pub fn personal_name(name: &str) -> bool {
    if secret_name(name) {
        return true;
    }
    let words = words(name);
    let joined = words.concat();
    NAME_PARTS.iter().any(|part| joined.contains(part))
        || words.iter().any(|word| NAME_WORDS.contains(&word.as_str()))
}

/// Whether what a field declares about its kind names personal data.
pub fn personal_hint(hint: &str) -> bool {
    hint.split_whitespace()
        .any(|token| HINT_STARTS.iter().any(|start| token.starts_with(start)))
}

/// Whether `value` is an e-mail address.
fn email_like(value: &str) -> bool {
    let value = value.trim();
    value.len() <= 254
        && !value.contains(char::is_whitespace)
        && value.split_once('@').is_some_and(|(local, host)| {
            !local.is_empty() && host.contains('.') && !host.ends_with('.')
        })
}

/// Whether `text` holds an e-mail address as one of its words.
fn has_email(text: &str) -> bool {
    text.split_whitespace().any(|word| {
        email_like(word.trim_matches(|c: char| {
            matches!(
                c,
                '.' | ',' | ';' | ':' | '(' | ')' | '<' | '>' | '"' | '\''
            )
        }))
    })
}

/// Whether the field takes a password: a form with one is a login or a registration and sends
/// credentials, filled or not (the field of an empty password still stands for the person's
/// account).
fn takes_password(secret: bool, hint: &str) -> bool {
    secret
        || hint
            .split_whitespace()
            .any(|token| matches!(token, "password" | "new-password" | "current-password"))
}

/// Whether one field sent as it is names or holds personal data. An empty field sends nothing,
/// except a password field, which always makes a form a credentials form.
pub fn field_is_personal(name: &str, value: &str, secret: bool, hint: &str) -> bool {
    if takes_password(secret, hint) {
        return true;
    }
    if value.trim().is_empty() {
        return false;
    }
    personal_hint(hint) || personal_name(name) || has_email(value)
}

/// Whether a form with these fields would send personal data.
pub fn form_carries_personal(fields: &[FormField]) -> bool {
    fields
        .iter()
        .any(|(name, value, secret, hint)| field_is_personal(name, value, *secret, hint))
}

/// How deep a JSON body is searched.
const MAX_DEPTH: usize = 8;

fn json_carries(value: &Value, depth: usize) -> bool {
    if depth > MAX_DEPTH {
        // The rest of this payload is opaque to the classifier. A changing request must ask.
        return true;
    }
    match value {
        Value::Object(object) => {
            // A form sent as JSON names its field in a value (`{"name": "email", "value": "…"}`).
            let pair_names_personal = object
                .values()
                .any(|member| matches!(member, Value::String(text) if !text.contains(char::is_whitespace) && personal_name(text)));
            if pair_names_personal && object.contains_key("value") {
                return true;
            }
            object.iter().any(|(name, member)| match member {
                Value::Null => false,
                Value::String(text) if text.trim().is_empty() => false,
                Value::Object(_) | Value::Array(_) => {
                    personal_name(name) || json_carries(member, depth + 1)
                }
                _ => personal_name(name) || json_carries(member, depth + 1),
            })
        }
        Value::Array(items) => items.iter().any(|item| json_carries(item, depth + 1)),
        Value::String(text) => has_email(text),
        _ => false,
    }
}

/// Whether the text body of a request carries personal data: a JSON body by its members at any
/// depth, a form by its fields, any other text (a multipart form) by the names and addresses in
/// it.
pub fn body_carries_personal(body: &str) -> bool {
    let body = body.trim();
    if body.is_empty() {
        return false;
    }
    if let Ok(json) = serde_json::from_str::<Value>(body) {
        return json_carries(&json, 0);
    }
    if body.contains('=') && !body.contains('\n') && !body.starts_with('{') {
        return url::form_urlencoded::parse(body.as_bytes())
            .any(|(name, value)| field_is_personal(&name, &value, false, ""));
    }
    secret_name(body) || personal_name(body) || body.split_whitespace().any(email_like)
}
