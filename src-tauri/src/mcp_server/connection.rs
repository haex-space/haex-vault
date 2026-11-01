//! One connection from the `--mcp-agent` helper (spec 021 US1, US8, contracts/agent-helper.md): the
//! greeting decides whether the agent is known, waits for a registration, or is turned away; then
//! rmcp serves the session while a pump slips control lines (the new credential, the reason for
//! closing) between its messages. A registration runs next to the session, so the agent can finish
//! `initialize` while the user decides.

use std::pin::Pin;
use std::sync::Arc;
use std::task::{Context, Poll};
use std::time::Duration;

use super::attach;
use super::delegates;
use super::endpoint::BoxStream;
use super::hello::{
    control_line, json_line, parse_greeting, Control, Greeting, HelloReply, RejectReason,
    MAX_GREETING_BYTES,
};
use super::live::Link;
use super::manage;
use super::prompts::GateTicket;
use super::registration::{self, Previous, RegistrationContext};
use super::registry::{self, AgentStatus};
use super::server::{Core, Notice};
use super::session::{Session, SessionAgent};
use rmcp::ServiceExt;
use tokio::io::{
    AsyncBufReadExt, AsyncRead, AsyncReadExt, AsyncWriteExt, BufReader, DuplexStream, ReadBuf,
    ReadHalf, WriteHalf,
};
use tokio::sync::mpsc;

/// How long the pump may take to write the last lines after the session ended.
const FLUSH_DEADLINE: Duration = Duration::from_secs(1);

/// How a greeting is admitted.
enum Admission {
    Known(SessionAgent),
    /// Waits for the registration dialog; an expired agent brings its previous registration.
    Pending(GateTicket, Option<Previous>),
    Rejected(RejectReason),
}

/// Serves one connection until the agent leaves, the user declines it or the server stops.
pub async fn serve(core: Arc<Core>, stream: BoxStream) {
    let (read_half, mut write_half) = tokio::io::split(stream);
    let mut reader = BufReader::new(read_half);
    let greeting = match read_greeting(&mut reader, core.limits.greeting_deadline).await {
        Some(greeting) => greeting,
        None => {
            let reply = HelloReply::Rejected {
                reason: RejectReason::Malformed,
            };
            let _ = write_half.write_all(json_line(&reply).as_bytes()).await;
            return;
        }
    };
    let admission = admit(&core, &greeting).await;
    let reply = match &admission {
        Admission::Known(_) => HelloReply::Accepted,
        Admission::Pending(..) => HelloReply::Pending,
        Admission::Rejected(reason) => HelloReply::Rejected { reason: *reason },
    };
    if write_half
        .write_all(json_line(&reply).as_bytes())
        .await
        .is_err()
    {
        return;
    }
    let (agent, waiting) = match admission {
        Admission::Known(agent) => (Some(agent), None),
        Admission::Pending(ticket, previous) => (None, Some((ticket, previous))),
        Admission::Rejected(_) => return,
    };
    run_session(core, greeting, agent, waiting, reader, write_half).await;
}

