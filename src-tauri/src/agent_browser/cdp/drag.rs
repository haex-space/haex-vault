//! Dragging with the mouse (spec 065 FR-060, research R22): press at one point, move in small
//! steps, release at another, within [`BUDGET`]. A page that makes an element draggable would hand
//! the drag to the operating system; with `Input.setInterceptDrags` the browser reports it as
//! `Input.dragIntercepted` instead, and holzi finishes it with `Input.dispatchDragEvent`. The
//! release is always sent, and interception always ends, whatever went wrong on the way: no
//! button stays held.

use std::sync::Arc;
use std::time::Duration;

use serde_json::{json, Value};
use tokio::sync::mpsc::UnboundedReceiver;

use super::{Connection, Event};
use crate::agent_browser::error::{BrowserError, Result};

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

/// How long the release and the end of interception are given, together. A browser that does not
/// answer them is closed: it quits and the session starts a fresh one, so no button and no
/// interception outlive it.
const CLEANUP: Duration = Duration::from_secs(1);

/// Drags from `from` to `to` (CSS pixels of the view). `events` are the tab's events from before
/// the call, so an interception is not missed. The button is released in every case, also when the
/// caller stops waiting: the drag runs in a task of its own, from the press to the release.
pub async fn drag(
    connection: Arc<Connection>,
    session: String,
    events: UnboundedReceiver<Event>,
    from: (f64, f64),
    to: (f64, f64),
) -> Result<Dragged> {
    tokio::spawn(async move {
        let mut events = events;
        run(
            &connection,
            &session,
            &mut events,
            from,
            to,
            BUDGET,
            CLEANUP,
        )
        .await
    })
    .await
    .map_err(|error| BrowserError::Unavailable(format!("the drag ended unexpectedly: {error}")))?
}

