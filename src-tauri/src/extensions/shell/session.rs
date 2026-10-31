//! A running shell (spec 017, US11, T110): its PTY, a thread that reads its output, one that
//! writes its input, and how it ends.
//!
//! The shell leads a session of its own on its terminal (`portable-pty` calls `setsid`), and an
//! interactive shell puts every job in a process group of its own. Ending it is a hangup to its
//! group, as from a closed terminal, then after [`HANGUP_GRACE`] a kill of every group in its
//! session ([`kill_process_tree`]); not every shell passes the hangup on to its jobs (dash does
//! not). When the shell's output ends, whatever is left in its session is killed before the
//! shell is reaped, so no job outlives its session. The output is read only as fast as the
//! frames hand it on ([`Flow`]).

use std::collections::HashMap;
use std::io::{Read, Write};
use std::sync::mpsc::{sync_channel, Receiver, SyncSender, TrySendError};
use std::sync::{Arc, Mutex, MutexGuard, OnceLock, PoisonError};
use std::time::Duration;

use serde_json::json;
use uuid::Uuid;

use super::flow::Flow;
use super::{shell_error, EXIT, MAX_SESSIONS, OUTPUT};
use crate::extensions::bridge::dispatch::{is_enabled, CallContext, Emit};
use crate::extensions::bridge::events::{emit_to_frames, emit_to_frames_counted};
use crate::extensions::error::{BridgeError, ExtensionErrorCode};
use crate::extensions::host::ExtensionHost;
use crate::vault_gate::{kill_process_tree, ChildGuard, ChildRegistry};

/// How long a shell has to end its jobs after the hangup before its session is killed.
const HANGUP_GRACE: Duration = Duration::from_millis(500);
/// Writes waiting for the shell to read its input; one more is refused, so a shell that does not
/// read never blocks a caller.
const MAX_QUEUED_WRITES: usize = 16;

/// Turns PTY output into text: a character split across two reads stays whole, an invalid byte
/// becomes U+FFFD.
#[derive(Default)]
pub struct Utf8Stream {
    pending: Vec<u8>,
}

impl Utf8Stream {
    /// The text of `bytes` that is complete so far; an unfinished character waits for the next.
    pub fn push(&mut self, bytes: &[u8]) -> String {
        self.pending.extend_from_slice(bytes);
        let mut text = String::new();
        let mut rest: &[u8] = &self.pending;
        loop {
            match std::str::from_utf8(rest) {
                Ok(valid) => {
                    text.push_str(valid);
                    rest = &[];
                    break;
                }
                Err(error) => {
                    let (valid, after) = rest.split_at(error.valid_up_to());
                    // `from_utf8` checked this prefix.
                    text.push_str(std::str::from_utf8(valid).unwrap_or_default());
                    match error.error_len() {
                        Some(bad) => {
                            text.push(char::REPLACEMENT_CHARACTER);
                            rest = &after[bad..];
                        }
                        None => {
                            rest = after;
                            break;
                        }
                    }
                }
            }
        }
        self.pending = rest.to_vec();
        text
    }
}

/// The process id of a session's shell until the shell is reaped: only until then can a signal
/// to its group or session not reach a process that got the id later.
#[derive(Clone)]
struct Leader(Arc<Mutex<Option<u32>>>);

impl Leader {
    fn lock(&self) -> MutexGuard<'_, Option<u32>> {
        self.0.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// A hangup to the shell's group, as from a closed terminal.
    fn hang_up(&self) {
        #[cfg(unix)]
        if let Some(group) = self
            .lock()
            .and_then(|pid| i32::try_from(pid).ok())
            .filter(|p| *p > 1)
        {
            // SAFETY: FFI call with a plain group id above 1 (never every process or our own
            // group) and a signal constant; the id is not reaped yet (the lock is held).
            unsafe {
                libc::kill(-group, libc::SIGHUP);
            }
        }
        #[cfg(not(unix))]
        let _ = self;
    }

    /// Kills the shell's group and every other group of its session.
    fn kill(&self) {
        if let Some(pid) = *self.lock() {
            kill_process_tree(pid);
        }
    }

    /// Kills what is left and forgets the id: the caller reaps the shell right after.
    fn kill_before_reaping(&self) {
        if let Some(pid) = self.lock().take() {
            kill_process_tree(pid);
        }
    }
}