async fn run_session(
    core: Arc<Core>,
    greeting: Greeting,
    agent: Option<SessionAgent>,
    waiting: Option<(GateTicket, Option<Previous>)>,
    reader: BufReader<ReadHalf<BoxStream>>,
    write_half: WriteHalf<BoxStream>,
) {
    let (controls, control_rx) = mpsc::unbounded_channel();
    let (rmcp_side, pump_side) = tokio::io::duplex(64 * 1024);
    let pump = tokio::spawn(pump(pump_side, write_half, control_rx));
    let known_id = agent.as_ref().map(|agent| agent.id);
    let delegate_thread = agent.as_ref().and_then(|agent| agent.delegate_thread);
    let link = Arc::new(Link::new(core.token.child_token()));
    // A known agent is installed only by `attach::join`'s synchronized database refresh; using the
    // admission snapshot here could let an early tool call observe stale rights.
    let session = Session::new(Arc::clone(&core), None, Arc::clone(&link));
    let reader = LineLimit::new(reader, core.limits.max_message_bytes);
    let running = match session.clone().serve((reader, rmcp_side)).await {
        Ok(running) => running,
        Err(error) => {
            log::info!("mcp server: the session did not start: {error}");
            drop(controls);
            let _ = tokio::time::timeout(FLUSH_DEADLINE, pump).await;
            return;
        }
    };
    let peer = running.peer().clone();
    let live = attach::join(&core, &session, &peer, &link, known_id, delegate_thread).await;
    let ended = link.close.child_token();
    let registration = waiting.map(|(ticket, previous)| {
        tokio::spawn(registration::register(RegistrationContext {
            core: Arc::clone(&core),
            session: session.clone(),
            peer: peer.clone(),
            controls: controls.clone(),
            ticket,
            previous,
            slug: greeting.slug.clone(),
            ended: ended.clone(),
            link: Arc::clone(&link),
        }))
    });
    let watcher = tokio::spawn(attach::tell_catalog_changes(
        Arc::clone(&core),
        session.clone(),
        peer,
    ));
    // The link's token is a child of the server's: it fires for a revocation, a denial, the
    // vault closing and the access turned off.
    let closing = tokio::select! {
        _ = running.waiting() => None,
        _ = link.close.cancelled() => Some(link.reason().unwrap_or_else(|| core.closing_reason())),
    };
    if let Some(reason) = closing {
        let _ = controls.send(Control::Closing(reason));
    }
    watcher.abort();
    ended.cancel();
    if let Some(registration) = registration {
        if tokio::time::timeout(FLUSH_DEADLINE, registration)
            .await
            .is_err()
        {
            log::warn!("mcp server: registration did not end in time");
        }
    }
    drop(controls);
    let _ = tokio::time::timeout(FLUSH_DEADLINE, pump).await;
    let agent = session.agent().map(|agent| agent.id);
    drop(live);
    if let Some(agent) = agent.filter(|agent| !core.live.connected(*agent)) {
        (core.notify)(Notice::Disconnected(agent));
    }
}

/// Reads the greeting line within `deadline`; `None` for a late, long or invalid one.
async fn read_greeting(
    reader: &mut BufReader<ReadHalf<BoxStream>>,
    deadline: Duration,
) -> Option<Greeting> {
    let mut line = Vec::new();
    let read = async {
        // One byte more than allowed, so a line that is too long is noticed without its end.
        let mut limited = (&mut *reader).take(MAX_GREETING_BYTES as u64 + 1);
        limited.read_until(b'\n', &mut line).await
    };
    match tokio::time::timeout(deadline, read).await {
        Ok(Ok(read)) if read > 0 && line.ends_with(b"\n") => {
            line.pop();
            parse_greeting(&line).ok()
        }
        _ => None,
    }
}

async fn admit(core: &Arc<Core>, greeting: &Greeting) -> Admission {
    // A delegate registers in the chat, never through this dialog (US6).
    if greeting.delegate {
        return match delegates::admit(core, greeting.token.as_deref()).await {
            Ok(agent) => Admission::Known(agent),
            Err(reason) => Admission::Rejected(reason),
        };
    }
    let device = core.device;
    if let Some(credential) = greeting.token.clone() {
        let found = core
            .db
            .read(move |q| registry::find_by_credential(q, device, &credential))
            .await;
        let now = now_millis();
        let mut previous = None;
        match found {
            Ok(Some(agent)) if manage::is_expired(&agent, now) => {
                let id = agent.id;
                let loaded = core
                    .db
                    .write(move |tx| {
                        manage::expire(tx, device, id)?;
                        registry::load_rights(tx, device, id)
                    })
                    .await;
                match loaded {
                    Ok(rights) => {
                        (core.notify)(Notice::Changed);
                        previous = Some(Previous {
                            id,
                            rights,
                            mode: agent.mode,
                        });
                    }
                    Err(error) => {
                        log::warn!("mcp server: could not expire an agent: {error}");
                        return Admission::Rejected(RejectReason::Disabled);
                    }
                }
            }
            Ok(Some(agent)) if agent.status == AgentStatus::Registered => {
                let id = agent.id;
                let (mode, client_name) = (agent.mode, agent.client_name);
                let rights = core
                    .db
                    .write(move |tx| {
                        registry::touch(tx, device, id, now)?;
                        registry::load_rights(tx, device, id)
                    })
                    .await;
                return match rights {
                    Ok(rights) => Admission::Known(SessionAgent {
                        id,
                        rights,
                        mode,
                        client_name,
                        delegate_thread: None,
                    }),
                    Err(error) => {
                        log::warn!("mcp server: could not read an agent's rights: {error}");
                        Admission::Rejected(RejectReason::Disabled)
                    }
                };
            }
            Ok(_) => {}
            Err(error) => {
                log::warn!("mcp server: could not look up a credential: {error}");
                return Admission::Rejected(RejectReason::Disabled);
            }
        }
        if previous.is_some() {
            return pending(core, greeting, previous).await;
        }
    }
    pending(core, greeting, None).await
}

