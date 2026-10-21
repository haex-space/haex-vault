//! What a running bridge shares between its connections (spec 049, research R4, R10): the server
//! keys, the vault, the decisions of this session, the open connections per client and the
//! approval questions, at most one per client (FR-007).

use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use tokio::sync::{mpsc, Semaphore};
use tokio_util::sync::CancellationToken;
use uuid::Uuid;

use super::approvals::{
    self, validate, ClientIdentity, Decision, SessionApproval, SessionDecisions, Status,
    StoredClient,
};
use super::declaration::Declared;
use super::envelope::ServerKeys;
use super::prompts::{
    ApprovalAnswer, ApprovalQuestion, BridgeAnswer, BridgeClientRef, BridgeDeclaredGrant,
    BridgePrompts, BridgeQuestion,
};
use crate::prompt_park::PromptEnd;
use crate::vault_gate::VaultDb;
use crate::Result;

/// The bounds of the bridge (research R1, R4); tests shorten the deadline.
#[derive(Debug, Clone, Copy)]
pub struct Limits {
    pub handshake_deadline: Duration,
    pub max_connections: usize,
    pub max_message_bytes: usize,
    pub max_in_flight: usize,
}

impl Default for Limits {
    fn default() -> Self {
        Self {
            handshake_deadline: Duration::from_secs(10),
            max_connections: 16,
            max_message_bytes: 64 * 1024,
            max_in_flight: 4,
        }
    }
}

/// What the bridge tells the window besides its questions.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Notice {
    /// A question ended without an answer; the window closes it.
    PromptCancelled(String),
    /// A client was approved, blocked, connected or disconnected; the settings reload.
    Changed,
}

pub type Notifier = Arc<dyn Fn(Notice) + Send + Sync>;

/// What a connection learns from outside its own socket.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Control {
    /// The user approved the client (FR-011).
    Authorized,
    /// The user declined or revoked the client: say so, then close (FR-011).
    Denied,
}

struct ConnEntry {
    client_id: Option<String>,
    control: mpsc::UnboundedSender<Control>,
}

#[derive(Default)]
struct Hub {
    next_id: u64,
    connections: HashMap<u64, ConnEntry>,
    /// Clients with an open approval question.
    asking: HashSet<String>,
}

pub struct BridgeCore {
    pub(super) keys: ServerKeys,
    pub(super) db: VaultDb,
    pub(super) device: Uuid,
    pub(super) token: CancellationToken,
    pub(super) prompts: BridgePrompts,
    pub(super) limits: Limits,
    pub(super) slots: Arc<Semaphore>,
    notify: Notifier,
    session: Mutex<SessionDecisions>,
    hub: Mutex<Hub>,
}

impl BridgeCore {
    pub(super) fn new(
        db: VaultDb,
        device: Uuid,
        token: CancellationToken,
        prompts: BridgePrompts,
        notify: Notifier,
        limits: Limits,
    ) -> Self {
        Self {
            keys: ServerKeys::generate(),
            db,
            device,
            token,
            prompts,
            slots: Arc::new(Semaphore::new(limits.max_connections)),
            limits,
            notify,
            session: Mutex::new(SessionDecisions::default()),
            hub: Mutex::new(Hub::default()),
        }
    }

    pub(super) fn notify(&self, notice: Notice) {
        (self.notify)(notice);
    }

