//! The `Target` domain: attaching to the targets of the agent browser in flat mode
//! (contracts/agent-browser.md "Start" and "Je Agenten-Tab").

use std::sync::Arc;

use serde_json::{json, Value};

use super::{runtime, Connection};
use crate::agent_browser::error::Result;

/// Takes a new target that waits for holzi (the `params` of its `Target.attachedToTarget`):
/// `true` when the taker lets it go on itself once it is set up.
pub type Claim = Arc<dyn Fn(&Arc<Connection>, &Value) -> bool + Send + Sync>;

/// `Target.setAutoAttach` of the browser in flat mode: every target gets a session whose events
/// carry its `sessionId`, and each new one waits until holzi lets it go on. A page could otherwise
/// open a tab that loads before anything checks it (research R9); [`resume_unclaimed`] lets every
/// target go on that `claim` does not take. Call that first.
pub async fn auto_attach(connection: &Connection) -> Result<()> {
    connection
        .call(
            "Target.setAutoAttach",
            json!({ "autoAttach": true, "waitForDebuggerOnStart": true, "flatten": true }),
            None,
        )
        .await
        .map(drop)
}

/// Lets each new target of the browser go on at once unless `claim` takes it.
pub fn resume_unclaimed(connection: &Arc<Connection>, claim: Claim) {
    let mut events = connection.subscribe(None);
    let connection = Arc::downgrade(connection);
    tokio::spawn(async move {
        while let Some(event) = events.recv().await {
            if event.method != "Target.attachedToTarget"
                || event.params["waitingForDebugger"] != true
            {
                continue;
            }
            let Some(connection) = connection.upgrade() else {
                return;
            };
            if claim(&connection, &event.params) {
                continue;
            }
            let Some(session) = event.params["sessionId"].as_str().map(str::to_owned) else {
                continue;
            };
            tokio::spawn(async move {
                if let Err(error) = runtime::run_if_waiting(&connection, &session).await {
                    log::debug!("agent browser: a new target did not go on: {error}");
                }
            });
        }
    });
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
