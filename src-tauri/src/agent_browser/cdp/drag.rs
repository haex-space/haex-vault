//! Dragging with the mouse (spec 065 FR-060, research R22): press at one point, move in small
//! steps, release at another, within [`BUDGET`]. A page that makes an element draggable would hand
//! the drag to the operating system; with `Input.setInterceptDrags` the browser reports it as
//! `Input.dragIntercepted` instead, and holzi finishes it with `Input.dispatchDragEvent`. The
//! release is always sent, and interception always ends, whatever went wrong on the way: no
//! button stays held.

use std::time::Duration;

use serde_json::{json, Value};
use tokio::sync::mpsc::UnboundedReceiver;

use super::{Connection, Event};
use crate::agent_browser::error::Result;

/// The most moves between press and release (at least [`MIN_STEPS`]).
const STEPS: usize = 12;
/// The fewest moves between press and release (FR-060).
pub const MIN_STEPS: usize = 8;
/// How long the whole drag may take.
pub const BUDGET: Duration = Duration::from_secs(3);
/// How long a drag that just ended is given to report that the browser took it over.
const INTERCEPT_WAIT: Duration = Duration::from_millis(150);

async fn mouse(connection: &Connection, session: &str, event: Value) -> Result<()> {
    connection
        .call("Input.dispatchMouseEvent", event, Some(session))
        .await
        .map(drop)
}

/// The data of a drag the browser took over, when `event` reports one.
fn intercepted(event: &Event) -> Option<Value> {
    (event.method == "Input.dragIntercepted").then(|| event.params["data"].clone())
}

/// The first interception among the events that arrived, without waiting.
fn taken(events: &mut UnboundedReceiver<Event>) -> Option<Value> {
    while let Ok(event) = events.try_recv() {
        if let Some(data) = intercepted(&event) {
            return Some(data);
        }
    }
    None
}

/// The points of the moves from `from` to `to`, the last one `to` itself.
pub fn path(from: (f64, f64), to: (f64, f64)) -> Vec<(f64, f64)> {
    (1..=STEPS)
        .map(|step| {
            let t = step as f64 / STEPS as f64;
            (from.0 + (to.0 - from.0) * t, from.1 + (to.1 - from.1) * t)
        })
        .collect()
}

/// How a drag ended.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Dragged {
    /// Pressed, moved and released.
    Done,
    /// It took longer than [`BUDGET`]; the button was released where the pointer was.
    TooSlow,
}

/// Drags from `from` to `to` (CSS pixels of the view). `events` are the tab's events from before
/// the call, so an interception is not missed. The button is released in every case.
pub async fn drag(
    connection: &Connection,
    session: &str,
    events: &mut UnboundedReceiver<Event>,
    from: (f64, f64),
    to: (f64, f64),
) -> Result<Dragged> {
    connection
        .call(
            "Input.setInterceptDrags",
            json!({ "enabled": true }),
            Some(session),
        )
        .await?;
    let pressed = tokio::time::timeout(
        BUDGET,
        press_and_move(connection, session, events, from, to),
    )
    .await;
    // The release goes out whatever the way there did.
    let released = mouse(
        connection,
        session,
        json!({
            "type": "mouseReleased", "x": to.0, "y": to.1, "button": "left",
            "buttons": 0, "clickCount": 1,
        }),
    )
    .await;
    let off = connection
        .call(
            "Input.setInterceptDrags",
            json!({ "enabled": false }),
            Some(session),
        )
        .await
        .map(drop);
    released?;
    off?;
    match pressed {
        Ok(result) => result.map(|()| Dragged::Done),
        Err(_) => Ok(Dragged::TooSlow),
    }
}

/// Everything up to the release: press, the moves, and the end of a drag the browser took over.
async fn press_and_move(
    connection: &Connection,
    session: &str,
    events: &mut UnboundedReceiver<Event>,
    from: (f64, f64),
    to: (f64, f64),
) -> Result<()> {
    mouse(
        connection,
        session,
        json!({ "type": "mouseMoved", "x": from.0, "y": from.1, "buttons": 0 }),
    )
    .await?;
    mouse(
        connection,
        session,
        json!({
            "type": "mousePressed", "x": from.0, "y": from.1, "button": "left",
            "buttons": 1, "clickCount": 1,
        }),
    )
    .await?;
    let mut data = None;
    for (x, y) in path(from, to) {
        mouse(
            connection,
            session,
            json!({ "type": "mouseMoved", "x": x, "y": y, "button": "left", "buttons": 1 }),
        )
        .await?;
        data = data.or_else(|| taken(events));
    }
    if data.is_none() {
        // The report of a drag the browser took over may come a moment after the last move.
        data = tokio::time::timeout(INTERCEPT_WAIT, async {
            while let Some(event) = events.recv().await {
                if let Some(data) = intercepted(&event) {
                    return Some(data);
                }
            }
            None
        })
        .await
        .ok()
        .flatten();
    }
    let Some(data) = data else {
        return Ok(());
    };
    // The page's own drag: enter, over, drop at the target.
    for kind in ["dragEnter", "dragOver", "drop"] {
        connection
            .call(
                "Input.dispatchDragEvent",
                json!({ "type": kind, "x": to.0, "y": to.1, "data": data }),
                Some(session),
            )
            .await?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_path_has_enough_steps_and_ends_on_the_target() {
        let steps = path((10.0, 20.0), (110.0, 70.0));
        assert!(steps.len() >= MIN_STEPS);
        assert_eq!(steps.last(), Some(&(110.0, 70.0)));
        assert!(steps.windows(2).all(|pair| pair[0].0 < pair[1].0));
    }
}
