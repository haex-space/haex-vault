//! The `Fetch` domain: holding a tab's requests until holzi has decided about them: documents for
//! the website check (research R9), and every request that may change something on a website
//! (research R5 step 2). Images, styles, scripts, fonts and media are never held; they are only
//! read.

use serde_json::json;

use super::Connection;
use crate::agent_browser::error::Result;

/// The kinds of request a page can change something with, or that load a document.
const HELD: [&str; 6] = ["Document", "XHR", "Fetch", "Ping", "EventSource", "Other"];

/// Pauses every request of `session` of the kinds in [`HELD`] before it is sent.
pub async fn hold_requests(connection: &Connection, session: &str) -> Result<()> {
    let patterns: Vec<_> = HELD
        .iter()
        .map(|kind| json!({ "resourceType": kind, "requestStage": "Request" }))
        .collect();
    connection
        .call(
            "Fetch.enable",
            json!({ "patterns": patterns }),
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
