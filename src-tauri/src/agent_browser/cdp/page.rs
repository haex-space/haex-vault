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

/// `Page.navigate`; reports whether a new document loader was created. An error text of the
/// browser (`net::ERR_BLOCKED_BY_CLIENT`) is an error.
pub async fn navigate(connection: &Connection, session: &str, url: &str) -> Result<bool> {
    let reply = connection
        .call("Page.navigate", json!({ "url": url }), Some(session))
        .await?;
    match reply.get("errorText").and_then(Value::as_str) {
        Some(error) if !error.is_empty() => Err(BrowserError::Protocol(error.to_owned())),
        _ => Ok(reply.get("loaderId").and_then(Value::as_str).is_some()),
    }
}

/// `Page.stopLoading`: a pending navigation ends where it is.
pub async fn stop_loading(connection: &Connection, session: &str) -> Result<()> {
    connection
        .call("Page.stopLoading", json!({}), Some(session))
        .await
        .map(drop)
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

/// Every frame of the page in `session` with the address of its document, the main frame first,
/// then the frames inside it in document order. A frame of another process is listed too, but runs
/// nothing through this session.
pub async fn frames(connection: &Connection, session: &str) -> Result<Vec<(String, String)>> {
    let tree = connection
        .call("Page.getFrameTree", json!({}), Some(session))
        .await?;
    let mut frames = Vec::new();
    let mut open = vec![&tree["frameTree"]];
    while let Some(node) = open.pop() {
        if let (Some(id), Some(url)) = (node["frame"]["id"].as_str(), node["frame"]["url"].as_str())
        {
            frames.push((id.to_owned(), url.to_owned()));
        }
        if let Some(children) = node["childFrames"].as_array() {
            open.extend(children.iter().rev());
        }
    }
    if frames.is_empty() {
        return Err(BrowserError::Protocol("no main frame".to_owned()));
    }
    Ok(frames)
}

/// One frame of a page's frame tree.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FrameNode {
    pub id: String,
    pub url: String,
    /// Index of the frame that holds it in the list [`frame_nodes`] returns; `None` for the root.
    pub parent: Option<usize>,
}

/// The frame tree of the page in `session` in document order, the root first, each frame with the
/// index of the one that holds it. A frame of another process is listed too (as a stub), but runs
/// nothing through this session.
pub async fn frame_nodes(connection: &Connection, session: &str) -> Result<Vec<FrameNode>> {
    let tree = connection
        .call("Page.getFrameTree", json!({}), Some(session))
        .await?;
    let mut nodes: Vec<FrameNode> = Vec::new();
    // (node, index of its parent) still to visit; children are pushed in reverse to keep the order.
    let mut open = vec![(&tree["frameTree"], None)];
    while let Some((node, parent)) = open.pop() {
        let (Some(id), Some(url)) = (node["frame"]["id"].as_str(), node["frame"]["url"].as_str())
        else {
            continue;
        };
        let index = nodes.len();
        nodes.push(FrameNode {
            id: id.to_owned(),
            url: url.to_owned(),
            parent,
        });
        if let Some(children) = node["childFrames"].as_array() {
            open.extend(children.iter().rev().map(|child| (child, Some(index))));
        }
    }
    if nodes.is_empty() {
        return Err(BrowserError::Protocol("no main frame".to_owned()));
    }
    Ok(nodes)
}

/// Whether the document in `session` holds an element that owns frame `frame`: for a frame of
/// another process, which is no part of the page's frame tree, this tells which document holds it.
pub async fn holds_frame(connection: &Connection, session: &str, frame: &str) -> bool {
    if connection
        .call("DOM.enable", json!({}), Some(session))
        .await
        .is_err()
    {
        return false;
    }
    connection
        .call(
            "DOM.getFrameOwner",
            json!({ "frameId": frame }),
            Some(session),
        )
        .await
        .is_ok()
}

/// Where the content of a frame lies in the viewport that holds it: the corner of its top-left, and
/// the step in that viewport for one pixel across and one down inside the frame. A frame that is
/// scaled, turned or sheared by its element's `transform` has steps other than (1, 0) and (0, 1),
/// so a point inside it is moved by the whole quad of the element and not by its corner alone.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Placement {
    pub x: f64,
    pub y: f64,
    pub across: (f64, f64),
    pub down: (f64, f64),
}

impl Placement {
    /// A frame that only lies at (`x`, `y`), unscaled.
    pub fn offset(x: f64, y: f64) -> Self {
        Self {
            x,
            y,
            across: (1.0, 0.0),
            down: (0.0, 1.0),
        }
    }

