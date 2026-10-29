//! Tests for the CDP connection (spec 065, T020) over in-memory pipes: a fake browser reads the
//! commands and writes what each test scripts.

use std::io::{BufRead, BufReader, PipeReader, PipeWriter, Write};
use std::time::Duration;

use serde_json::{json, Value};

use super::Connection;
use crate::agent_browser::error::BrowserError;

/// The browser's ends: where it reads holzi's commands and where it writes.
struct FakeBrowser {
    commands: BufReader<PipeReader>,
    out: PipeWriter,
}

impl FakeBrowser {
    /// The next command holzi sent.
    fn command(&mut self) -> Value {
        let mut bytes = Vec::new();
        self.commands.read_until(0, &mut bytes).expect("read");
        assert_eq!(bytes.pop(), Some(0), "every message ends with NUL");
        serde_json::from_slice(&bytes).expect("json")
    }

    fn send(&mut self, message: Value) {
        let mut bytes = serde_json::to_vec(&message).unwrap();
        bytes.push(0);
        self.out.write_all(&bytes).expect("write");
    }
}

fn connected() -> (Connection, FakeBrowser) {
    let (to_holzi_read, to_holzi_write) = std::io::pipe().expect("pipe");
    let (to_browser_read, to_browser_write) = std::io::pipe().expect("pipe");
    let connection = Connection::new(to_holzi_read, to_browser_write);
    let browser = FakeBrowser {
        commands: BufReader::new(to_browser_read),
        out: to_holzi_write,
    };
    (connection, browser)
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn replies_are_matched_by_id_whatever_their_order() {
    let (connection, mut browser) = connected();
    let script = std::thread::spawn(move || {
        let first = browser.command();
        let second = browser.command();
        assert_eq!(first["method"], "A.one");
        assert_eq!(second["sessionId"], "S1");
        // Answer the second first.
        browser.send(json!({ "id": second["id"], "result": { "n": 2 } }));
        browser.send(json!({ "id": first["id"], "result": { "n": 1 } }));
        browser
    });
    let (one, two) = tokio::join!(connection.call("A.one", json!({}), None), async {
        // Make sure "A.one" goes out first.
        tokio::time::sleep(Duration::from_millis(50)).await;
        connection.call("A.two", json!({}), Some("S1")).await
    });
    assert_eq!(one.unwrap()["n"], 1);
    assert_eq!(two.unwrap()["n"], 2);
    drop(script.join().unwrap());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn an_error_reply_is_a_protocol_error() {
    let (connection, mut browser) = connected();
    let script = std::thread::spawn(move || {
        let command = browser.command();
        browser.send(json!({
            "id": command["id"],
            "error": { "code": -32601, "message": "'X.y' wasn't found" }
        }));
        browser
    });
    let result = connection.call("X.y", json!({}), None).await;
    assert_eq!(
        result,
        Err(BrowserError::Protocol("'X.y' wasn't found".to_owned()))
    );
    drop(script.join().unwrap());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn events_go_to_the_subscribers_of_their_session() {
    let (connection, mut browser) = connected();
    let mut page = connection.subscribe(Some("S1"));
    let mut other = connection.subscribe(Some("S2"));
    let mut own = connection.subscribe(None);
    browser.send(json!({ "method": "Page.loadEventFired", "params": {}, "sessionId": "S1" }));
    browser.send(json!({ "method": "Target.targetCreated", "params": { "t": 1 } }));
    let event = page.recv().await.expect("the page's event");
    assert_eq!(event.method, "Page.loadEventFired");
    assert_eq!(event.session_id.as_deref(), Some("S1"));
    let event = own.recv().await.expect("the browser's event");
    assert_eq!(event.method, "Target.targetCreated");
    assert_eq!(event.params["t"], 1);
    drop(browser);
    assert!(
        other.recv().await.is_none(),
        "S2 got nothing, and its stream ends with the pipe"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_reply_after_cancel_is_dropped() {
    let (connection, mut browser) = connected();
    let late = tokio::time::timeout(
        Duration::from_millis(100),
        connection.call("Slow.call", json!({}), None),
    )
    .await;
    assert!(late.is_err(), "the call was cancelled by its timeout");
    let slow = browser.command();
    // The late reply arrives; the next call still gets its own.
    let script = std::thread::spawn(move || {
        browser.send(json!({ "id": slow["id"], "result": { "late": true } }));
        let next = browser.command();
        browser.send(json!({ "id": next["id"], "result": { "next": true } }));
        browser
    });
    let next = connection.call("Next.call", json!({}), None).await.unwrap();
    assert_eq!(next, json!({ "next": true }));
    drop(script.join().unwrap());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_closed_pipe_ends_every_waiting_call() {
    let (connection, mut browser) = connected();
    let script = std::thread::spawn(move || {
        browser.command();
        browser.command();
        // The browser dies without answering.
        drop(browser);
    });
    let (one, two) = tokio::join!(
        connection.call("A.one", json!({}), None),
        connection.call("A.two", json!({}), None)
    );
    script.join().unwrap();
    for result in [one, two] {
        assert!(matches!(result, Err(BrowserError::Unavailable(_))));
        assert_eq!(result.unwrap_err().code(), "browser_unavailable");
    }
    assert!(connection.is_closed());
    assert!(matches!(
        connection.call("A.three", json!({}), None).await,
        Err(BrowserError::Unavailable(_))
    ));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn closing_holzis_end_lets_the_browser_read_the_end() {
    let (connection, mut browser) = connected();
    connection.close();
    let mut rest = Vec::new();
    let read = std::thread::spawn(move || {
        browser.commands.read_until(0, &mut rest).expect("read");
        rest
    });
    assert!(
        read.join().unwrap().is_empty(),
        "end of the pipe, no message"
    );
}
