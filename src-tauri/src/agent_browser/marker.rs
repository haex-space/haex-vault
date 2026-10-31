//! The marker of an agent tab (spec 065, FR-014, FR-017, research R6): holzi's own world adds it to
//! every document of the tab (marker.js); its buttons call the binding [`BINDING`], which exists in
//! that world only, and only for a trusted click. So does the user's own input in the page
//! ([`Signal::Input`]). While the user has the tab, the marker offers "Zurückgeben" instead of
//! "Eingreifen" ([`update`]); a new document's marker says [`Signal::Hello`] and gets the state
//! then. When the tab goes back to the user, the marker leaves the current document and, with
//! holzi's session of the tab, every later one.

use serde_json::{json, Value};

use super::cdp::{page, runtime, Connection};
use super::error::Result;
use crate::window_texts::text;

/// The binding the marker calls, in holzi's world only.
pub const BINDING: &str = "__holzi";

/// What the marker reports through [`BINDING`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Signal {
    /// "Stoppen".
    Stop,
    /// "Eingreifen".
    Takeover,
    /// "Zurückgeben".
    Return,
    /// A trusted press of a mouse button or key in the page while the agent steers.
    Input,
    /// A new document's marker is there and wants the session's state.
    Hello,
}

impl Signal {
    /// The signal of a `Runtime.bindingCalled` event, when it is the marker's.
    pub fn of(event: &Value) -> Option<Self> {
        if event["name"] != BINDING {
            return None;
        }
        match event["payload"].as_str()? {
            "stop" => Some(Self::Stop),
            "takeover" => Some(Self::Takeover),
            "return" => Some(Self::Return),
            "input" => Some(Self::Input),
            "hello" => Some(Self::Hello),
            _ => None,
        }
    }
}

const SCRIPT: &str = include_str!("marker.js");

/// The marker's texts in the language of holzi's window.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MarkerTexts {
    pub text: String,
    /// While the user has the tab.
    pub paused_text: String,
    pub take_over: String,
    pub give_back: String,
    pub stop: String,
    /// Put before the tab's title.
    pub prefix: String,
}

impl MarkerTexts {
    /// The texts for `agent` (its own claim), or the generic label for an unnamed external agent.
    pub fn new(locale: &str, builtin: bool, agent: &str) -> Self {
        let agent = if builtin {
            text(locale, "agentBrowser.prompt.builtinAgent", &[])
        } else {
            let agent = agent.trim();
            if agent.is_empty() {
                text(locale, "agentBrowser.prompt.agent", &[])
            } else {
                agent.to_owned()
            }
        };
        Self {
            text: text(locale, "agentBrowser.marker.text", &[("agent", &agent)]),
            paused_text: text(locale, "agentBrowser.marker.paused", &[("agent", &agent)]),
            take_over: text(locale, "agentBrowser.marker.takeOver", &[]),
            give_back: text(locale, "agentBrowser.marker.giveBack", &[]),
            stop: text(locale, "agentBrowser.marker.stop", &[]),
            prefix: text(locale, "agentBrowser.marker.titlePrefix", &[]),
        }
    }

    /// The script that adds the marker to a document, or updates the one there; `paused` while
    /// the user has the tab.
    pub fn script(&self, paused: bool) -> String {
        let config = json!({
            "text": self.text,
            "pausedText": self.paused_text,
            "takeOver": self.take_over,
            "giveBack": self.give_back,
            "stop": self.stop,
            "prefix": self.prefix,
            "paused": paused,
        });
        format!("({SCRIPT})({config})")
    }
}

/// Adds the marker to every new document of the tab in `session`, the current one included.
pub async fn install(connection: &Connection, session: &str, texts: &MarkerTexts) -> Result<()> {
    runtime::enable(connection, session).await?;
    runtime::add_binding(connection, session, BINDING, page::WORLD).await?;
    page::add_script(connection, session, &texts.script(false)).await
}

/// Shows the marker as `paused` or not, in the document of holzi's world `context`, or in the
/// tab's current document.
pub async fn update(
    connection: &Connection,
    session: &str,
    context: Option<i64>,
    texts: &MarkerTexts,
    paused: bool,
) -> Result<()> {
    let context = match context {
        Some(context) => context,
        None => {
            let (frame, _) = page::main_frame(connection, session).await?;
            page::isolated_world(connection, session, &frame).await?
        }
    };
    runtime::evaluate(connection, session, context, &texts.script(paused))
        .await
        .map(drop)
}

/// Removes the marker from the tab's current document.
pub async fn remove(connection: &Connection, session: &str) -> Result<()> {
    let (frame, _) = page::main_frame(connection, session).await?;
    let context = page::isolated_world(connection, session, &frame).await?;
    runtime::evaluate(
        connection,
        session,
        context,
        "globalThis.__holziMarker && globalThis.__holziMarker.remove()",
    )
    .await
    .map(drop)
}

#[cfg(test)]
#[path = "marker_tests.rs"]
mod tests;
