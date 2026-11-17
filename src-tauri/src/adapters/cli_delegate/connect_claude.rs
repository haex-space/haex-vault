//! `claude setup-token` connect-flow driver (spec 007-cli-delegate US2).
//!
//! A raw-ANSI Ink TUI, not a line-oriented CLI — piped, non-PTY stdio
//! produces zero output (research.md §5, live-verified). This runs the
//! process inside a real PTY (`portable-pty`) instead, extracts the OAuth URL
//! from an OSC 8 terminal hyperlink escape sequence, and — once the caller
//! collects the authorization code the user copies back from the browser —
//! writes it to the PTY and scrapes the resulting long-lived token from the
//! subsequent output.

use std::io::{Read, Write};
use std::sync::Arc;
use std::time::Duration;

use portable_pty::{native_pty_system, Child, CommandBuilder, PtySize};
use tokio::sync::{mpsc, Notify};
use uuid::Uuid;

use crate::adapters::AdapterError;
use crate::cli_tools::runtime::{DelegateRuntime, RunFolder};
use crate::vault_gate::{ChildGuard, ChildRegistry};

use super::process::map_spawn_error;
use super::DelegateVendor;

const URL_TIMEOUT: Duration = Duration::from_secs(30);
/// Generous: covers the time the user spends in the browser plus copying the
/// code back.
const TOKEN_TIMEOUT: Duration = Duration::from_secs(10 * 60);

fn pty_size() -> PtySize {
    PtySize {
        rows: 40,
        cols: 120,
        pixel_width: 0,
        pixel_height: 0,
    }
}

/// Crate-visible (not just `cli_delegate`-visible) because
/// `providers::connect`'s background auto-completion watcher
/// (`try_recv_terminal`) and its tests construct these directly to drive
/// a `ClaudeConnectSession` test double without a real PTY/process.
pub(crate) enum ReaderEvent {
    Url(String),
    Token(String),
    Eof,
    Error(String),
}

/// A `ReaderEvent` collapsed to the three outcomes that end a connect
/// flow, for `ClaudeConnectSession::try_recv_terminal`'s non-blocking
/// poll — used by `providers::connect`'s background auto-completion
/// watcher (see its doc comment for why the poll must be non-blocking).
pub(crate) enum TerminalOutcome {
    Token(String),
    Ended,
    Errored(String),
}

/// Extracts the URI from the first OSC 8 terminal hyperlink escape sequence
/// in `buf`: `\x1b]8;<params>;<uri><BEL or ST>`. `claude setup-token`
/// re-emits the whole URL inside this payload on every redraw even though the
/// *visible* text is word-wrapped differently each time (research.md §5) —
/// this scans raw bytes for that reason, rather than the rendered/wrapped
/// text. Pure and unit-testable.
pub(super) fn extract_osc8_url(buf: &[u8]) -> Option<String> {
    const OSC8_PREFIX: &[u8] = b"\x1b]8;";
    let prefix_at = find_subslice(buf, OSC8_PREFIX)?;
    let after_prefix = prefix_at + OSC8_PREFIX.len();
    let params_end = after_prefix + buf[after_prefix..].iter().position(|&b| b == b';')?;
    let uri_start = params_end + 1;
    let uri_end = uri_start
        + buf[uri_start..]
            .iter()
            .position(|&b| b == 0x07 || b == b'\\')?;
    std::str::from_utf8(&buf[uri_start..uri_end])
        .ok()
        .map(str::to_string)
        .filter(|s| !s.is_empty())
}

fn find_subslice(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    haystack.windows(needle.len()).position(|w| w == needle)
}

/// Strips CSI (`\x1b[...<letter>`) and OSC (`\x1b]...BEL`) escape sequences,
/// leaving plain content. Used only for scraping the final token screen — the
/// URL step reads OSC 8 payloads from raw bytes instead via
/// [`extract_osc8_url`], since stripping would destroy exactly what it needs.
fn strip_ansi(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut chars = text.chars().peekable();
    while let Some(c) = chars.next() {
        if c != '\u{1b}' {
            out.push(c);
            continue;
        }
        match chars.peek() {
            Some('[') => {
                chars.next();
                for next in chars.by_ref() {
                    if next.is_ascii_alphabetic() {
                        break;
                    }
                }
            }
            Some(']') => {
                chars.next();
                for next in chars.by_ref() {
                    if next == '\u{7}' {
                        break;
                    }
                }
            }
            _ => {}
        }
    }
    out
}