/// The way of an agent holzi does not know (any more): blocked, busy, or a dialog.
async fn pending(core: &Arc<Core>, greeting: &Greeting, previous: Option<Previous>) -> Admission {
    let device = core.device;
    let slug = greeting.slug.clone();
    let blocked = core.declined(&slug)
        || core
            .db
            .read(move |q| registry::is_blocked(q, device, &slug))
            .await
            .unwrap_or(true);
    if blocked {
        return Admission::Rejected(RejectReason::Blocked);
    }
    match core.gate.try_open(&greeting.slug) {
        Some(ticket) => Admission::Pending(ticket, previous),
        None => Admission::Rejected(RejectReason::RegistrationBusy),
    }
}

/// Writes rmcp's lines and the control lines to the helper, a control line first when both wait.
async fn pump(
    from_rmcp: DuplexStream,
    mut to_helper: WriteHalf<BoxStream>,
    mut controls: mpsc::UnboundedReceiver<Control>,
) {
    let mut lines = BufReader::new(from_rmcp);
    let mut line = Vec::new();
    let mut rmcp_open = true;
    let mut controls_open = true;
    while rmcp_open || controls_open {
        tokio::select! {
            biased;
            control = controls.recv(), if controls_open => match control {
                Some(control) => {
                    if to_helper.write_all(control_line(control).as_bytes()).await.is_err() {
                        break;
                    }
                }
                None => controls_open = false,
            },
            // `read_until` keeps a partial line in `line` when the other branch wins, so the line
            // is only cleared once it was written whole.
            read = lines.read_until(b'\n', &mut line), if rmcp_open => match read {
                Ok(0) | Err(_) => rmcp_open = false,
                Ok(_) => {
                    if to_helper.write_all(&line).await.is_err() {
                        break;
                    }
                    line.clear();
                }
            },
        }
    }
    let _ = to_helper.flush().await;
    let _ = to_helper.shutdown().await;
}

/// Ends the read side when a line grows longer than the limit (contracts/agent-helper.md).
struct LineLimit<R> {
    inner: R,
    max: usize,
    current: usize,
}

impl<R> LineLimit<R> {
    fn new(inner: R, max: usize) -> Self {
        Self {
            inner,
            max,
            current: 0,
        }
    }
}

impl<R: AsyncRead + Unpin> AsyncRead for LineLimit<R> {
    fn poll_read(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut ReadBuf<'_>,
    ) -> Poll<std::io::Result<()>> {
        let before = buf.filled().len();
        let polled = Pin::new(&mut self.inner).poll_read(cx, buf);
        if let Poll::Ready(Ok(())) = &polled {
            let max = self.max;
            for byte in &buf.filled()[before..] {
                if *byte == b'\n' {
                    self.current = 0;
                } else {
                    self.current += 1;
                    if self.current > max {
                        return Poll::Ready(Err(std::io::Error::new(
                            std::io::ErrorKind::InvalidData,
                            "an MCP message is too long",
                        )));
                    }
                }
            }
        }
        polled
    }
}

/// Unix milliseconds.
pub fn now_millis() -> i64 {
    i64::try_from(
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_millis())
            .unwrap_or_default(),
    )
    .unwrap_or(i64::MAX)
}
