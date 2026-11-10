//! Values a model writes in a form its tool does not expect but whose meaning is unique: `"True"`
//! for a yes/no, `"5"` for a number. Taken as meant before the guard, so the dialog, the tool and
//! the stored call see the same value (spec 087 US4, contracts/value-forms.md). Anything else stays
//! as sent and meets the tool's own check.

use serde_json::{Map, Number, Value};

/// `input` with each top-level argument turned into the type its property in `schema` declares,
/// when that type is a single `boolean`, `integer` or `number` and the text means one value only.
pub fn as_meant(input: Value, schema: &Value) -> Value {
    let Value::Object(arguments) = input else {
        return input;
    };
    let properties = &schema["properties"];
    let turned: Map<String, Value> = arguments
        .into_iter()
        .map(|(name, value)| {
            let value = match (&value, properties[&name]["type"].as_str()) {
                (Value::String(text), Some(kind)) => meant(text, kind).unwrap_or(value),
                _ => value,
            };
            (name, value)
        })
        .collect();
    Value::Object(turned)
}

fn meant(text: &str, kind: &str) -> Option<Value> {
    match kind {
        "boolean" => match text.to_ascii_lowercase().as_str() {
            "true" => Some(Value::Bool(true)),
            "false" => Some(Value::Bool(false)),
            _ => None,
        },
        "integer" => integer(text),
        "number" => integer(text).or_else(|| decimal(text)),
        _ => None,
    }
}

/// An optional `-`, then digits only.
fn integer(text: &str) -> Option<Value> {
    let digits = text.strip_prefix('-').unwrap_or(text);
    if digits.is_empty() || !digits.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    text.parse::<i64>().ok().map(Value::from)
}

/// An optional `-`, digits, a point and digits; no exponent, no `inf` or `NaN`.
fn decimal(text: &str) -> Option<Value> {
    let unsigned = text.strip_prefix('-').unwrap_or(text);
    let (whole, fraction) = unsigned.split_once('.')?;
    let digits = |part: &str| !part.is_empty() && part.bytes().all(|b| b.is_ascii_digit());
    if !digits(whole) || !digits(fraction) {
        return None;
    }
    text.parse::<f64>()
        .ok()
        .and_then(Number::from_f64)
        .map(Value::Number)
}

#[cfg(test)]
#[path = "arg_forms_tests.rs"]
mod tests;
