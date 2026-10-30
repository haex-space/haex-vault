//! The marker of an agent tab (spec 065, FR-014, research R6): holzi's own world adds it to every
//! document of the tab (marker.js); its "Stoppen" calls the binding [`BINDING`], which exists in
//! that world only, and only for a trusted click. When the tab goes back to the user, the marker
//! leaves the current document and, with holzi's session of the tab, every later one.

use serde_json::json;

use super::cdp::{page, runtime, Connection};
use super::error::Result;
use crate::window_texts::text;

/// The binding the marker's buttons call, in holzi's world only.
pub const BINDING: &str = "__holzi";

/// What the marker's stop button sends through [`BINDING`].
pub const STOP: &str = "stop";

const SCRIPT: &str = include_str!("marker.js");

/// The marker's texts in the language of holzi's window.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MarkerTexts {
    pub text: String,
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
            stop: text(locale, "agentBrowser.marker.stop", &[]),
            prefix: text(locale, "agentBrowser.marker.titlePrefix", &[]),
        }
    }

    /// The script that adds the marker to a document.
    pub fn script(&self) -> String {
        let config = json!({ "text": self.text, "stop": self.stop, "prefix": self.prefix });
        format!("({SCRIPT})({config})")
    }
}

/// Adds the marker to every new document of the tab in `session`, the current one included.
pub async fn install(connection: &Connection, session: &str, texts: &MarkerTexts) -> Result<()> {
    runtime::enable(connection, session).await?;
    runtime::add_binding(connection, session, BINDING, page::WORLD).await?;
    page::add_script(connection, session, &texts.script()).await
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