    pub(super) fn session(&self) -> MutexGuard<'_, SessionDecisions> {
        self.session.lock().unwrap_or_else(PoisonError::into_inner)
    }

    fn hub(&self) -> MutexGuard<'_, Hub> {
        self.hub.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// Registers a connection; its id and the channel it hears [`Control`] on.
    pub(super) fn register(&self) -> (u64, mpsc::UnboundedReceiver<Control>) {
        let (control, receiver) = mpsc::unbounded_channel();
        let mut hub = self.hub();
        hub.next_id += 1;
        let id = hub.next_id;
        hub.connections.insert(
            id,
            ConnEntry {
                client_id: None,
                control,
            },
        );
        (id, receiver)
    }

    /// Ties a connection to the client its handshake named.
    pub(super) fn bind(&self, conn: u64, client_id: &str) {
        if let Some(entry) = self.hub().connections.get_mut(&conn) {
            entry.client_id = Some(client_id.to_owned());
        }
        self.notify(Notice::Changed);
    }

    pub(super) fn unregister(&self, conn: u64) {
        let bound = self
            .hub()
            .connections
            .remove(&conn)
            .is_some_and(|entry| entry.client_id.is_some());
        if bound {
            self.notify(Notice::Changed);
        }
    }

    /// Whether a client has an open connection.
    pub fn is_connected(&self, client_id: &str) -> bool {
        self.hub()
            .connections
            .values()
            .any(|entry| entry.client_id.as_deref() == Some(client_id))
    }

    /// Tells every connection of a client.
    pub(super) fn broadcast(&self, client_id: &str, control: Control) {
        for entry in self.hub().connections.values() {
            if entry.client_id.as_deref() == Some(client_id) {
                let _ = entry.control.send(control);
            }
        }
    }

    /// The decision for a handshake or a request, from the vault and this session.
    pub(super) async fn decide(&self, identity: &ClientIdentity) -> Result<Decision> {
        let device = self.device;
        let client_id = identity.client_id.clone();
        let stored = self
            .db
            .read(move |q| approvals::load(q, device, &client_id))
            .await?;
        Ok(approvals::decide(
            identity,
            stored.as_ref(),
            &self.session(),
        ))
    }

    /// Notes the connection of a remembered client.
    pub(super) async fn touch(&self, client_id: &str) {
        let device = self.device;
        let client_id = client_id.to_owned();
        if let Err(error) = self
            .db
            .write(move |tx| approvals::touch(tx, device, &client_id, now_millis()))
            .await
        {
            log::warn!("browser bridge: could not note the connection: {error}");
        }
    }

    /// Asks the user about a client, unless a question for it is already open (FR-007). The
    /// answer reaches every connection of the client.
    pub(super) fn request_approval(self: &Arc<Self>, identity: ClientIdentity, declared: Declared) {
        if !self.hub().asking.insert(identity.client_id.clone()) {
            return;
        }
        let core = Arc::clone(self);
        tokio::spawn(async move {
            let client_id = identity.client_id.clone();
            let (access, scope_tags) = declared.proposal();
            let question = BridgeQuestion::Approval(ApprovalQuestion {
                client: BridgeClientRef {
                    client_id: identity.client_id.clone(),
                    client_name: identity.client_name.clone(),
                    origin: identity.origin.clone(),
                },
                declared: BridgeDeclaredGrant { access, scope_tags },
                declares_write: declared.declares_write(),
            });
            let mut prompt_id = String::new();
            let answer = core
                .prompts
                .ask(&question, &core.token, |id| prompt_id = id.to_owned())
                .await;
            let control = match answer {
                Ok(BridgeAnswer::Approval(answer)) => {
                    core.apply_approval(&identity, &declared, answer).await
                }
                // An answer of another kind to this question counts as declining once.
                Ok(_) => Control::Denied,
                Err(end) => {
                    if end != PromptEnd::NoWindow {
                        core.notify(Notice::PromptCancelled(prompt_id));
                    }
                    Control::Denied
                }
            };
            core.hub().asking.remove(&client_id);
            core.broadcast(&client_id, control);
            core.notify(Notice::Changed);
        });
    }

    /// Stores the user's answer; an invalid approval counts as declining once.
    async fn apply_approval(
        &self,
        identity: &ClientIdentity,
        declared: &Declared,
        answer: ApprovalAnswer,
    ) -> Control {
        let now = now_millis();
        let status = if answer.allow {
            let Some(access) = answer.access else {
                return Control::Denied;
            };
            match validate(declared, access, answer.scope_tags, answer.default_tag) {
                Ok(approval) => Status::Allowed(approval),
                Err(error) => {
                    log::warn!("browser bridge: approval refused: {error}");
                    return Control::Denied;
                }
            }
        } else {
            Status::Blocked
        };
        let control = match status {
            Status::Allowed(_) => Control::Authorized,
            Status::Blocked => Control::Denied,
        };
        if answer.remember {
            self.session().forget(&identity.client_id);
            let client = StoredClient {
                identity: identity.clone(),
                last_seen_at: (control == Control::Authorized).then_some(now),
                status,
                decided_at: now,
            };
            let device = self.device;
            if let Err(error) = self
                .db
                .write(move |tx| approvals::save(tx, device, &client))
                .await
            {
                log::warn!("browser bridge: could not store the decision: {error}");
                return Control::Denied;
            }
        } else {
            match status {
                Status::Allowed(approval) => self.session().allow(SessionApproval {
                    identity: identity.clone(),
                    approval,
                    decided_at: now,
                }),
                Status::Blocked => self.session().block(identity.clone(), now),
            }
        }
        control
    }
}

/// Unix milliseconds.
pub fn now_millis() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| i64::try_from(d.as_millis()).unwrap_or(i64::MAX))
        .unwrap_or(0)
}
