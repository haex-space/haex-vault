//! Where haex-vault learns that an Abo-Agent read foreign content with its own tools (spec 097 FR-019,
//! research R7, T071 to T073). The reader of a pass's output marks the delegate thread when a line
//! names such a tool. Haex-vault's MCP server handles the agent's calls independently of that
//! reader, so before it decides a write of the memory it sets a fence: the reader acknowledges it
//! only after every line the agent had written by then is read and judged. An agent writes a line
//! about its tool before its next call reaches haex-vault, so the write is judged with everything
//! the agent read before it. A fence that is not acknowledged counts as foreign, and the write asks.

use std::collections::{HashMap, HashSet};
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
/// If lines keep arriving until this deadline, unread output makes the fence fail.
const QUIET: Duration = Duration::from_millis(25);
/// The longest a fence waits while the agent keeps writing; after that its output is unread.
const MOST_DRAIN: Duration = Duration::from_secs(1);
/// The longest the server waits for an acknowledgement before the write asks.
pub const FENCE_WAIT: Duration = Duration::from_secs(5);

type Fence = oneshot::Sender<()>;

struct Reader {
    id: u64,
    fences: mpsc::UnboundedSender<Fence>,
}

static READERS: Mutex<Option<HashMap<Uuid, Reader>>> = Mutex::new(None);
/// Foreign content that could not yet be persisted must keep memory writes on the approval path.
static UNRECORDED_FOREIGN: Mutex<Option<HashSet<Uuid>>> = Mutex::new(None);
static NEXT_ID: AtomicU64 = AtomicU64::new(1);

fn mark_unrecorded(thread: Uuid) {
    UNRECORDED_FOREIGN
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .get_or_insert_with(HashSet::new)
        .insert(thread);
}

fn clear_unrecorded(thread: Uuid) {
    if let Some(threads) = UNRECORDED_FOREIGN
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .as_mut()
    {
        threads.remove(&thread);
    }
}

fn is_unrecorded(thread: Uuid) -> bool {
    UNRECORDED_FOREIGN
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .as_ref()
        .is_some_and(|threads| threads.contains(&thread))
}

/// Persists a foreign-content mark. A failed write stays fail-closed in this process; later memory
/// decisions retry it and still ask for the current write even if the retry succeeds.
pub async fn persist_foreign_mark(db: &VaultDb, thread: Uuid) -> bool {
    mark_unrecorded(thread);
    let now = crate::passwords::clock::unix_millis(std::time::SystemTime::now());
    if let Err(error) = db
        .write(move |tx| chat_threads::mark_foreign_once(tx, thread, now))
        .await
    {
        log::warn!("agent memory: the mark of foreign content was not written: {error}");
        return false;
    }

    let stored = db
        .read(move |q| chat_threads::memory_foreign_at(q, thread))
        .await
        .is_ok_and(|at| at.is_some());
    if stored {
        clear_unrecorded(thread);
    } else {
        log::warn!("agent memory: the mark of foreign content was not stored");
    }
    stored
}

/// Retries a failed mark before a memory decision. Returns true if this thread was fail-closed, so
/// the current write must ask even when this retry stores the durable mark.
pub async fn retry_unrecorded(db: &VaultDb, thread: Uuid) -> bool {
    if !is_unrecorded(thread) {
        return false;
    }
    let _ = persist_foreign_mark(db, thread).await;
    true
}

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

    /// The next line of the output. A fence that arrived meanwhile is acknowledged only once no
    /// line follows within [`QUIET`]. If output does not go quiet before [`MOST_DRAIN`], the pending
    /// fences fail because unread lines may still contain foreign content.
    pub async fn next_line<R: AsyncBufRead + Unpin>(
        &mut self,
        lines: &mut Lines<R>,
    ) -> std::io::Result<Option<String>> {
        loop {
            if let Some(since) = self.since {
                let remaining = MOST_DRAIN.saturating_sub(since.elapsed());
                if let Ok(line) =
                    tokio::time::timeout(QUIET.min(remaining), lines.next_line()).await
                {
                    return line;
                }
                if since.elapsed() < MOST_DRAIN {
                    // A quiet interval means every line before the fence was read and judged.
                    for fence in self.pending.drain(..) {
                        let _ = fence.send(());
                    }
                } else {
                    // Continuous output can leave a foreign line unread. Dropping the senders
                    // makes `fence` fail closed instead of treating that output as judged.
                    self.pending.clear();
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
        if persist_foreign_mark(db, thread).await {
            self.marked = true;
        } else {
            self.fences = None;
            self.pending.clear();
            self.unregister();
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
    if is_unrecorded(thread) {
        return false;
    }
    let (sender, receiver) = oneshot::channel();
    let sent = READERS
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .as_ref()
        .and_then(|readers| readers.get(&thread))
        .is_some_and(|reader| reader.fences.send(sender).is_ok());
    sent && matches!(tokio::time::timeout(wait, receiver).await, Ok(Ok(())))
}
