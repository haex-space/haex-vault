//! One connection from the `--mcp-agent` helper (spec 021 US1, US8, contracts/agent-helper.md): the
//! greeting decides whether the agent is known, waits for a registration, or is turned away; then
//! rmcp serves the session while a pump slips control lines (the new credential, the reason for
//! closing) between its messages. A registration runs next to the session, so the agent can finish
//! `initialize` while the user decides.

use std::pin::Pin;
use std::sync::Arc;
use std::task::{Context, Poll};
use std::time::Duration;

use super::endpoint::BoxStream;
use super::grants::{self, default_rights};
use super::hello::{
    control_line, json_line, parse_greeting, CloseReason, Control, Greeting, HelloReply,
    RejectReason, MAX_GREETING_BYTES,
};
use super::prompts::{
    GateTicket, PromptClosed, PromptClosedReason, RegistrationDecision, RegistrationQuestion,
};
use super::registry::{self, AgentKind, AgentMode, AgentStatus, Credential, NewAgent, Transport};
use super::server::{Core, Notice};
use super::session::{Session, SessionAgent};
use crate::prompt_park::PromptEnd;
use rmcp::service::{Peer, RoleServer};
use rmcp::ServiceExt;
use tokio::io::{
    AsyncBufReadExt, AsyncRead, AsyncReadExt, AsyncWriteExt, BufReader, DuplexStream, ReadBuf,
    ReadHalf, WriteHalf,
};
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;

/// How long the pump may take to write the last lines after the session ended.
const FLUSH_DEADLINE: Duration = Duration::from_secs(1);

/// How a greeting is admitted.
enum Admission {
    Known(SessionAgent),
    Pending(GateTicket),
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
        Admission::Pending(_) => HelloReply::Pending,
        Admission::Rejected(reason) => HelloReply::Rejected { reason: *reason },
    };
    if write_half
        .write_all(json_line(&reply).as_bytes())
        .await
        .is_err()
    {
        return;
    }
    let (agent, ticket) = match admission {
        Admission::Known(agent) => (Some(agent), None),
        Admission::Pending(ticket) => (None, Some(ticket)),
        Admission::Rejected(_) => return,
    };
    run_session(core, greeting, agent, ticket, reader, write_half).await;
}

