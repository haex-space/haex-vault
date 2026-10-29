//! The `Page` domain: navigation, the isolated world `holzi` and screenshots (research R4, R12).

use base64::Engine;
use serde_json::{json, Value};

use super::Connection;
use crate::agent_browser::error::{BrowserError, Result};

/// The name of holzi's own world in every agent tab (research R4).
pub const WORLD: &str = "holzi";

pub async fn enable(connection: &Connection, session: &str) -> Result<()> {
    connection
        .call("Page.enable", json!({}), Some(session))
        .await
        .map(drop)
}

/// `Page.navigate`; an error text of the browser (`net::ERR_BLOCKED_BY_CLIENT`) is an error.
pub async fn navigate(connection: &Connection, session: &str, url: &str) -> Result<()> {
    let reply = connection
        .call("Page.navigate", json!({ "url": url }), Some(session))
        .await?;
    match reply.get("errorText").and_then(Value::as_str) {
        Some(error) if !error.is_empty() => Err(BrowserError::Protocol(error.to_owned())),
        _ => Ok(()),
    }
}

/// `Page.reload`.
pub async fn reload(connection: &Connection, session: &str) -> Result<()> {
    connection
        .call("Page.reload", json!({}), Some(session))
        .await
        .map(drop)
}

/// The id of the main frame and of its current document.
pub async fn main_frame(connection: &Connection, session: &str) -> Result<(String, String)> {
    let tree = connection
        .call("Page.getFrameTree", json!({}), Some(session))
        .await?;
    let frame = &tree["frameTree"]["frame"];
    match (frame["id"].as_str(), frame["loaderId"].as_str()) {
        (Some(id), Some(loader)) => Ok((id.to_owned(), loader.to_owned())),
        _ => Err(BrowserError::Protocol("no main frame".to_owned())),
    }
}

/// A fresh context of the world [`WORLD`] in `frame`.
pub async fn isolated_world(connection: &Connection, session: &str, frame: &str) -> Result<i64> {
    let reply = connection
        .call(
            "Page.createIsolatedWorld",
            json!({ "frameId": frame, "worldName": WORLD, "grantUniveralAccess": false }),
            Some(session),
        )
        .await?;
    reply["executionContextId"]
        .as_i64()
        .ok_or_else(|| BrowserError::Protocol("no execution context".to_owned()))
}

/// The history of the tab: the index of the current entry and the ids of all entries.
pub async fn history(connection: &Connection, session: &str) -> Result<(usize, Vec<i64>)> {
    let reply = connection
        .call("Page.getNavigationHistory", json!({}), Some(session))
        .await?;
    let current = reply["currentIndex"].as_u64().unwrap_or(0) as usize;
    let ids = reply["entries"]
        .as_array()
        .map(|entries| entries.iter().filter_map(|e| e["id"].as_i64()).collect())
        .unwrap_or_default();
    Ok((current, ids))
}

/// Goes to the history entry `id`.
pub async fn go_to_entry(connection: &Connection, session: &str, id: i64) -> Result<()> {
    connection
        .call(
            "Page.navigateToHistoryEntry",
            json!({ "entryId": id }),
            Some(session),
        )
        .await
        .map(drop)
}

/// The visible part of the tab as JPEG (quality 70), at most `max_width` CSS pixels wide.
pub async fn screenshot(connection: &Connection, session: &str, max_width: f64) -> Result<Vec<u8>> {
    let metrics = connection
        .call("Page.getLayoutMetrics", json!({}), Some(session))
        .await?;
    let viewport = &metrics["cssVisualViewport"];
    let width = viewport["clientWidth"].as_f64().unwrap_or(max_width);
    let height = viewport["clientHeight"].as_f64().unwrap_or(max_width);
    let scale = (max_width / width).min(1.0);
    let reply = connection
        .call(
            "Page.captureScreenshot",
            json!({
                "format": "jpeg",
                "quality": 70,
                "clip": {
                    "x": viewport["pageX"].as_f64().unwrap_or(0.0),
                    "y": viewport["pageY"].as_f64().unwrap_or(0.0),
                    "width": width,
                    "height": height,
                    "scale": scale,
                },
            }),
            Some(session),
        )
        .await?;
    let data = reply["data"]
        .as_str()
        .ok_or_else(|| BrowserError::Protocol("no image".to_owned()))?;
    base64::engine::general_purpose::STANDARD
        .decode(data)
        .map_err(|error| BrowserError::Protocol(error.to_string()))
}
