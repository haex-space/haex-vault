//! What the user may read about a call (spec 021 FR-022, FR-028, research R10): the inputs as the
//! call prompt and the agent log show them. An input field the schema marks with
//! `"x-holzi-secret": true` becomes `•••`, and so does a credential inside a raw command line or an
//! argument list (`--token=abc`, `--password abc`, `GITHUB_TOKEN=abc`, `Bearer abc`,
//! `https://user:pw@host`), whatever the schema says. Values over 200 characters are cut.

use serde_json::{Map, Value};

/// What stands in for a secret.
pub const SECRET_PLACEHOLDER: &str = "•••";

/// The longest value the log and the prompt keep.
pub const MAX_VALUE_CHARS: usize = 200;

/// The fields that name the target of a call, in the order they are tried.
const TARGET_FIELDS: [&str; 4] = ["target", "id", "path", "name"];

/// The inputs of a call as the user may read them.
pub fn redact(schema: &Value, input: &Value) -> Value {
    match input {
        Value::Object(fields) => Value::Object(redact_fields(schema, fields)),
        Value::Array(items) => redact_list(schema.get("items").unwrap_or(&Value::Null), items),
        Value::String(text) => Value::String(cut(&redact_text(text))),
        other => other.clone(),
    }
}

fn redact_fields(schema: &Value, fields: &Map<String, Value>) -> Map<String, Value> {
    let properties = schema.get("properties");
    fields
        .iter()
        .map(|(name, value)| {
            let field = properties
                .and_then(|properties| properties.get(name))
                .unwrap_or(&Value::Null);
            let secret = field.get("x-holzi-secret").and_then(Value::as_bool) == Some(true);
            let shown = if secret {
                Value::String(SECRET_PLACEHOLDER.to_owned())
            } else {
                redact(field, value)
            };
            (name.clone(), shown)
        })
        .collect()
}

/// An argument list: a secret flag without `=` hides the argument after it.
fn redact_list(items: &Value, list: &[Value]) -> Value {
    let mut hide_next = false;
    let shown = list
        .iter()
        .map(|value| {
            if std::mem::take(&mut hide_next) {
                return Value::String(SECRET_PLACEHOLDER.to_owned());
            }
            if let Value::String(text) = value {
                hide_next = is_secret_flag(text);
            }
            redact(items, value)
        })
        .collect();
    Value::Array(shown)
}

/// The readable target of a call: the first of `target`, `id`, a field ending in `Id`, `path` or
/// `name`. Takes the inputs [`redact`] returned, so a secret never becomes a target.
pub fn target_of(shown: &Value) -> Option<String> {
    let fields = shown.as_object()?;
    let named = |name: &str| fields.get(name);
    let id_like = || {
        fields
            .iter()
            .find(|(name, _)| name.ends_with("Id"))
            .map(|(_, value)| value)
    };
    let value = named(TARGET_FIELDS[0])
        .or_else(|| named(TARGET_FIELDS[1]))
        .or_else(id_like)
        .or_else(|| named(TARGET_FIELDS[2]))
        .or_else(|| named(TARGET_FIELDS[3]))?;
    match value {
        Value::String(text) => Some(cut(text)),
        Value::Number(number) => Some(number.to_string()),
        _ => None,
    }
}

/// Cuts `text` to [`MAX_VALUE_CHARS`] characters, marking the cut with `…`.
pub fn cut(text: &str) -> String {
    if text.chars().count() <= MAX_VALUE_CHARS {
        return text.to_owned();
    }
    let mut kept: String = text.chars().take(MAX_VALUE_CHARS - 1).collect();
    kept.push('…');
    kept
}

/// Hides the credentials in a raw command line, word by word; the spacing stays.
pub fn redact_text(text: &str) -> String {
    let mut shown = String::with_capacity(text.len());
    let mut hide_next = false;
    let mut rest = text;
    while !rest.is_empty() {
        let space = rest.len() - rest.trim_start().len();
        shown.push_str(&rest[..space]);
        rest = &rest[space..];
        let end = rest.find(char::is_whitespace).unwrap_or(rest.len());
        let word = &rest[..end];
        rest = &rest[end..];
        if word.is_empty() {
            continue;
        }
        if std::mem::take(&mut hide_next) {
            shown.push_str(SECRET_PLACEHOLDER);
            continue;
        }
        hide_next = is_secret_flag(word) || is_scheme(word);
        shown.push_str(&redact_word(word));
    }
    shown
}

/// One word: `name=value` with a secret name, and the password of a URL.
fn redact_word(word: &str) -> String {
    if let Some((name, _)) = word.split_once('=') {
        if is_secret_name(name) {
            return format!("{name}={SECRET_PLACEHOLDER}");
        }
    }
    redact_url_password(word)
}

/// `scheme://user:password@host` → `scheme://user:•••@host`.
fn redact_url_password(word: &str) -> String {
    let Some(start) = word.find("://").map(|at| at + 3) else {
        return word.to_owned();
    };
    let authority_end = word[start..].find('/').map_or(word.len(), |at| start + at);
    let Some(at) = word[start..authority_end].rfind('@').map(|at| start + at) else {
        return word.to_owned();
    };
    match word[start..at].find(':').map(|colon| start + colon) {
        Some(colon) => format!("{}{SECRET_PLACEHOLDER}{}", &word[..=colon], &word[at..]),
        None => word.to_owned(),
    }
}

/// `--token`, `-password`, `x-api-key:`: a flag or header whose value follows as the next word.
fn is_secret_flag(word: &str) -> bool {
    if word.contains('=') {
        return false;
    }
    let flag = word.starts_with('-');
    let header = word.ends_with(':');
    (flag || header) && is_secret_name(word.trim_end_matches(':'))
}

/// `Bearer` and `Basic`: the credential follows.
fn is_scheme(word: &str) -> bool {
    word.eq_ignore_ascii_case("bearer") || word.eq_ignore_ascii_case("basic")
}

/// Whether a flag, variable or header name names a credential (`--api-key`, `GITHUB_TOKEN`,
/// `db_password`, `auth`).
fn is_secret_name(name: &str) -> bool {
    const ENDINGS: [&str; 9] = [
        "token",
        "password",
        "passwd",
        "pwd",
        "secret",
        "apikey",
        "api-key",
        "credential",
        "credentials",
    ];
    let name = name
        .trim_start_matches('-')
        .to_ascii_lowercase()
        .replace('_', "-");
    name == "auth" || name == "pass" || ENDINGS.iter().any(|ending| name.ends_with(ending))
}