/// Scans ANSI-stripped output for Anthropic's documented long-lived OAuth
/// token prefix. research.md §5: the exact final-screen layout is not
/// live-verified (completing the flow would mint a real credential against
/// the operator's subscription), so this is deliberately a tolerant pattern
/// match rather than a screen-position assumption. Pure and unit-testable.
pub(super) fn extract_oauth_token(buf: &[u8]) -> Option<String> {
    const PREFIX: &str = "sk-ant-oat";
    let text = strip_ansi(&String::from_utf8_lossy(buf));
    let start = text.find(PREFIX)?;
    let token: String = text[start..]
        .chars()
        .take_while(|c| c.is_ascii_alphanumeric() || *c == '-' || *c == '_')
        .collect();
    (token.len() > PREFIX.len()).then_some(token)
}

/// `token_ready` is notified exactly once, right after the terminal event
/// (`Token`/`Eof`/`Error`) that ends this thread is pushed onto `tx` — never
/// for `Url`, which `start_claude_connect`'s own wait loop already drains
/// before anyone else can observe this channel. This lets a background
/// watcher (`providers::connect`) learn a token arrived without polling or
/// blocking on `events.recv()` itself (see `ClaudeConnectSession::token_ready`).
fn spawn_reader_thread(
    mut reader: Box<dyn Read + Send>,
    token_ready: Arc<Notify>,
) -> mpsc::UnboundedReceiver<ReaderEvent> {
    let (tx, rx) = mpsc::unbounded_channel();
    std::thread::spawn(move || {
        let mut buf: Vec<u8> = Vec::new();
        let mut url_sent = false;
        let mut chunk = [0_u8; 4096];
        loop {
            match reader.read(&mut chunk) {
                Ok(0) => {
                    let _ = tx.send(ReaderEvent::Eof);
                    token_ready.notify_one();
                    return;
                }
                Ok(n) => {
                    buf.extend_from_slice(&chunk[..n]);
                    if !url_sent {
                        if let Some(url) = extract_osc8_url(&buf) {
                            url_sent = true;
                            if tx.send(ReaderEvent::Url(url)).is_err() {
                                return;
                            }
                        }
                    }
                    if let Some(token) = extract_oauth_token(&buf) {
                        let _ = tx.send(ReaderEvent::Token(token));
                        token_ready.notify_one();
                        return;
                    }
                }
                Err(error) => {
                    let _ = tx.send(ReaderEvent::Error(error.to_string()));
                    token_ready.notify_one();
                    return;
                }
            }
        }
    });
    rx
}

/// A `claude setup-token` flow paused after showing the OAuth URL, waiting
/// for either the user's authorization code via [`submit_claude_code`] or
/// the token arriving on its own (`providers::connect`'s background
/// watcher) — the current Anthropic OAuth page never shows a code to paste
/// back, so the browser round-trip alone must be able to finish the flow.
pub struct ClaudeConnectSession {
    /// Identifies this specific connect attempt. `DelegateConnectState`
    /// only ever holds one pending Claude session at a time — starting a
    /// new `connect_cli_delegate(claude)` call replaces it — so a
    /// background watcher spawned for an earlier attempt compares this
    /// against the id it was given to detect it's been superseded rather
    /// than acting on a session that isn't the one it was watching.
    id: Uuid,
    child: Box<dyn Child + Send + Sync>,
    /// Keeps the PTY process registered until this pending connect session is dropped.
    _registration: Option<ChildGuard>,
    writer: Box<dyn Write + Send>,
    events: mpsc::UnboundedReceiver<ReaderEvent>,
    /// Notified once, right when a terminal `ReaderEvent` lands in
    /// `events` — see [`Self::token_ready`].
    token_ready: Arc<Notify>,
    /// The run folder of the flow, in the vault's home; removed when the session ends.
    _run: RunFolder,
}

