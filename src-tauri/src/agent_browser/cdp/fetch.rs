//! The `Fetch` domain: holding a tab's requests until holzi has decided about them: documents for
//! the website check (research R9), and every request that may change something on a website
//! (research R5 step 2). While the tracker filters are on, every request of every type is held,
//! so the filters see images, styles, scripts, fonts and media as well (research R16); those are
//! let go at once unless a filter blocks them.

use serde_json::json;

use super::Connection;
use crate::agent_browser::error::Result;

/// The kinds of request a page can change something with, or that load a document.
const HELD: [&str; 6] = ["Document", "XHR", "Fetch", "Ping", "EventSource", "Other"];

/// Whether requests of `kind` are held for holzi's checks, not only for the tracker filters.
pub fn checked(kind: &str) -> bool {
    HELD.contains(&kind)
}

/// Pauses the requests of `session` before they are sent: those of the kinds in [`HELD`], or with
/// `all` every request, so the tracker filters can look at each.
pub async fn hold_requests(connection: &Connection, session: &str, all: bool) -> Result<()> {
    let patterns: Vec<_> = if all {
        vec![json!({ "requestStage": "Request" })]
    } else {
        HELD.iter()
            .map(|kind| json!({ "resourceType": kind, "requestStage": "Request" }))
            .collect()
    };
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
