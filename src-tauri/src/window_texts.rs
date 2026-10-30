//! Texts of holzi's window that Rust shows itself (system notifications, the marker in an agent
//! tab): read from the window's own locale files, so every text has one source.

use serde_json::Value;

const DE: &str = include_str!("../../src/i18n/locales/de.json");
const EN: &str = include_str!("../../src/i18n/locales/en.json");

/// The text at `key` (`agentBrowser.marker.stop`) in the language of holzi's window (German, else
/// English), with `{name}` placeholders filled; the key itself when it is missing.
pub fn text(locale: &str, key: &str, values: &[(&str, &str)]) -> String {
    let source = if locale.starts_with("de") { DE } else { EN };
    let pointer = format!("/{}", key.replace('.', "/"));
    let found = serde_json::from_str::<Value>(source)
        .ok()
        .and_then(|texts| {
            texts
                .pointer(&pointer)
                .and_then(Value::as_str)
                .map(str::to_owned)
        });
    let mut text = found.unwrap_or_else(|| key.to_owned());
    for (name, value) in values {
        text = text.replace(&format!("{{{name}}}"), value);
    }
    text
}
