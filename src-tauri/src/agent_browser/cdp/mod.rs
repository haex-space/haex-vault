//! The CDP connection to the agent browser (spec 065, research R1, R2): JSON messages separated by
//! NUL over the two pipe ends of `--remote-debugging-pipe`. Replies are matched by `id`; events go
//! to the subscribers of their `sessionId` (flat mode), browser events to those of no session.
//!
//! Two plain threads move the bytes, one per pipe end, so the same code serves the pipes of every
//! operating system (Windows' anonymous pipes have no async reads) and the in-memory pipes of the
//! tests. When the browser's end closes, every waiting call ends with
//! [`BrowserError::Unavailable`] and the event streams end.

pub mod browser;
pub mod target;

use std::collections::HashMap;
use std::io::{BufRead, BufReader, Read, Write};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{mpsc as std_mpsc, Arc, Mutex, PoisonError};

use serde_json::{json, Value};
use tokio::sync::{mpsc, oneshot};

use super::error::{BrowserError, Result};

/// One event of the browser.
#[derive(Debug, Clone, PartialEq)]
pub struct Event {
    pub method: String,
    pub params: Value,
    /// The flat-mode session it belongs to; `None` for the browser itself.
    pub session_id: Option<String>,
}

type Reply = oneshot::Sender<Result<Value>>;

#[derive(Default)]
struct Shared {
    pending: Mutex<HashMap<u64, Reply>>,
    /// Subscribers per session; the key `""` is the browser.
    events: Mutex<HashMap<String, Vec<mpsc::UnboundedSender<Event>>>>,
    closed: AtomicBool,
}

impl Shared {
    fn close(&self, why: &str) {
        self.closed.store(true, Ordering::SeqCst);
        let waiting: Vec<Reply> = self
            .pending
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .drain()
            .map(|(_, reply)| reply)
            .collect();
        for reply in waiting {
            let _ = reply.send(Err(BrowserError::Unavailable(why.to_owned())));
        }
        self.events
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clear();
    }

    fn dispatch(&self, message: Value) {
        if let Some(id) = message.get("id").and_then(Value::as_u64) {
            let reply = self
                .pending
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .remove(&id);
            // A reply nobody waits for any more (the call was cancelled) is dropped here.
            if let Some(reply) = reply {
                let result = match message.get("error") {
                    Some(error) => Err(BrowserError::Protocol(
                        error
                            .get("message")
                            .and_then(Value::as_str)
                            .unwrap_or("error")
                            .to_owned(),
                    )),
                    None => Ok(message.get("result").cloned().unwrap_or(Value::Null)),
                };
                let _ = reply.send(result);
            }
            return;
        }
        let Some(method) = message.get("method").and_then(Value::as_str) else {
            return;
        };
        let session_id = message
            .get("sessionId")
            .and_then(Value::as_str)
            .map(str::to_owned);
        let event = Event {
            method: method.to_owned(),
            params: message.get("params").cloned().unwrap_or(Value::Null),
            session_id,
        };
        let key = event.session_id.clone().unwrap_or_default();
        let mut events = self.events.lock().unwrap_or_else(PoisonError::into_inner);
        if let Some(subscribers) = events.get_mut(&key) {
            subscribers.retain(|subscriber| subscriber.send(event.clone()).is_ok());
        }
    }
}

/// Removes a call from the waiting ones when its future is dropped (cancelled or timed out).
struct Waiting<'a> {
    shared: &'a Shared,
    id: u64,
}

impl Drop for Waiting<'_> {
    fn drop(&mut self) {
        self.shared
            .pending
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .remove(&self.id);
    }
}

/// A connection to one browser. Dropping it closes holzi's end of the pipe.
pub struct Connection {
    shared: Arc<Shared>,
    outgoing: Mutex<Option<std_mpsc::Sender<Vec<u8>>>>,
    next_id: AtomicU64,
}

