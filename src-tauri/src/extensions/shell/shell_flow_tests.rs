//! `shell:output` only as fast as the frames acknowledge it (spec 017, US11).

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, OnceLock};
use std::time::{Duration, Instant};

use serde_json::{json, Value};

use super::{of, output, setup, sh, Recorded, Setup, EXIT, OUTPUT, PATIENCE};
use crate::extensions::bridge::dispatch::{call, CallContext, Emit};
use crate::extensions::bridge::events::FRAME_EVENT;
use crate::extensions::bridge::frames::FrameSource;
use crate::extensions::error::BridgeError;
use crate::extensions::host::ExtensionHost;
use crate::extensions::shell::flow::{STALL, WINDOW};

/// About 1 MB of output, then a line the typed command does not contain.
const FLOOD: &str = "head -c 1000000 /dev/zero | tr '\\0' x; echo; echo flood-$((6 * 7))\n";

/// Another frame of the notes extension, with its own emitter.
fn another_frame(s: &Setup, emitter: Arc<dyn Emit>) -> CallContext {
    let FrameSource::Bundle(bundle) = &s.notes.session.source else {
        panic!("a bundle frame");
    };
    CallContext {
        db: s.vault.clone(),
        host: Arc::clone(&s.host),
        session: s
            .host
            .frames
            .open(s.notes.session.extension_id, *bundle, "tab-2"),
        device: s.notes.device,
        emitter,
    }
}

fn ack(ctx: &CallContext, session: &str, count: u64) -> Result<Value, BridgeError> {
    call(
        ctx,
        "extension_shell_ack",
        &json!({ "sessionId": session, "count": count }),
    )
}

/// The output events `frame` got of `session`.
fn events_of(recorded: &Recorded, session: &str, frame: &str) -> Vec<Value> {
    recorded
        .all()
        .into_iter()
        .filter(|(e, p)| {
            e == FRAME_EVENT
                && p["type"] == OUTPUT
                && p["frame"] == frame
                && p["data"]["sessionId"] == session
        })
        .map(|(_, p)| p["data"].clone())
        .collect()
}

fn text_of(events: &[Value]) -> String {
    events.iter().filter_map(|d| d["data"].as_str()).collect()
}

/// The events `frame` has open, if the session's reading waits for it.
fn open_events(s: &Setup, session: &str, frame: &str) -> Option<u64> {
    s.host.shells.lock().get(session)?.open_events(frame)
}

#[test]
fn an_acknowledging_frame_holds_a_flooding_shell_back() {
    let s = setup();
    s.allow(&s.notes, &sh());
    let session = s.start(&s.notes);
    let ack = |count: u64| {
        call(
            &s.notes,
            "extension_shell_ack",
            &json!({ "sessionId": session, "count": count }),
        )
    };
    let count = |events: &[(String, Value)]| of(events, OUTPUT, &session).len() as u64;
    let events = || count(&s.recorded.all());
    s.write(&s.notes, &session, "echo ready\n").unwrap();
    s.recorded.waited_for_output(&session, "ready");
    // From its first acknowledgement on, the frame is waited for.
    ack(events()).unwrap();
    let acknowledged = events();

    // About 2 MB: hundreds of events, far more than one window.
    s.write(
        &s.notes,
        &session,
        "head -c 2000000 /dev/zero | tr '\\0' x; echo; echo flood-$((6 * 7))\n",
    )
    .unwrap();
    s.recorded
        .wait_until("a full window", |e| count(e) >= acknowledged + WINDOW);
    std::thread::sleep(Duration::from_millis(300));
    let held = events();
    let open = open_events(&s, &session, &s.notes.session.frame).expect("the frame is open");
    assert!(open <= WINDOW, "{open} open events, window {WINDOW}");
    std::thread::sleep(Duration::from_millis(300));
    assert_eq!(events(), held, "the reading waits");

    // A frame that keeps up gets everything (the echoed line has no "flood-42").
    let mut acked = acknowledged;
    while !output(&s.recorded.all(), &session).contains("flood-42") {
        let now = events();
        if now > acked {
            ack(now - acked).unwrap();
            acked = now;
        }
        std::thread::sleep(Duration::from_millis(5));
    }
    let flooded = output(&s.recorded.all(), &session)
        .chars()
        .filter(|c| *c == 'x')
        .count();
    assert!(flooded >= 2_000_000, "{flooded}");

    assert_eq!(
        ack(0).unwrap_err().code.as_u16(),
        3001,
        "a count is at least 1"
    );
    call(
        &s.notes,
        "extension_shell_close",
        &json!({ "sessionId": session }),
    )
    .unwrap();
    s.recorded.waited_for_exit(&session);
}

