//! `browser.search` (spec 065 revision 2026-11-02, FR-039): a web search in the agent browser. It
//! is `browser.open` of the search engine's result page for the query, so the website rules, the
//! questions and the reading are the same; the model gets the result page with its links.
//!
//! The engine is DuckDuckGo's HTML page: no key, no account, and plain links the page reader
//! numbers. `ponytail:` one fixed engine; upgrade: an engine the user picks in the browser
//! settings.

use serde::Deserialize;
use serde_json::{json, Value};

use super::calls::input;
use crate::chat::tools::native_action::NativeError;

/// The tool's id.
pub const SEARCH: &str = "browser.search";
/// The result page of the default engine; the encoded query follows.
pub const DEFAULT_ENGINE: &str = "https://html.duckduckgo.com/html/?q=";
/// The longest query, in characters.
const MAX_QUERY: usize = 500;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Search {
    query: String,
}

/// The input of `browser.open` for the search `input` on `engine`.
pub fn as_open(engine: &str, input_value: Value) -> Result<Value, NativeError> {
    let search: Search = input(input_value)?;
    let query = search.query.trim();
    if query.is_empty() || query.chars().count() > MAX_QUERY {
        return Err(NativeError::invalid_input(
            Some("query"),
            format!("A search needs 1 to {MAX_QUERY} characters."),
        ));
    }
    let encoded: String = url::form_urlencoded::byte_serialize(query.as_bytes()).collect();
    Ok(json!({ "url": format!("{engine}{encoded}") }))
}

#[cfg(test)]
#[path = "search_tests.rs"]
mod tests;
