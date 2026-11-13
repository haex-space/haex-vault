//! The `Input` domain: real input events for clicking and typing (research R4). The page sees
//! trusted events, the same a person makes, and runs its own checks on them.

use serde_json::json;

use super::Connection;
use crate::agent_browser::error::Result;
use crate::agent_browser::keys::KeyPress;

/// A left click at `(x, y)` in CSS pixels of the visible part of the tab.
pub async fn click(connection: &Connection, session: &str, x: f64, y: f64) -> Result<()> {
    click_with(connection, session, x, y, Button::Left, 1).await
}

/// The mouse button of a click.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Button {
    Left,
    Right,
}

impl Button {
    fn name(self) -> &'static str {
        match self {
            Self::Left => "left",
            Self::Right => "right",
        }
    }

    /// The bit of `buttons` while it is held.
    fn bit(self) -> u8 {
        match self {
            Self::Left => 1,
            Self::Right => 2,
        }
    }
}

/// A click with `button` at `(x, y)`: `count` pairs of press and release, the nth with
/// `clickCount` n, so the page sees a double click as one. A right click makes the page fire
/// `contextmenu` (research R22).
pub async fn click_with(
    connection: &Connection,
    session: &str,
    x: f64,
    y: f64,
    button: Button,
    count: u8,
) -> Result<()> {
    connection
        .call(
            "Input.dispatchMouseEvent",
            json!({ "type": "mouseMoved", "x": x, "y": y, "buttons": 0 }),
            Some(session),
        )
        .await?;
    for click_count in 1..=count.max(1) {
        for (kind, buttons) in [("mousePressed", button.bit()), ("mouseReleased", 0)] {
            connection
                .call(
                    "Input.dispatchMouseEvent",
                    json!({
                        "type": kind, "x": x, "y": y, "button": button.name(),
                        "buttons": buttons, "clickCount": click_count,
                    }),
                    Some(session),
                )
                .await?;
        }
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

/// Moves the pointer to `(x, y)` in CSS pixels of the visible part of the tab, as a person's hand
/// does; a menu that opens on hover sees it.
pub async fn move_to(connection: &Connection, session: &str, x: f64, y: f64) -> Result<()> {
    connection
        .call(
            "Input.dispatchMouseEvent",
            json!({ "type": "mouseMoved", "x": x, "y": y, "buttons": 0 }),
            Some(session),
        )
        .await
        .map(drop)
}

/// A turn of the mouse wheel over `(x, y)`: `delta_y` positive scrolls down, `delta_x` positive
/// scrolls right (CSS pixels). The page and its scrollers see a real `wheel` event.
pub async fn wheel(
    connection: &Connection,
    session: &str,
    x: f64,
    y: f64,
    delta_x: f64,
    delta_y: f64,
) -> Result<()> {
    connection
        .call(
            "Input.dispatchMouseEvent",
            json!({
                "type": "mouseWheel", "x": x, "y": y, "deltaX": delta_x, "deltaY": delta_y,
                "buttons": 0,
            }),
            Some(session),
        )
        .await
        .map(drop)
}

/// Presses one key of [`crate::agent_browser::keys`]: down, then up. A key that types a character
/// is a `keyDown` with its text; one that types nothing is a `rawKeyDown`, as a browser sends it.
pub async fn press(connection: &Connection, session: &str, key: &KeyPress) -> Result<()> {
    for kind in ["down", "up"] {
        let mut event = json!({
            "type": match (kind, &key.text) {
                ("up", _) => "keyUp",
                (_, Some(_)) => "keyDown",
                _ => "rawKeyDown",
            },
            "modifiers": key.modifiers,
            "key": key.key,
            "code": key.code,
            "windowsVirtualKeyCode": key.virtual_key,
            "nativeVirtualKeyCode": key.virtual_key,
        });
        if kind == "down" {
            if let Some(text) = &key.text {
                event["text"] = json!(text);
                event["unmodifiedText"] = json!(text.to_lowercase());
            }
            if !key.commands.is_empty() {
                event["commands"] = json!(key.commands);
            }
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