/// One running shell.
pub(super) struct Session {
    pub(super) extension_id: Uuid,
    master: Box<dyn portable_pty::MasterPty + Send>,
    input: SyncSender<Vec<u8>>,
    leader: Leader,
    flow: Arc<Flow>,
}

impl Session {
    /// Queues `data` for the shell's input.
    pub(super) fn write(&self, data: Vec<u8>) -> Result<(), BridgeError> {
        self.input.try_send(data).map_err(|error| match error {
            TrySendError::Full(_) => BridgeError::new(
                ExtensionErrorCode::LimitExceeded,
                "the shell is not reading its input",
            ),
            TrySendError::Disconnected(_) => shell_error("the shell has ended"),
        })
    }

    pub(super) fn resize(&self, size: portable_pty::PtySize) -> Result<(), BridgeError> {
        self.master
            .resize(size)
            .map_err(|_| shell_error("the shell has ended"))
    }

    /// `frame` handed on `count` more output events.
    pub(super) fn acknowledge(&self, frame: &str, count: u64) {
        self.flow.acknowledge(frame, count);
    }

    /// The output events `frame` has open, if the reading waits for it.
    #[cfg(all(test, unix))]
    pub(super) fn open_events(&self, frame: &str) -> Option<u64> {
        self.flow.open_of(frame)
    }

    /// Ends the shell: a hangup now, a kill of its whole session after [`HANGUP_GRACE`]. Never
    /// waits. The terminal and the input go with `self`; the output thread reports the end.
    pub(super) fn end(self) {
        self.flow.end();
        let leader = self.leader;
        leader.hang_up();
        let later = leader.clone();
        let spawned = std::thread::Builder::new()
            .name("extension-shell-end".into())
            .spawn(move || {
                std::thread::sleep(HANGUP_GRACE);
                later.kill();
            });
        if spawned.is_err() {
            leader.kill();
        }
    }
}

/// The shells of this process, by session id.
#[derive(Default)]
pub struct ShellState {
    sessions: Mutex<HashMap<String, Session>>,
    children: OnceLock<ChildRegistry>,
}

impl ShellState {
    pub(super) fn lock(&self) -> MutexGuard<'_, HashMap<String, Session>> {
        self.sessions.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// The vault's registry of child processes; set once, at start.
    pub fn set_children(&self, children: ChildRegistry) {
        let _ = self.children.set(children);
    }

    /// Ends every shell of `extension_id` (its last frame closed, disabled, removed).
    pub fn end_all(&self, extension_id: Uuid) {
        let ended: Vec<Session> = {
            let mut sessions = self.lock();
            let ids: Vec<String> = sessions
                .iter()
                .filter(|(_, s)| s.extension_id == extension_id)
                .map(|(id, _)| id.clone())
                .collect();
            ids.iter().filter_map(|id| sessions.remove(id)).collect()
        };
        for session in ended {
            session.end();
        }
    }

    /// `frame` loaded a new page with a new SDK: the output events its old page had open are
    /// lost, so no session waits for it until it acknowledges again.
    pub fn frame_reloaded(&self, frame: &str) {
        for session in self.lock().values() {
            session.flow.forget(frame);
        }
    }

    /// How many shells `extension_id` has open.
    pub fn count(&self, extension_id: Uuid) -> usize {
        self.lock()
            .values()
            .filter(|s| s.extension_id == extension_id)
            .count()
    }
}