impl Drop for ClaudeConnectSession {
    fn drop(&mut self) {
        let _ = self.child.kill();
    }
}

impl ClaudeConnectSession {
    /// Returns the unique identifier for this connect attempt.
    pub fn id(&self) -> Uuid {
        self.id
    }

    /// A clone of the notifier a background watcher awaits instead of
    /// calling `events.recv()` directly — that call would block
    /// indefinitely while the watcher holds `DelegateConnectState`'s
    /// mutex, starving a concurrent manual `submit_cli_delegate_code`
    /// call of the same lock it needs to submit a pasted code.
    pub fn token_ready(&self) -> Arc<Notify> {
        self.token_ready.clone()
    }

    /// Non-blocking check for a terminal event, collapsing the channel's
    /// `Disconnected` case (reader thread gone without sending one, which
    /// should not happen but is not a token either) into `Ended` like a
    /// plain EOF. Only ever called after `token_ready` fires, so it drains
    /// queued non-terminal URL events before returning a terminal outcome.
    pub(crate) fn try_recv_terminal(&mut self) -> Option<TerminalOutcome> {
        loop {
            match self.events.try_recv() {
                Ok(ReaderEvent::Token(token)) => return Some(TerminalOutcome::Token(token)),
                Ok(ReaderEvent::Eof) | Err(mpsc::error::TryRecvError::Disconnected) => {
                    return Some(TerminalOutcome::Ended);
                }
                Ok(ReaderEvent::Error(error)) => return Some(TerminalOutcome::Errored(error)),
                Ok(ReaderEvent::Url(_)) => continue,
                Err(mpsc::error::TryRecvError::Empty) => return None,
            }
        }
    }

    /// Builds a session around an injected event channel instead of a real
    /// PTY/`claude` process, so `providers::connect`'s auto-completion
    /// watcher can be tested by feeding it `ReaderEvent`s directly. The
    /// child is a real, disposable platform-compatible process
    /// (`portable_pty::Child` is already implemented for
    /// `std::process::Child`) purely so `Drop` has something harmless to
    /// kill; nothing reads or writes through it.
    #[cfg(test)]
    pub(crate) fn new_for_test(
        events: mpsc::UnboundedReceiver<ReaderEvent>,
        token_ready: Arc<Notify>,
    ) -> Self {
        #[cfg(windows)]
        let child = std::process::Command::new("timeout")
            .args(["/T", "300", "/NOBREAK"])
            .spawn()
            .expect("spawn a disposable placeholder child for the test double");
        #[cfg(not(windows))]
        let child = std::process::Command::new("sleep")
            .arg("300")
            .spawn()
            .expect("spawn a disposable placeholder child for the test double");
        let scratch = tempfile::TempDir::new().expect("allocate a temp dir for the test double");
        Self {
            id: Uuid::new_v4(),
            child: Box::new(child),
            _registration: None,
            writer: Box::new(std::io::sink()),
            events,
            token_ready,
            _run: RunFolder::create(scratch.path(), scratch.path())
                .expect("allocate a run folder for the test double"),
        }
    }
}

