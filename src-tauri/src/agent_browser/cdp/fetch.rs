//! The `Fetch` domain: holding document requests until holzi has checked their website
//! (research R9).

use serde_json::json;

use super::Connection;
use crate::agent_browser::error::Result;

/// Pauses every document request of `session` before it is sent.
pub async fn hold_documents(connection: &Connection, session: &str) -> Result<()> {
    connection
        .call(
            "Fetch.enable",
            json!({ "patterns": [{ "resourceType": "Document", "requestStage": "Request" }] }),
            Some(session),
        )
        .await
        .map(drop)
}

/// Lets the paused request `request` go on.
pub async fn go_on(connection: &Connection, session: &str, request: &str) -> Result<()> {
    connection
        .call(
            "Fetch.continueRequest",
            json!({ "requestId": request }),
            Some(session),
        )
        .await
        .map(drop)
}

/// Ends the paused request `request` as blocked; the page shows the browser's error.
pub async fn block(connection: &Connection, session: &str, request: &str) -> Result<()> {
    connection
        .call(
            "Fetch.failRequest",
            json!({ "requestId": request, "errorReason": "BlockedByClient" }),
            Some(session),
        )
        .await
        .map(drop)
}
