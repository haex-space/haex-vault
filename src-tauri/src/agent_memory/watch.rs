//! Where haex-vault learns that an Abo-Agent read foreign content with its own tools (spec 097 FR-019,
//! research R7, T071 to T073). The reader of a pass's output marks the delegate thread when a line
//! names such a tool. Haex-vault's MCP server handles the agent's calls independently of that
//! reader, so before it decides a write of the memory it sets a fence: the reader acknowledges it
//! only after every line the agent had written by then is read and judged. An agent writes a line
//! about its tool before its next call reaches haex-vault, so the write is judged with everything
//! the agent read before it. A fence that is not acknowledged counts as foreign, and the write asks.

use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Mutex, PoisonError};
use std::time::Duration;

use tokio::io::{AsyncBufRead, Lines};
use tokio::sync::{mpsc, oneshot};
use tokio::time::Instant;
use uuid::Uuid;

use crate::storage::chat_threads;
use crate::vault_gate::VaultDb;

#[cfg(test)]
#[path = "watch_tests.rs"]
mod tests;

/// How long the reader waits for one more line before it takes the output as read up to a fence.
/// The lines the agent wrote before its call have long been in the pipe when the call arrives.
const QUIET: Duration = Duration::from_millis(25);
/// The longest a fence waits while the agent keeps writing; what comes after it is later anyway.
const MOST_DRAIN: Duration = Duration::from_secs(1);
/// The longest the server waits for an acknowledgement before the write asks.
pub const FENCE_WAIT: Duration = Duration::from_secs(5);

type Fence = oneshot::Sender<()>;

struct Reader {
    id: u64,
    fences: mpsc::UnboundedSender<Fence>,
}

static READERS: Mutex<Option<HashMap<Uuid, Reader>>> = Mutex::new(None);
static NEXT_ID: AtomicU64 = AtomicU64::new(1);

/// The reader's side for one pass: it reads lines, answers fences and marks the thread.
pub struct Watch {
    thread: Option<Uuid>,
    db: Option<VaultDb>,
    marked: bool,
    id: u64,
    fences: Option<mpsc::UnboundedReceiver<Fence>>,
    pending: Vec<Fence>,
    since: Option<Instant>,
}

impl Watch {
    /// Starts watching the output of a pass in the chat thread `thread`. Without a thread or a
    /// database there is nothing to mark, and a fence finds no reader (the write asks).
    pub fn start(thread: Option<Uuid>, db: Option<VaultDb>) -> Self {
        let id = NEXT_ID.fetch_add(1, Ordering::Relaxed);
        let fences = match (thread, &db) {
            (Some(thread), Some(_)) => {
                let (sender, receiver) = mpsc::unbounded_channel();
                READERS
                    .lock()
                    .unwrap_or_else(PoisonError::into_inner)
                    .get_or_insert_with(HashMap::new)
                    .insert(thread, Reader { id, fences: sender });
                Some(receiver)
            }
            _ => None,
        };
        Self {
            thread,
            db,
            marked: false,
            id,
            fences,
            pending: Vec::new(),
            since: None,
        }
    }

    /// The next line of the output. A fence that arrived meanwhile is acknowledged once no line
    /// follows within [`QUIET`], or after [`MOST_DRAIN`]: every line before it has then been
    /// returned, and judged by the caller before it asked for this one.
    pub async fn next_line<R: AsyncBufRead + Unpin>(
        &mut self,
        lines: &mut Lines<R>,
    ) -> std::io::Result<Option<String>> {
        loop {
            if let Some(since) = self.since {
                if since.elapsed() < MOST_DRAIN {
                    if let Ok(line) = tokio::time::timeout(QUIET, lines.next_line()).await {
                        return line;
                    }
                }
                for fence in self.pending.drain(..) {
                    let _ = fence.send(());
                }
                self.since = None;
                continue;
            }
            let Some(fences) = self.fences.as_mut() else {
                return lines.next_line().await;
            };
            tokio::select! {
                biased;
                Some(fence) = fences.recv() => {
                    self.pending.push(fence);
                    self.since = Some(Instant::now());
                }
                line = lines.next_line() => return line,
            }
        }
    }

    /// Marks the thread: a line named a tool that brings foreign content. Written before the
    /// next line is read, so a fence after it finds the mark.
    pub async fn saw_foreign(&mut self) {
        if self.marked {
            return;
        }
        let (Some(thread), Some(db)) = (self.thread, self.db.as_ref()) else {
            return;
        };
        let now = crate::passwords::clock::unix_millis(std::time::SystemTime::now());
        match db
            .write(move |tx| chat_threads::mark_foreign_once(tx, thread, now))
            .await
        {
            Ok(_) => self.marked = true,
            Err(error) => {
                // Not marked: the fence still succeeds, so the write would run unasked. Close the
                // fences instead, so every later write of this pass asks.
                log::warn!("agent memory: the mark of foreign content was not written: {error}");
                self.fences = None;
                self.pending.clear();
                self.unregister();
            }
        }
    }

    fn unregister(&self) {
        let Some(thread) = self.thread else {
            return;
        };
        let mut readers = READERS.lock().unwrap_or_else(PoisonError::into_inner);
        if let Some(readers) = readers.as_mut() {
            if readers
                .get(&thread)
                .is_some_and(|reader| reader.id == self.id)
            {
                readers.remove(&thread);
            }
        }
    }
}

impl Drop for Watch {
    fn drop(&mut self) {
        self.unregister();
    }
}

/// Waits until the reader of the pass in `thread` has judged every line written so far. `false`
/// when no reader watches the thread or it does not answer within `wait`: the write then asks.
pub async fn fence(thread: Uuid, wait: Duration) -> bool {
    let (sender, receiver) = oneshot::channel();
    let sent = READERS
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .as_ref()
        .and_then(|readers| readers.get(&thread))
        .is_some_and(|reader| reader.fences.send(sender).is_ok());
    sent && matches!(tokio::time::timeout(wait, receiver).await, Ok(Ok(())))
}