async fn run_session(
    core: Arc<Core>,
    greeting: Greeting,
    agent: Option<SessionAgent>,
    ticket: Option<GateTicket>,
    reader: BufReader<ReadHalf<BoxStream>>,
    write_half: WriteHalf<BoxStream>,
) {
    let (controls, control_rx) = mpsc::unbounded_channel();
    let (rmcp_side, pump_side) = tokio::io::duplex(64 * 1024);
    let pump = tokio::spawn(pump(pump_side, write_half, control_rx));
    let session = Session::new(Arc::clone(&core), agent);
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
    // Declining ends the session; its own token, so the server's token stays for the others.
    let declined = CancellationToken::new();
    let ended = core.token.child_token();
    let registration = ticket.map(|ticket| {
        tokio::spawn(register(RegistrationContext {
            core: Arc::clone(&core),
            session: session.clone(),
            peer: peer.clone(),
            controls: controls.clone(),
            ticket,
            slug: greeting.slug.clone(),
            ended: ended.clone(),
            declined: declined.clone(),
        }))
    });
    let watcher = tokio::spawn(tell_catalog_changes(
        Arc::clone(&core),
        session.clone(),
        peer,
    ));
    let closing = tokio::select! {
        _ = running.waiting() => None,
        _ = core.token.cancelled() => Some(core.closing_reason()),
        _ = declined.cancelled() => Some(CloseReason::Denied),
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
    let device = core.device;
    if let Some(credential) = greeting.token.clone() {
        let found = core
            .db
            .read(move |q| registry::find_by_credential(q, device, &credential))
            .await;
        match found {
            Ok(Some(agent)) if agent.status == AgentStatus::Registered => {
                let id = agent.id;
                let now = now_millis();
                let rights = core
                    .db
                    .write(move |tx| {
                        registry::touch(tx, device, id, now)?;
                        registry::load_rights(tx, device, id)
                    })
                    .await;
                return match rights {
                    Ok(rights) => Admission::Known(SessionAgent { id, rights }),
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
    }
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
        Some(ticket) => Admission::Pending(ticket),
        None => Admission::Rejected(RejectReason::RegistrationBusy),
    }
}

/// Inputs shared by the registration task.
struct RegistrationContext {
    core: Arc<Core>,
    session: Session,
    peer: Peer<RoleServer>,
    controls: mpsc::UnboundedSender<Control>,
    ticket: GateTicket,
    slug: String,
    ended: CancellationToken,
    declined: CancellationToken,
}

/// Asks the window once the agent introduced itself, and acts on the answer.
async fn register(context: RegistrationContext) {
    let RegistrationContext {
        core,
        session,
        peer,
        controls,
        ticket,
        slug,
        ended,
        declined,
    } = context;
    let _ticket = ticket;
    let mut initialized = session.initialized();
    // The guard `wait_for` returns must not live across the awaits below.
    let introduced = matches!(
        tokio::time::timeout(
            core.limits.initialize_deadline,
            initialized.wait_for(|done| *done),
        )
        .await,
        Ok(Ok(_))
    );
    if !introduced {
        return;
    }
    let (client_name, client_version) = match peer.peer_info() {
        Some(info) => (
            registry::kept_name(&info.client_info.name),
            Some(registry::kept_name(&info.client_info.version)),
        ),
        None => (slug.clone(), None),
    };
    let question = RegistrationQuestion {
        slug: slug.clone(),
        client_name: client_name.clone(),
        client_version: client_version.clone(),
        transport: Transport::Ipc,
        rights: default_rights(),
        mode: AgentMode::default(),
    };
    let mut request_id = String::new();
    let answer = core
        .prompts
        .ask(&question, &ended, |id| request_id = id.to_owned())
        .await;
    let reason = match &answer {
        Ok(_) => PromptClosedReason::Answered,
        Err(PromptEnd::Expired) => PromptClosedReason::Timeout,
        Err(_) => PromptClosedReason::Cancelled,
    };
    (core.notify)(Notice::PromptClosed(PromptClosed { request_id, reason }));
    let answer = match answer {
        Ok(answer) => answer,
        // No window, no answer in time: as if declined once (US1 scenario 4).
        Err(PromptEnd::Expired | PromptEnd::NoWindow) => {
            core.decline(&slug);
            declined.cancel();
            return;
        }
        Err(PromptEnd::Cancelled) => return,
    };
    let device = core.device;
    if answer.decision == RegistrationDecision::Deny || grants::validate(&answer.rights).is_err() {
        let remember = answer.decision == RegistrationDecision::Deny && answer.remember;
        if remember {
            let now = now_millis();
            let blocked_slug = slug.clone();
            let blocked_name = client_name.clone();
            if let Err(error) = core
                .db
                .write(move |tx| registry::block(tx, device, &blocked_slug, &blocked_name, now))
                .await
            {
                log::warn!("mcp server: could not remember a block: {error}");
            }
        }
        core.decline(&slug);
        (core.notify)(Notice::Changed);
        declined.cancel();
        return;
    }
    let credential = match Credential::generate() {
        Ok(credential) => credential,
        Err(error) => {
            log::warn!("mcp server: no random source for a credential: {error}");
            declined.cancel();
            return;
        }
    };
    let digest = registry::hash(credential.as_str());
    let new = NewAgent {
        kind: AgentKind::External,
        slug,
        client_name,
        client_version,
        transport: Transport::Ipc,
        mode: answer.mode,
    };
    let rights = answer.rights.clone();
    let now = now_millis();
    let stored = core
        .db
        .write(move |tx| registry::register(tx, device, &new, &digest, &rights, now))
        .await;
    let agent = match stored {
        Ok(agent) => agent,
        Err(error) => {
            log::warn!("mcp server: could not store a registration: {error}");
            declined.cancel();
            return;
        }
    };
    session.set_agent(SessionAgent {
        id: agent.id,
        rights: answer.rights,
    });
    // The credential before the new tool list, so the helper has saved it when the agent asks.
    let _ = controls.send(Control::Token(credential.into_string()));
    let _ = peer.notify_tool_list_changed().await;
    (core.notify)(Notice::Changed);
}

/// Tells a registered agent when the window pushed a new action list (FR-017).
async fn tell_catalog_changes(core: Arc<Core>, session: Session, peer: Peer<RoleServer>) {
    let mut versions = core.catalog.subscribe();
    while versions.changed().await.is_ok() {
        if session.agent().is_some() && peer.notify_tool_list_changed().await.is_err() {
            break;
        }
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
