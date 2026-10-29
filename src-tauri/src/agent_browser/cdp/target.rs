//! The `Target` domain: attaching to the targets of the agent browser in flat mode
//! (contracts/agent-browser.md "Start" and "Je Agenten-Tab").

use serde_json::json;

use super::Connection;
use crate::agent_browser::error::Result;

/// `Target.setAutoAttach` in flat mode: every new target, nested frames included, gets a session
/// whose events carry its `sessionId`.
pub async fn auto_attach(connection: &Connection, session: Option<&str>) -> Result<()> {
    connection
        .call(
            "Target.setAutoAttach",
            json!({ "autoAttach": true, "waitForDebuggerOnStart": false, "flatten": true }),
            session,
        )
        .await
        .map(drop)
}