impl Connection {
    /// A connection over `from_browser` (the browser writes, fd 4) and `to_browser` (fd 3).
    pub fn new(
        from_browser: impl Read + Send + 'static,
        to_browser: impl Write + Send + 'static,
    ) -> Self {
        let shared = Arc::new(Shared::default());
        let reading = Arc::clone(&shared);
        let spawned = std::thread::Builder::new()
            .name("agent-browser-read".to_owned())
            .spawn(move || read_loop(from_browser, &reading));
        if spawned.is_err() {
            shared.close("no reader thread");
        }
        let (outgoing, queue) = std_mpsc::channel::<Vec<u8>>();
        let writing = Arc::clone(&shared);
        let spawned = std::thread::Builder::new()
            .name("agent-browser-write".to_owned())
            .spawn(move || write_loop(to_browser, &queue, &writing));
        if spawned.is_err() {
            shared.close("no writer thread");
        }
        Self {
            shared,
            outgoing: Mutex::new(Some(outgoing)),
            next_id: AtomicU64::new(1),
        }
    }

    /// Whether the browser's end has closed.
    pub fn is_closed(&self) -> bool {
        self.shared.closed.load(Ordering::SeqCst)
    }

    /// Calls `method` in `session` (`None`: the browser) and waits for its reply.
    pub async fn call(&self, method: &str, params: Value, session: Option<&str>) -> Result<Value> {
        if self.is_closed() {
            return Err(BrowserError::Unavailable("the browser ended".to_owned()));
        }
        let id = self.next_id.fetch_add(1, Ordering::SeqCst);
        let (reply, answer) = oneshot::channel();
        self.shared
            .pending
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .insert(id, reply);
        let _waiting = Waiting {
            shared: &self.shared,
            id,
        };
        let mut message = json!({ "id": id, "method": method, "params": params });
        if let Some(session) = session {
            message["sessionId"] = Value::String(session.to_owned());
        }
        let mut bytes = serde_json::to_vec(&message)
            .map_err(|error| BrowserError::Protocol(error.to_string()))?;
        bytes.push(0);
        let sent = self
            .outgoing
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .as_ref()
            .is_some_and(|outgoing| outgoing.send(bytes).is_ok());
        if !sent {
            return Err(BrowserError::Unavailable("the pipe is closed".to_owned()));
        }
        answer
            .await
            .unwrap_or_else(|_| Err(BrowserError::Unavailable("the browser ended".to_owned())))
    }

    /// The events of `session` (`None`: the browser's own) from now on; the stream ends with the
    /// connection.
    pub fn subscribe(&self, session: Option<&str>) -> mpsc::UnboundedReceiver<Event> {
        let (sender, receiver) = mpsc::unbounded_channel();
        if !self.is_closed() {
            self.shared
                .events
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .entry(session.unwrap_or_default().to_owned())
                .or_default()
                .push(sender);
        }
        receiver
    }

    /// Closes holzi's end of the pipe; the browser reads its end and quits.
    pub fn close(&self) {
        self.outgoing
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .take();
    }
}

fn read_loop(from_browser: impl Read, shared: &Shared) {
    let mut reader = BufReader::new(from_browser);
    let mut message = Vec::new();
    loop {
        message.clear();
        match reader.read_until(0, &mut message) {
            Ok(0) => break,
            Ok(_) => {
                if message.last() == Some(&0) {
                    message.pop();
                } else {
                    // The pipe ended inside a message.
                    break;
                }
                match serde_json::from_slice::<Value>(&message) {
                    Ok(value) => shared.dispatch(value),
                    Err(error) => log::warn!("agent browser: unreadable message: {error}"),
                }
            }
            Err(error) if error.kind() == std::io::ErrorKind::Interrupted => {}
            Err(_) => break,
        }
    }
    shared.close("the browser closed the pipe");
}

fn write_loop(mut to_browser: impl Write, queue: &std_mpsc::Receiver<Vec<u8>>, shared: &Shared) {
    while let Ok(bytes) = queue.recv() {
        if to_browser
            .write_all(&bytes)
            .and_then(|()| to_browser.flush())
            .is_err()
        {
            shared.close("the pipe to the browser broke");
            return;
        }
    }
}

#[cfg(test)]
mod mod_tests;