#[test]
fn a_frame_that_stops_acknowledging_holds_the_shell_back_only_for_a_while() {
    let s = setup();
    s.allow(&s.notes, &sh());
    let live = another_frame(&s, Arc::clone(&s.notes.emitter));
    let session = s.start(&s.notes);
    s.write(&s.notes, &session, "echo ready\n").unwrap();
    s.recorded.waited_for_output(&session, "ready");
    // Both frames acknowledge; then the notes frame stops (its page loaded anew, its acks failed).
    let (stuck, live_frame) = (s.notes.session.frame.clone(), live.session.frame.clone());
    ack(&s.notes, &session, 1).unwrap();
    ack(&live, &session, 1).unwrap();
    let mut acked = events_of(&s.recorded, &session, &live_frame).len() as u64;

    let started = Instant::now();
    s.write(&s.notes, &session, FLOOD).unwrap();
    // The frame that keeps acknowledging gets everything once the stuck one is no longer waited for.
    while !text_of(&events_of(&s.recorded, &session, &live_frame)).contains("flood-42") {
        assert!(started.elapsed() < PATIENCE, "the shell froze");
        let now = events_of(&s.recorded, &session, &live_frame).len() as u64;
        if now > acked {
            ack(&live, &session, now - acked).unwrap();
            acked = now;
        }
        std::thread::sleep(Duration::from_millis(5));
    }
    assert!(
        started.elapsed() >= STALL,
        "the stuck frame held it back first"
    );
    let flooded = text_of(&events_of(&s.recorded, &session, &live_frame))
        .chars()
        .filter(|c| *c == 'x')
        .count();
    assert!(flooded >= 1_000_000, "{flooded}");
    assert_eq!(
        open_events(&s, &session, &stuck),
        None,
        "no longer waited for"
    );
    assert!(open_events(&s, &session, &live_frame).is_some());

    // Acknowledging again, it counts again, from zero.
    ack(&s.notes, &session, 3).unwrap();
    assert_eq!(open_events(&s, &session, &stuck), Some(0));
    // A new page in a frame: it is not waited for until it acknowledges again.
    s.host.shells.frame_reloaded(&stuck);
    assert_eq!(open_events(&s, &session, &stuck), None);

    s.write(&s.notes, &session, "exit 0\n").unwrap();
    s.recorded.waited_for_exit(&session);
}

/// Acknowledges every output event of one frame as soon as it goes out, before `emit` returns.
/// Counts the events that were not yet counted as sent when they went out.
struct AckingAtOnce {
    recorded: Arc<Recorded>,
    host: Arc<ExtensionHost>,
    frame: OnceLock<String>,
    uncounted: AtomicUsize,
}

impl Emit for AckingAtOnce {
    fn emit(&self, event: &str, payload: Value) {
        let frame = self.frame.get().filter(|f| payload["frame"] == f.as_str());
        let session = payload["data"]["sessionId"].as_str();
        if let (FRAME_EVENT, Some(frame), Some(session)) = (event, frame, session) {
            if payload["type"] == OUTPUT {
                if let Some(running) = self.host.shells.lock().get(session) {
                    if running.open_events(frame) == Some(0) {
                        self.uncounted.fetch_add(1, Ordering::SeqCst);
                    }
                    running.acknowledge(frame, 1);
                }
            }
        }
        self.recorded.emit(event, payload);
    }
}

#[test]
fn an_acknowledgement_that_comes_before_emit_returns_still_counts() {
    let s = setup();
    s.allow(&s.notes, &sh());
    let emitter = Arc::new(AckingAtOnce {
        recorded: Arc::clone(&s.recorded),
        host: Arc::clone(&s.host),
        frame: OnceLock::new(),
        uncounted: AtomicUsize::new(0),
    });
    let fast = another_frame(&s, Arc::clone(&emitter) as Arc<dyn Emit>);
    emitter.frame.set(fast.session.frame.clone()).unwrap();
    let session = s.start(&fast);
    // The first acknowledgement makes the frame one that is waited for.
    ack(&fast, &session, 1).unwrap();
    let started = Instant::now();
    s.write(&fast, &session, FLOOD).unwrap();
    s.recorded.waited_for_output(&session, "flood-42");
    assert_eq!(
        emitter.uncounted.load(Ordering::SeqCst),
        0,
        "every event is counted before it goes out"
    );
    assert_eq!(
        open_events(&s, &session, &fast.session.frame),
        Some(0),
        "nothing is open, the window did not shrink"
    );
    assert!(started.elapsed() < STALL * 5, "the reading never stalled");
    call(
        &fast,
        "extension_shell_close",
        &json!({ "sessionId": session }),
    )
    .unwrap();
    s.recorded.waited_for_exit(&session);
}

#[test]
fn a_shell_that_exits_while_the_reading_waits_is_reaped_and_reported() {
    let s = setup();
    s.allow(&s.notes, &sh());
    let session = s.start(&s.notes);
    s.write(&s.notes, &session, "echo ready\n").unwrap();
    s.recorded.waited_for_output(&session, "ready");
    ack(&s.notes, &session, 1).unwrap();
    // Far more than a window, then the end; the frame never acknowledges again.
    s.write(
        &s.notes,
        &session,
        "head -c 1000000 /dev/zero | tr '\\0' x; exit 7\n",
    )
    .unwrap();
    assert_eq!(s.recorded.waited_for_exit(&session)["exitCode"], 7);
    assert_eq!(s.host.shells.count(s.notes.session.extension_id), 0);
    assert_eq!(
        of(&s.recorded.all(), EXIT, &session).len(),
        1,
        "one shell:exit"
    );
}
