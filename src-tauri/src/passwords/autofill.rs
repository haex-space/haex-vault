//! What an entry offers to fill a form with (spec 049 FR-020, spec 065 research R7): username,
//! password, the custom fields and the marker `otp: "TOTP"`, all as text, and the entry's own
//! autofill aliases (036 FR-001a). One rule for the External Bridge and the agent browser.

use serde_json::{Map, Value};

use super::model::SecretItem;

/// The key that marks a TOTP; its value is never a value to type.
pub const OTP_MARKER: &str = "otp";

/// The fields of `item` that hold a value. A custom field never replaces one of the three.
pub fn fields_of(item: &SecretItem) -> Map<String, Value> {
    let mut fields = Map::new();
    for key_value in &item.key_values {
        if let (Some(key), Some(value)) = (&key_value.key, &key_value.value) {
            if !key.is_empty() {
                fields.insert(key.clone(), Value::String(value.clone()));
            }
        }
    }
    for (name, value) in [("username", &item.username), ("password", &item.password)] {
        match value {
            Some(value) if !value.is_empty() => {
                fields.insert(name.to_owned(), Value::String(value.clone()));
            }
            _ => {
                fields.remove(name);
            }
        }
    }
    if item.otp_secret.is_some() {
        fields.insert(OTP_MARKER.to_owned(), Value::String("TOTP".to_owned()));
    } else {
        fields.remove(OTP_MARKER);
    }
    fields
}

/// The entry's own autofill aliases as an object of key → names, or `null`.
pub fn aliases_of(item: &SecretItem) -> Value {
    item.autofill_aliases
        .as_deref()
        .and_then(|text| serde_json::from_str::<Value>(text).ok())
        .filter(Value::is_object)
        .unwrap_or(Value::Null)
}