/// Starts `command` on a new terminal of `size` for the calling extension → the session id.
pub(super) fn start(
    ctx: &CallContext,
    command: portable_pty::CommandBuilder,
    size: portable_pty::PtySize,
) -> Result<String, BridgeError> {
    let pair = portable_pty::native_pty_system()
        .openpty(size)
        .map_err(|_| shell_error("no terminal available"))?;
    let mut child = pair
        .slave
        .spawn_command(command)
        .map_err(|_| shell_error("the program did not start"))?;
    drop(pair.slave);
    let leader = Leader(Arc::new(Mutex::new(child.process_id())));
    let guard = child
        .process_id()
        .and_then(|pid| ctx.host.shells.children.get().map(|c| c.register(pid)));
    let mut abandon = |error: BridgeError| {
        leader.kill_before_reaping();
        let _ = child.wait();
        Err(error)
    };
    let (reader, writer) = match (pair.master.try_clone_reader(), pair.master.take_writer()) {
        (Ok(reader), Ok(writer)) => (reader, writer),
        _ => return abandon(shell_error("no terminal available")),
    };
    let (input, queued) = sync_channel(MAX_QUEUED_WRITES);
    let flow = Arc::new(Flow::default());
    let fed = std::thread::Builder::new()
        .name("extension-shell-input".into())
        .spawn(move || feed(writer, queued));
    if fed.is_err() {
        return abandon(shell_error("the program did not start"));
    }

    let extension_id = ctx.session.extension_id;
    let session_id = Uuid::new_v4().to_string();
    {
        // Counted again under the lock: calls run in parallel.
        let mut sessions = ctx.host.shells.lock();
        if sessions
            .values()
            .filter(|s| s.extension_id == extension_id)
            .count()
            >= MAX_SESSIONS
        {
            drop(sessions);
            return abandon(BridgeError::new(
                ExtensionErrorCode::LimitExceeded,
                "too many shells",
            ));
        }
        sessions.insert(
            session_id.clone(),
            Session {
                extension_id,
                master: pair.master,
                input,
                leader: leader.clone(),
                flow: Arc::clone(&flow),
            },
        );
    }
    let ended = |shells: &ShellState| {
        if let Some(session) = shells.lock().remove(&session_id) {
            session.end();
        }
    };
    let pump = Pump {
        host: Arc::clone(&ctx.host),
        emitter: Arc::clone(&ctx.emitter),
        extension_id,
        session_id: session_id.clone(),
        leader,
        registered: guard,
        flow,
    };
    if std::thread::Builder::new()
        .name("extension-shell".into())
        .spawn(move || pump.run(reader, child))
        .is_err()
    {
        ended(&ctx.host.shells);
        return Err(shell_error("the program did not start"));
    }
    // The extension's last frame may have closed, or it may have been disabled or removed, while
    // the shell started; its `end_all` ran before the session was listed. Each of those changes
    // comes before its `end_all`, so one looked at after the listing is seen.
    if ctx.host.frames.of_extension(extension_id).is_empty() {
        ended(&ctx.host.shells);
        return Err(shell_error("the extension has no open frame"));
    }
    if !is_enabled(ctx).unwrap_or(false) {
        ended(&ctx.host.shells);
        return Err(BridgeError::disabled());
    }
    Ok(session_id)
}

/// Writes queued input until the session is gone or the terminal refuses it.
fn feed(mut writer: Box<dyn Write + Send>, queued: Receiver<Vec<u8>>) {
    for data in queued {
        if writer
            .write_all(&data)
            .and_then(|()| writer.flush())
            .is_err()
        {
            break;
        }
    }
}

/// Reads a session's output until it ends, then reports its exit code.
struct Pump {
    host: Arc<ExtensionHost>,
    emitter: Arc<dyn Emit>,
    extension_id: Uuid,
    session_id: String,
    leader: Leader,
    /// Keeps the shell registered with the vault until it is killed for good.
    registered: Option<ChildGuard>,
    flow: Arc<Flow>,
}

impl Pump {
    fn emit(&self, event: &str, data: serde_json::Value) {
        emit_to_frames(&*self.emitter, &self.host, self.extension_id, event, &data);
    }

    fn open_frames(&self) -> Vec<String> {
        self.host
            .frames
            .of_extension(self.extension_id)
            .iter()
            .map(|session| session.frame.clone())
            .collect()
    }

    fn run(
        mut self,
        mut reader: Box<dyn Read + Send>,
        mut child: Box<dyn portable_pty::Child + Send + Sync>,
    ) {
        let mut stream = Utf8Stream::default();
        let mut buffer = [0u8; 8192];
        loop {
            self.flow.wait_for_room(|| self.open_frames());
            match reader.read(&mut buffer) {
                Ok(0) | Err(_) => break,
                Ok(n) => {
                    let data = stream.push(&buffer[..n]);
                    if !data.is_empty() {
                        // Counted before it goes out: an acknowledgement never comes first.
                        emit_to_frames_counted(
                            &*self.emitter,
                            &self.host,
                            self.extension_id,
                            OUTPUT,
                            &json!({ "sessionId": self.session_id, "data": data }),
                            |frame| self.flow.sent(frame),
                        );
                    }
                }
            }
        }
        // No process holds the terminal any more; one that let go of it still must not outlive
        // the session.
        self.leader.kill_before_reaping();
        drop(self.registered.take());
        let exit_code = child.wait().ok().map(|status| status.exit_code());
        self.host.shells.lock().remove(&self.session_id);
        self.emit(
            EXIT,
            json!({ "sessionId": self.session_id, "exitCode": exit_code }),
        );
    }
}