/// [`drag`] with its time limits given.
async fn run(
    connection: &Connection,
    session: &str,
    events: &mut UnboundedReceiver<Event>,
    from: (f64, f64),
    to: (f64, f64),
    budget: Duration,
    cleanup: Duration,
) -> Result<Dragged> {
    let on = tokio::time::timeout(
        cleanup,
        connection.call(
            "Input.setInterceptDrags",
            json!({ "enabled": true }),
            Some(session),
        ),
    )
    .await;
    match on {
        Ok(on) => on.map(drop)?,
        Err(_) => {
            connection.close();
            return Err(BrowserError::Unavailable(
                "the browser did not start the drag".to_owned(),
            ));
        }
    }
    // Where the pointer was last sent to: a drag that ends early is released there, not at a
    // point the pointer never reached.
    let mut pointer = from;
    let pressed = tokio::time::timeout(
        budget,
        press_and_move(connection, session, events, from, to, &mut pointer),
    )
    .await;
    // The release goes out whatever the way there did.
    let ended = tokio::time::timeout(cleanup, async {
        let released = mouse(
            connection,
            session,
            json!({
                "type": "mouseReleased", "x": pointer.0, "y": pointer.1, "button": "left",
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
        released.and(off)
    })
    .await;
    match ended {
        Ok(ended) => ended?,
        Err(_) => {
            // No answer: the button may still be held. Ending the browser is the one sure way.
            connection.close();
            return Err(BrowserError::Unavailable(
                "the browser did not end the drag".to_owned(),
            ));
        }
    }
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
    pointer: &mut (f64, f64),
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
        *pointer = (x, y);
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
    use std::sync::mpsc;

    use super::super::mod_tests::{connected, FakeBrowser};
    use super::*;

    #[test]
    fn the_path_has_enough_steps_and_ends_on_the_target() {
        let steps = path((10.0, 20.0), (110.0, 70.0));
        assert!(steps.len() >= MIN_STEPS);
        assert_eq!(steps.last(), Some(&(110.0, 70.0)));
        assert!(steps.windows(2).all(|pair| pair[0].0 < pair[1].0));
    }

    /// Runs a fake browser: every command it gets is passed on; `answers` says whether it replies.
    fn fake(
        mut browser: FakeBrowser,
        mut answers: impl FnMut(&Value) -> bool + Send + 'static,
    ) -> mpsc::Receiver<Value> {
        let (seen, commands) = mpsc::channel();
        std::thread::spawn(move || {
            while let Some(command) = browser.try_command() {
                let answer = answers(&command);
                let id = command["id"].clone();
                let _ = seen.send(command);
                if answer {
                    browser.send(json!({ "id": id, "result": {} }));
                }
            }
        });
        commands
    }

    /// The commands up to and including the end of interception.
    fn until_off(commands: &mpsc::Receiver<Value>) -> Vec<Value> {
        let mut all = Vec::new();
        while let Ok(command) = commands.recv_timeout(Duration::from_secs(3)) {
            let off = command["method"] == "Input.setInterceptDrags"
                && command["params"]["enabled"] == false;
            all.push(command);
            if off {
                return all;
            }
        }
        panic!("interception was never ended: {all:?}");
    }

    fn release(commands: &[Value]) -> &Value {
        commands
            .iter()
            .find(|command| command["params"]["type"] == "mouseReleased")
            .expect("a release")
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn the_button_is_released_when_the_caller_stops_waiting() {
        let (connection, browser) = connected();
        let (pressed, wait_pressed) = mpsc::channel();
        let (go, gate) = mpsc::channel::<()>();
        let commands = fake(browser, move |command| {
            if command["params"]["type"] == "mousePressed" {
                pressed.send(()).unwrap();
                gate.recv().unwrap();
            }
            true
        });
        let events = connection.subscribe(Some("S1"));
        let caller = tokio::spawn(drag(
            Arc::new(connection),
            "S1".to_owned(),
            events,
            (10.0, 20.0),
            (110.0, 70.0),
        ));
        tokio::task::spawn_blocking(move || wait_pressed.recv().unwrap())
            .await
            .unwrap();
        // Stop: the action is dropped while the button is down.
        caller.abort();
        assert!(caller.await.unwrap_err().is_cancelled());
        go.send(()).unwrap();
        let sent = tokio::task::spawn_blocking(move || until_off(&commands))
            .await
            .unwrap();
        assert_eq!(release(&sent)["params"]["buttons"], 0);
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn a_drag_that_is_too_slow_is_released_where_the_pointer_was_sent() {
        let (connection, browser) = connected();
        let mut moves = 0;
        let commands = fake(browser, move |command| {
            if command["params"]["type"] == "mouseMoved" && command["params"]["buttons"] == 1 {
                moves += 1;
                // The fourth move of the drag is never answered.
                return moves != 4;
            }
            true
        });
        let mut events = connection.subscribe(Some("S1"));
        let (from, to) = (10.0, 20.0);
        let ended = run(
            &connection,
            "S1",
            &mut events,
            (from, to),
            (110.0, 70.0),
            Duration::from_millis(300),
            Duration::from_secs(2),
        )
        .await;
        assert_eq!(ended, Ok(Dragged::TooSlow));
        let sent = tokio::task::spawn_blocking(move || until_off(&commands))
            .await
            .unwrap();
        let stuck = path((from, to), (110.0, 70.0))[3];
        assert_eq!(release(&sent)["params"]["x"], stuck.0);
        assert_eq!(release(&sent)["params"]["y"], stuck.1);
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn a_browser_that_does_not_answer_the_release_is_closed() {
        let (connection, browser) = connected();
        let _commands = fake(browser, |command| {
            command["params"]["type"] != "mouseReleased"
        });
        let mut events = connection.subscribe(Some("S1"));
        let ended = run(
            &connection,
            "S1",
            &mut events,
            (10.0, 20.0),
            (110.0, 70.0),
            Duration::from_secs(2),
            Duration::from_millis(200),
        )
        .await;
        assert!(
            matches!(ended, Err(BrowserError::Unavailable(_))),
            "{ended:?}"
        );
        for _ in 0..40 {
            if connection.is_closed() {
                return;
            }
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
        panic!("the connection was not closed");
    }
}