/// Spawns `claude setup-token` in a PTY inside an isolated `CLAUDE_CONFIG_DIR`
/// (spec.md FR-003) and waits for the OAuth URL to appear. The vault's own installation is
/// started, in a run folder of the vault's home, under the vault's containment (spec 094).
pub async fn start_claude_connect(
    runtime: &DelegateRuntime,
    children: ChildRegistry,
) -> Result<(ClaudeConnectSession, String), AdapterError> {
    super::process::ensure_available(crate::platform::capabilities().cli_delegates)?;
    let runtime = runtime.clone();
    let mut session = tokio::task::spawn_blocking(move || {
        let run = runtime.run_folder().map_err(|error| AdapterError::Http {
            reason: format!("failed to create the run folder for claude login: {error}"),
        })?;

        let pty_system = native_pty_system();
        let pair = pty_system
            .openpty(pty_size())
            .map_err(|error| AdapterError::Http {
                reason: format!("failed to allocate a pty: {error}"),
            })?;

        let mut cmd = CommandBuilder::new(&runtime.executable);
        cmd.args(&runtime.vendor.config_args);
        cmd.arg("setup-token");
        cmd.cwd(run.path());
        // A pty child is not a `std::process::Command`; the containment is applied to one and its
        // changes are copied over, on top of the host environment `CommandBuilder` inherits.
        let mut probe = std::process::Command::new(&runtime.executable);
        runtime.contain(&mut probe, &run);
        for (name, value) in probe.get_envs() {
            match value {
                Some(value) => cmd.env(name, value),
                None => cmd.env_remove(name),
            }
        }

        let child = pair.slave.spawn_command(cmd).map_err(|error| {
            match error.downcast::<std::io::Error>() {
                Ok(io_error) => map_spawn_error(DelegateVendor::Claude, io_error),
                Err(other) => AdapterError::Http {
                    reason: format!("failed to start claude setup-token: {other}"),
                },
            }
        })?;
        let registration = child.process_id().map(|pid| children.register(pid));
        drop(pair.slave);

        let reader = pair
            .master
            .try_clone_reader()
            .map_err(|error| AdapterError::Http {
                reason: format!("failed to open pty reader: {error}"),
            })?;
        let writer = pair
            .master
            .take_writer()
            .map_err(|error| AdapterError::Http {
                reason: format!("failed to open pty writer: {error}"),
            })?;

        let token_ready = Arc::new(Notify::new());
        Ok::<_, AdapterError>(ClaudeConnectSession {
            id: Uuid::new_v4(),
            child,
            _registration: registration,
            writer,
            events: spawn_reader_thread(reader, token_ready.clone()),
            token_ready,
            _run: run,
        })
    })
    .await
    .map_err(|error| AdapterError::Http {
        reason: format!("connect task join failed: {error}"),
    })??;

    let url = tokio::time::timeout(URL_TIMEOUT, async {
        loop {
            match session.events.recv().await {
                Some(ReaderEvent::Url(url)) => return Ok(url),
                Some(ReaderEvent::Token(_)) => continue,
                Some(ReaderEvent::Eof) => {
                    return Err(AdapterError::Http {
                        reason: "claude setup-token exited before showing a login URL".into(),
                    })
                }
                Some(ReaderEvent::Error(error)) => {
                    return Err(AdapterError::Http { reason: error })
                }
                None => {
                    return Err(AdapterError::Http {
                        reason: "claude setup-token's output stream ended unexpectedly".into(),
                    })
                }
            }
        }
    })
    .await
    .map_err(|_| AdapterError::Http {
        reason: format!("timed out after {URL_TIMEOUT:?} waiting for the login URL"),
    })??;

    Ok((session, url))
}

/// Submits the authorization code the user copied back from the browser,
/// then waits for the resulting long-lived OAuth token.
pub async fn submit_claude_code(
    session: &mut ClaudeConnectSession,
    code: &str,
) -> Result<Vec<u8>, AdapterError> {
    let mut line = code.trim().as_bytes().to_vec();
    line.push(b'\r');
    session
        .writer
        .write_all(&line)
        .map_err(|error| AdapterError::Http {
            reason: format!("failed to submit the authorization code: {error}"),
        })?;

    let token = tokio::time::timeout(TOKEN_TIMEOUT, async {
        loop {
            match session.events.recv().await {
                Some(ReaderEvent::Token(token)) => return Ok(token),
                Some(ReaderEvent::Url(_)) => continue,
                Some(ReaderEvent::Eof) | None => return Err(AdapterError::InvalidCredentials),
                Some(ReaderEvent::Error(error)) => {
                    return Err(AdapterError::Http { reason: error })
                }
            }
        }
    })
    .await
    .map_err(|_| AdapterError::Http {
        reason: format!("timed out after {TOKEN_TIMEOUT:?} waiting for the login token"),
    })??;

    Ok(token.into_bytes())
}
