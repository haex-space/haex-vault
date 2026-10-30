//! The `Runtime` domain: running holzi's script in its own world (research R4).

use serde_json::{json, Value};

use super::Connection;
use crate::agent_browser::error::{BrowserError, Result};

/// Evaluates `expression` in the context `context` and returns its value; an exception of the
/// script is an error.
pub async fn evaluate(
    connection: &Connection,
    session: &str,
    context: i64,
    expression: &str,
) -> Result<Value> {
    let reply = connection
        .call(
            "Runtime.evaluate",
            json!({
                "expression": expression,
                "contextId": context,
                "returnByValue": true,
                "awaitPromise": true,
                "userGesture": false,
            }),
            Some(session),
        )
        .await?;
    if let Some(details) = reply.get("exceptionDetails") {
        let text = details["exception"]["description"]
            .as_str()
            .or_else(|| details["text"].as_str())
            .unwrap_or("exception");
        return Err(BrowserError::Protocol(format!("page script: {text}")));
    }
    Ok(reply["result"]["value"].clone())
}

/// Lets a target that waits for the debugger (a new frame) run.
pub async fn run_if_waiting(connection: &Connection, session: &str) -> Result<()> {
    connection
        .call("Runtime.runIfWaitingForDebugger", json!({}), Some(session))
        .await
        .map(drop)
}

/// `Runtime.enable`: without it, a binding reaches no context created later.
pub async fn enable(connection: &Connection, session: &str) -> Result<()> {
    connection
        .call("Runtime.enable", json!({}), Some(session))
        .await
        .map(drop)
}

/// Adds the binding `name` to every context of the world `world`, now and in later documents; a
/// call arrives as `Runtime.bindingCalled`. Needs [`enable`].
pub async fn add_binding(
    connection: &Connection,
    session: &str,
    name: &str,
    world: &str,
) -> Result<()> {
    connection
        .call(
            "Runtime.addBinding",
            json!({ "name": name, "executionContextName": world }),
            Some(session),
        )
        .await
        .map(drop)
}
