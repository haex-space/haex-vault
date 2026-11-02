//! The `Input` domain: real input events for clicking and typing (research R4). The page sees
//! trusted events, the same a person makes, and runs its own checks on them.

use serde_json::json;

use super::Connection;
use crate::agent_browser::error::Result;

/// A left click at `(x, y)` in CSS pixels of the visible part of the tab.
pub async fn click(connection: &Connection, session: &str, x: f64, y: f64) -> Result<()> {
    for (kind, buttons) in [("mouseMoved", 0), ("mousePressed", 1), ("mouseReleased", 0)] {
        connection
            .call(
                "Input.dispatchMouseEvent",
                json!({
                    "type": kind, "x": x, "y": y, "button": "left",
                    "buttons": buttons, "clickCount": 1,
                }),
                Some(session),
            )
            .await?;
    }
    Ok(())
}

/// Types `text` into the focused element, as an input method does.
pub async fn insert_text(connection: &Connection, session: &str, text: &str) -> Result<()> {
    connection
        .call("Input.insertText", json!({ "text": text }), Some(session))
        .await
        .map(drop)
}

/// Presses Enter in the focused element.
pub async fn enter(connection: &Connection, session: &str) -> Result<()> {
    for kind in ["keyDown", "keyUp"] {
        let mut event = json!({
            "type": kind, "key": "Enter", "code": "Enter",
            "windowsVirtualKeyCode": 13, "nativeVirtualKeyCode": 13,
        });
        if kind == "keyDown" {
            event["text"] = json!("\r");
        }
        connection
            .call("Input.dispatchKeyEvent", event, Some(session))
            .await?;
    }
    Ok(())
}

/// `DOM.setFileInputFiles`: puts the files at `paths` into the file field `object` (research R11);
/// the page sees them as if the user had chosen them.
pub async fn set_files(
    connection: &Connection,
    session: &str,
    object: &str,
    paths: &[String],
) -> Result<()> {
    connection
        .call(
            "DOM.setFileInputFiles",
            json!({ "files": paths, "objectId": object }),
            Some(session),
        )
        .await
        .map(drop)
}
