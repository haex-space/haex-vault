//! What holzi wrote into a page from the password manager never flows back to the agent (spec 065
//! FR-019, FR-022, FR-038, research R7). A session remembers the values it filled; every reply to
//! the agent and every question shown with page content passes [`Filled::scrub_json`], which
//! replaces each value, also URL-, Base64- and HTML-entity-encoded, with `•••`. From the first
//! fill on, screenshots stay refused for the rest of the session.
//!
//! ponytail: values shorter than [`MIN_CHARS`] are not searched for, since removing every
//! occurrence of one or two characters would erase the page. Ceiling: a one- or two-character
//! secret could show in page text. Upgrade path: none needed while the password manager refuses
//! such secrets.

use base64::engine::general_purpose::{STANDARD, STANDARD_NO_PAD, URL_SAFE, URL_SAFE_NO_PAD};
use base64::Engine;
use serde_json::Value;

use crate::chat::tools::native_action::{NativeError, NativeReply};
use crate::mcp_server::redact::SECRET_PLACEHOLDER;

/// Values shorter than this are not searched for.
pub const MIN_CHARS: usize = 3;

/// The values filled into the pages of one session.
#[derive(Debug, Clone, Default)]
pub struct Filled {
    /// Something was filled, even a value too short to search for.
    any: bool,
    /// Every spelling to remove, longest first.
    spellings: Vec<String>,
}

impl Filled {
    /// Whether anything was filled; screenshots are refused from then on.
    pub fn any(&self) -> bool {
        self.any
    }

    /// Remembers `value` and its encodings.
    pub fn add(&mut self, value: &str) {
        if value.is_empty() {
            return;
        }
        self.any = true;
        if value.chars().count() < MIN_CHARS {
            return;
        }
        for spelling in spellings(value) {
            if !self.spellings.contains(&spelling) {
                self.spellings.push(spelling);
            }
        }
        self.spellings
            .sort_by_key(|spelling| std::cmp::Reverse(spelling.len()));
    }

    /// `text` with every filled value replaced by `•••`.
    pub fn scrub(&self, text: &str) -> String {
        let mut text = text.to_owned();
        for spelling in &self.spellings {
            if text.contains(spelling.as_str()) {
                text = text.replace(spelling.as_str(), SECRET_PLACEHOLDER);
            }
        }
        text
    }

    /// Whether `text` holds a filled value.
    pub fn shows(&self, text: &str) -> bool {
        self.spellings
            .iter()
            .any(|spelling| text.contains(spelling.as_str()))
    }

    /// `value` with [`Self::scrub`] applied to every string in it, keys included.
    pub fn scrub_json(&self, value: &Value) -> Value {
        match value {
            Value::String(text) => Value::String(self.scrub(text)),
            Value::Array(items) => Value::Array(items.iter().map(|v| self.scrub_json(v)).collect()),
            Value::Object(object) => Value::Object(
                object
                    .iter()
                    .map(|(key, v)| (self.scrub(key), self.scrub_json(v)))
                    .collect(),
            ),
            other => other.clone(),
        }
    }

    /// `outcome` without anything filled: every string of the reply and the message of an error.
    pub fn scrub_outcome(
        &self,
        outcome: Result<NativeReply, NativeError>,
    ) -> Result<NativeReply, NativeError> {
        if !self.any {
            return outcome;
        }
        match outcome {
            Ok(mut reply) => {
                reply.result = self.scrub_json(&reply.result);
                reply.target = reply.target.map(|target| self.scrub(&target));
                Ok(reply)
            }
            Err(mut error) => {
                error.message = self.scrub(&error.message);
                Err(error)
            }
        }
    }

    /// Refuses a screenshot once anything was filled.
    ///
    /// ponytail: refused for the whole session, since holzi cannot tell whether a page shows a
    /// filled value somewhere. Ceiling: no screenshots after a login. Upgrade path: a new session
    /// (a new profile run) lifts it.
    pub fn allows_screenshots(&self) -> Result<(), NativeError> {
        if !self.any {
            return Ok(());
        }
        Err(NativeError::new(
            "screenshot_blocked",
            "Screenshots are off for the rest of this session, since data from the password \
             manager was filled in. Read the page instead.",
        ))
    }
}

/// `value` as it may appear in a page: as is, URL-encoded (both forms of a space), Base64 (all
/// four alphabets) and with HTML entities (named, decimal, hexadecimal).
pub fn spellings(value: &str) -> Vec<String> {
    let bytes = value.as_bytes();
    let form: String = url::form_urlencoded::byte_serialize(bytes).collect();
    let mut all = vec![
        value.to_owned(),
        form.clone(),
        form.replace('+', "%20"),
        STANDARD.encode(bytes),
        STANDARD_NO_PAD.encode(bytes),
        URL_SAFE.encode(bytes),
        URL_SAFE_NO_PAD.encode(bytes),
        html_named(value),
        value.chars().map(|c| format!("&#{};", c as u32)).collect(),
        value
            .chars()
            .map(|c| format!("&#x{:x};", c as u32))
            .collect(),
    ];
    all.sort();
    all.dedup();
    all
}

fn html_named(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&#39;")
}
