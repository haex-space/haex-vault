//! The view of a tab a screenshot showed (spec 065 FR-058, research R21). Positions in tools are in
//! image pixels of a full screenshot, so they mean something only while the page still looks as it
//! did: the tab remembers the document, the scroll position, the size of the view and the zoom,
//! and a call with a position reads them again and compares. A difference is `view_changed`, none
//! remembered is `no_view`, and nothing reaches the page then. A layout that moves under the same
//! scroll position is not seen here; the element really hit is checked by the tool (R22).

use serde_json::{json, Value};

use super::cdp::page::{self, Layout};
use super::cdp::Connection;
use super::error::Result;

/// The most CSS pixels of width a screenshot has as image pixels (the scale of the full image).
pub const IMAGE_WIDTH: f64 = 1280.0;

/// What a screenshot showed of a tab.
#[derive(Debug, Clone, PartialEq)]
pub struct View {
    /// The current document of the main frame; it changes with every navigation to a new document.
    pub loader: String,
    pub scroll_x: i64,
    pub scroll_y: i64,
    pub width: i64,
    pub height: i64,
    /// The zoom of the page, in hundredths.
    pub zoom: i64,
}

/// Why a position cannot be used.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ViewError {
    /// No screenshot of this tab was taken, or the last one was refused.
    NoView,
    /// The page looks different from the screenshot.
    Changed,
}

impl View {
    /// The view of `layout` in the document `loader`.
    pub fn of(loader: &str, layout: Layout) -> Self {
        Self {
            loader: loader.to_owned(),
            scroll_x: layout.scroll_x.round() as i64,
            scroll_y: layout.scroll_y.round() as i64,
            width: layout.width.round() as i64,
            height: layout.height.round() as i64,
            zoom: (layout.zoom * 100.0).round() as i64,
        }
    }

    /// Image pixels per CSS pixel of a full screenshot of this view.
    pub fn scale(&self) -> f64 {
        (IMAGE_WIDTH / self.width as f64).min(1.0)
    }

    /// The point of the view an image pixel of a full screenshot stands for.
    pub fn css(&self, x: f64, y: f64) -> (f64, f64) {
        let scale = self.scale();
        (x / scale, y / scale)
    }

    /// The size of a full screenshot of this view, in image pixels.
    pub fn image(&self) -> (u32, u32) {
        let scale = self.scale();
        (
            (self.width as f64 * scale).round() as u32,
            (self.height as f64 * scale).round() as u32,
        )
    }

    /// How the view is told to an agent.
    pub fn told(&self) -> Value {
        let (width, height) = self.image();
        json!({
            "width": width,
            "height": height,
            "scale": (self.scale() * 1000.0).round() / 1000.0,
            "scroll": { "x": self.scroll_x, "y": self.scroll_y },
        })
    }
}

/// Whether a position may be used: `shown` is what the last screenshot showed, `now` how the tab
/// looks.
pub fn check(shown: Option<&View>, now: &View) -> std::result::Result<(), ViewError> {
    match shown {
        None => Err(ViewError::NoView),
        Some(shown) if shown == now => Ok(()),
        Some(_) => Err(ViewError::Changed),
    }
}

/// How the tab in `session` looks now.
pub async fn current(connection: &Connection, session: &str) -> Result<View> {
    let layout = page::layout(connection, session).await?;
    let (_, loader) = page::main_frame(connection, session).await?;
    Ok(View::of(&loader, layout))
}

#[cfg(test)]
#[path = "view_tests.rs"]
mod tests;