    /// Where the point (`x`, `y`) of the frame lies in the holding viewport.
    pub fn apply(&self, x: f64, y: f64) -> (f64, f64) {
        (
            self.x + x * self.across.0 + y * self.down.0,
            self.y + x * self.across.1 + y * self.down.1,
        )
    }
}

/// The [`Placement`] of the content of an element from its `DOM.getBoxModel` model: the corner is
/// the first point of the content quad. The border quad against the element's own width and height
/// tells how the browser drew the element (its transform), which the content takes along.
pub fn placement(model: &Value) -> Option<Placement> {
    let point = |quad: &Value, index: usize| -> Option<(f64, f64)> {
        Some((
            quad.get(index * 2)?.as_f64()?,
            quad.get(index * 2 + 1)?.as_f64()?,
        ))
    };
    let (x, y) = point(&model["content"], 0)?;
    let mut placement = Placement::offset(x, y);
    let border = &model["border"];
    if let (Some(from), Some(right), Some(left), Some(width), Some(height)) = (
        point(border, 0),
        point(border, 1),
        point(border, 3),
        model["width"].as_f64(),
        model["height"].as_f64(),
    ) {
        if width > 0.0 && height > 0.0 {
            placement.across = ((right.0 - from.0) / width, (right.1 - from.1) / width);
            placement.down = ((left.0 - from.0) / height, (left.1 - from.1) / height);
        }
    }
    Some(placement)
}

/// How the content of the element that holds frame `frame` lies in the viewport of the root frame
/// of the process of `session` (the browser reports a box that way, whatever frames lie between).
/// Applying these one after the other across the processes from a frame to the tab gives where a
/// point of the frame is on the page.
pub async fn owner_placement(
    connection: &Connection,
    session: &str,
    frame: &str,
) -> Result<Placement> {
    connection
        .call("DOM.enable", json!({}), Some(session))
        .await?;
    let owner = connection
        .call(
            "DOM.getFrameOwner",
            json!({ "frameId": frame }),
            Some(session),
        )
        .await?;
    let node = owner["backendNodeId"].clone();
    let model = connection
        .call(
            "DOM.getBoxModel",
            json!({ "backendNodeId": node }),
            Some(session),
        )
        .await?;
    placement(&model["model"])
        .ok_or_else(|| BrowserError::Protocol("no box for a frame".to_owned()))
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

/// Runs `source` in the world [`WORLD`] of every new document of the tab, and of the current one.
pub async fn add_script(connection: &Connection, session: &str, source: &str) -> Result<()> {
    connection
        .call(
            "Page.addScriptToEvaluateOnNewDocument",
            json!({ "source": source, "worldName": WORLD, "runImmediately": true }),
            Some(session),
        )
        .await
        .map(drop)
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

/// The layout of the visible part of the tab: where the page is scrolled to, how large the view
/// is and the zoom, all in CSS pixels.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Layout {
    pub scroll_x: f64,
    pub scroll_y: f64,
    pub width: f64,
    pub height: f64,
    pub zoom: f64,
}

/// The layout of the tab in `session` (`Page.getLayoutMetrics`).
pub async fn layout(connection: &Connection, session: &str) -> Result<Layout> {
    let metrics = connection
        .call("Page.getLayoutMetrics", json!({}), Some(session))
        .await?;
    let viewport = &metrics["cssVisualViewport"];
    match (
        viewport["clientWidth"].as_f64(),
        viewport["clientHeight"].as_f64(),
    ) {
        (Some(width), Some(height)) if width > 0.0 && height > 0.0 => Ok(Layout {
            scroll_x: viewport["pageX"].as_f64().unwrap_or(0.0),
            scroll_y: viewport["pageY"].as_f64().unwrap_or(0.0),
            width,
            height,
            zoom: viewport["zoom"].as_f64().unwrap_or(1.0),
        }),
        _ => Err(BrowserError::Protocol("no view".to_owned())),
    }
}

/// A part of the page to capture, in CSS pixels of the page, with the scale of the image (image
/// pixels per CSS pixel).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Clip {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
    pub scale: f64,
}

/// `clip` of the tab in `session` as PNG, so the marks can be drawn on it without a second loss.
pub async fn capture(connection: &Connection, session: &str, clip: Clip) -> Result<Vec<u8>> {
    let reply = connection
        .call(
            "Page.captureScreenshot",
            json!({
                "format": "png",
                "clip": {
                    "x": clip.x,
                    "y": clip.y,
                    "width": clip.width,
                    "height": clip.height,
                    "scale": clip.scale,
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
