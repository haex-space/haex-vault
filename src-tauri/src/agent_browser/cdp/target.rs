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

/// A new page target on `about:blank`; its `targetId`.
pub async fn create(connection: &Connection) -> Result<String> {
    let reply = connection
        .call("Target.createTarget", json!({ "url": "about:blank" }), None)
        .await?;
    reply["targetId"]
        .as_str()
        .map(str::to_owned)
        .ok_or_else(|| {
            crate::agent_browser::error::BrowserError::Protocol("no target id".to_owned())
        })
}

/// Attaches to `target` in flat mode; the session id.
pub async fn attach(connection: &Connection, target: &str) -> Result<String> {
    let reply = connection
        .call(
            "Target.attachToTarget",
            json!({ "targetId": target, "flatten": true }),
            None,
        )
        .await?;
    reply["sessionId"]
        .as_str()
        .map(str::to_owned)
        .ok_or_else(|| {
            crate::agent_browser::error::BrowserError::Protocol("no session id".to_owned())
        })
}

/// Closes the page target `target`.
pub async fn close(connection: &Connection, target: &str) -> Result<()> {
    connection
        .call("Target.closeTarget", json!({ "targetId": target }), None)
        .await
        .map(drop)
}

/// Ends holzi's session `session` of a target; the target stays open, and what the session set
/// up (scripts, bindings, held requests) ends with it.
pub async fn detach(connection: &Connection, session: &str) -> Result<()> {
    connection
        .call(
            "Target.detachFromTarget",
            json!({ "sessionId": session }),
            None,
        )
        .await
        .map(drop)
}

/// Attaches the frames of the page in `session` as they appear, each waiting until holzi has set
/// it up (research R9: their documents are checked too).
pub async fn attach_frames(connection: &Connection, session: &str) -> Result<()> {
    connection
        .call(
            "Target.setAutoAttach",
            json!({ "autoAttach": true, "waitForDebuggerOnStart": true, "flatten": true }),
            Some(session),
        )
        .await
        .map(drop)
}
