//! The main device's link in the running app (spec 024, FR-023, FR-024,
//! research R11): shows a code, waits for the new installation to announce
//! itself at the code's rendezvous, dials it, and runs
//! [`crate::sync::link::host`] on the stream, with the user's decision
//! coming from a command.
//!
//! One code is live at a time; a new one replaces the old. The code ends
//! when it is used (the new installation's meeting arrived), cancelled, out
//! of time, or with the vault session.

use std::pin::Pin;
use std::sync::{Arc, Mutex};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use futures::{FutureExt, Stream, StreamExt};
use nostr::event::Kind;
use nostr::filter::Filter;
use nostr_sdk::client::{Client, ClientNotification};
use tokio::sync::oneshot;
use tokio_util::sync::CancellationToken;

use crate::error::HolziError;
use crate::storage::query;
use crate::sync::handshake::local_schema;
use crate::sync::keys;
use crate::sync::link::code::{LinkCode, CODE_LIFETIME};
use crate::sync::link::error::LinkError;
use crate::sync::link::host::{self, Decision, Host, Outcome};
use crate::sync::link::meeting;
use crate::sync::link::status::{LinkCodeInfo, LinkingStage, LinkingStatus};
use crate::sync::link::text;
use crate::sync::registry::SyncRuntime;
use crate::sync::servers;

/// How long the dial of the new installation may take.
const DIAL_TIMEOUT: Duration = Duration::from_secs(20);
/// How long one dial may take before a newer meeting is tried (spec 043): the first meeting can
/// lack the relay a device behind a NAT is reachable through, and the new installation sends a new
/// meeting every few seconds.
const DIAL_ATTEMPT: Duration = Duration::from_secs(6);
/// How long a proven new device may wait for the user's answer.
const DECISION_TIMEOUT: Duration = Duration::from_secs(5 * 60);
/// How long the new device gets to read the last message before this side
/// closes the connection.
const CLOSE_GRACE: Duration = Duration::from_secs(3);
/// How long the relays get to connect before the wait goes on without them.
const RELAY_CONNECT_TIMEOUT: Duration = Duration::from_secs(5);
/// The gift-wrap kind of a meeting (contracts/nostr-events.md).
const GIFT_WRAP_KIND: Kind = Kind::Custom(21059);

/// What the frontend is told whenever the link changes.
pub type Emit = Arc<dyn Fn(Option<LinkingStatus>) + Send + Sync>;

/// Where the live code stands.
enum Slot {
    Idle,
    CodeShown {
        id: u64,
        cancel: CancellationToken,
    },
    Awaiting {
        id: u64,
        name: String,
        decide: Option<oneshot::Sender<Decision>>,
        cancel: CancellationToken,
    },
}

struct Shared {
    slot: Mutex<Slot>,
    next_id: Mutex<u64>,
    emit: Emit,
}

/// The link state of a main device.
#[derive(Clone)]
pub struct LinkHost {
    shared: Arc<Shared>,
}

impl LinkHost {
    /// Creates an idle host that reports link status changes through `emit`.
    pub fn new(emit: Emit) -> Self {
        Self {
            shared: Arc::new(Shared {
                slot: Mutex::new(Slot::Idle),
                next_id: Mutex::new(0),
                emit,
            }),
        }
    }

    /// The link in progress, for `sync_status`.
    pub fn status(&self) -> Option<LinkingStatus> {
        match &*self.shared.slot() {
            Slot::Idle => None,
            Slot::CodeShown { .. } => Some(LinkingStatus {
                stage: LinkingStage::CodeShown,
                new_device_name: None,
            }),
            Slot::Awaiting { name, .. } => Some(LinkingStatus {
                stage: LinkingStage::AwaitingConfirmation,
                new_device_name: Some(name.clone()),
            }),
        }
    }

    /// Shows a new code and starts waiting for the new installation. Only a
    /// main device may (FR-024); the backend checks, not only the interface. The text of the
    /// code carries `vault_name` and the vault's servers (spec 085).
    pub async fn create_code(
        &self,
        runtime: &Arc<SyncRuntime>,
        vault_name: &str,
    ) -> Result<LinkCodeInfo, HolziError> {
        let replica = Arc::clone(&runtime.replica);
        let (is_main, config) = tokio::task::spawn_blocking(move || {
            query::read(replica.db(), |r| {
                Ok((keys::vault_secret(r)?.is_some(), servers::read(r)?))
            })
        })
        .await
        .map_err(|e| HolziError::CrdtInit {
            reason: format!("link task: {e}"),
        })??;
        if !is_main {
            return Err(HolziError::NotMainDevice);
        }

        let code = LinkCode::generate();
        let expires_at = now_ms() + CODE_LIFETIME.as_millis() as u64;
        let cancel = runtime.cancel.child_token();
        let id = {
            let mut next = self
                .shared
                .next_id
                .lock()
                .unwrap_or_else(|e| e.into_inner());
            *next += 1;
            *next
        };
        self.shared.replace(Slot::CodeShown {
            id,
            cancel: cancel.clone(),
        });
        self.shared.announce();
        let link_text = text::build(&code, vault_name, &config);
        let info = LinkCodeInfo {
            code: code.display(),
            qr_svg: text::qr_svg(&link_text),
            text: link_text,
            expires_at,
        };
        let (runtime, shared) = (Arc::clone(runtime), Arc::clone(&self.shared));
        tokio::spawn(async move {
            let result = drive(&runtime, &shared, id, &code, &cancel, expires_at).await;
            match result {
                Ok(Outcome::Linked { name, .. }) => {
                    log::info!("sync: linked a new device ({name})")
                }
                Ok(Outcome::Rejected) => log::info!("sync: a new device was declined"),
                Err(error) => log::info!("sync: the link ended without linking: {error}"),
            }
            shared.finish(id);
        });
        Ok(info)
    }

    /// Ends the live code, if any.
    pub fn cancel(&self) {
        self.shared.replace(Slot::Idle);
        self.shared.announce();
    }

    /// The user's yes for the new device, as a main device or not.
    pub fn confirm(&self, as_main: bool) -> Result<(), HolziError> {
        self.decide(Decision::Accept { as_main })
    }

    /// The user's no.
    pub fn reject(&self) -> Result<(), HolziError> {
        self.decide(Decision::Reject)
    }

    /// Consumes the pending decision sender and attempts to send the user's answer.
    /// Returns `InvalidInput` if no device awaits an answer or one was already sent.
    fn decide(&self, decision: Decision) -> Result<(), HolziError> {
        let mut slot = self.shared.slot();
        match &mut *slot {
            Slot::Awaiting { decide, .. } => match decide.take() {
                Some(tx) => {
                    let _ = tx.send(decision);
                    Ok(())
                }
                None => Err(HolziError::InvalidInput {
                    reason: "the link was decided already".into(),
                }),
            },
            _ => Err(HolziError::InvalidInput {
                reason: "no new device waits for an answer".into(),
            }),
        }
    }
}

impl Shared {
    /// Locks the live code slot, recovering its contents if the mutex is poisoned.
    fn slot(&self) -> std::sync::MutexGuard<'_, Slot> {
        self.slot.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// Puts `next` in the slot, cancelling the code it replaces.
    fn replace(&self, next: Slot) {
        let old = std::mem::replace(&mut *self.slot(), next);
        match old {
            Slot::CodeShown { cancel, .. } | Slot::Awaiting { cancel, .. } => cancel.cancel(),
            Slot::Idle => {}
        }
    }

    /// Emits the current link status, or `None` when the host is idle.
    fn announce(&self) {
        let status = match &*self.slot() {
            Slot::Idle => None,
            Slot::CodeShown { .. } => Some(LinkingStatus {
                stage: LinkingStage::CodeShown,
                new_device_name: None,
            }),
            Slot::Awaiting { name, .. } => Some(LinkingStatus {
                stage: LinkingStage::AwaitingConfirmation,
                new_device_name: Some(name.clone()),
            }),
        };
        (self.emit)(status);
    }

    /// Back to idle when `id` is still the live code.
    fn finish(&self, id: u64) {
        let mut slot = self.slot();
        let mine = matches!(
            &*slot,
            Slot::CodeShown { id: live, .. } | Slot::Awaiting { id: live, .. } if *live == id
        );
        if !mine {
            return;
        }
        *slot = Slot::Idle;
        drop(slot);
        self.announce();
    }

    /// Records and announces a device awaiting approval if `id` is still shown.
    fn awaiting(&self, id: u64, name: String, decide: oneshot::Sender<Decision>) {
        let mut slot = self.slot();
        if let Slot::CodeShown { id: live, cancel } = &*slot {
            if *live == id {
                let cancel = cancel.clone();
                *slot = Slot::Awaiting {
                    id,
                    name,
                    decide: Some(decide),
                    cancel,
                };
                drop(slot);
                self.announce();
            }
        }
    }
}

/// Everything one code does, from waiting for the meeting to the end of the
/// exchange.
async fn drive(
    runtime: &Arc<SyncRuntime>,
    shared: &Arc<Shared>,
    id: u64,
    code: &LinkCode,
    cancel: &CancellationToken,
    expires_at: u64,
) -> Result<Outcome, LinkError> {
    let mut rendezvous = Rendezvous::open(runtime, code).await?;
    let addr = tokio::select! {
        biased;
        _ = cancel.cancelled() => return Err(LinkError::Aborted(crate::sync::link::wire::AbortReason::Cancelled)),
        found = rendezvous.first(expires_at) => found?,
    };
    // The code counts as used from here on.
    let dialed = dial(runtime, &mut rendezvous, addr).await;
    rendezvous.close().await;
    let connection = dialed?;
    let remote = *connection.remote_id().as_bytes();
    let (mut send, mut recv) = connection
        .open_bi()
        .await
        .map_err(|e| LinkError::List(e.to_string()))?;

    let (decide, answer) = oneshot::channel();
    let on_new_device = {
        let shared = Arc::clone(shared);
        move |new: host::NewDevice| shared.awaiting(id, new.name, decide)
    };
    let decision = async {
        tokio::select! {
            answer = answer => answer.unwrap_or(Decision::Reject),
            _ = cancel.cancelled() => Decision::Reject,
            _ = tokio::time::sleep(DECISION_TIMEOUT) => Decision::Reject,
        }
    };
    let host = Host {
        replica: &runtime.replica,
        keys: &runtime.keys,
        vault: runtime.vault,
        code,
        schema: local_schema(),
        now_ms: now_ms(),
    };
    let outcome = host::run(&mut send, &mut recv, remote, &host, on_new_device, decision).await;
    let _ = send.finish();
    if matches!(outcome, Ok(Outcome::Linked { .. })) {
        // The list changed behind `VaultDb`, so tell the session and
        // presence the way a commit would (spec 024, FR-007).
        (runtime.wake)();
        runtime.node.announce_devices_changed();
    } else {
        // The last thing sent (a decline, an abort) must arrive before the
        // connection closes: closing at once can drop it. The new device
        // closes when it has read it.
        let _ = tokio::time::timeout(CLOSE_GRACE, connection.closed()).await;
    }
    connection.close(0u32.into(), b"done");
    outcome
}

/// Dials the new installation at `addr`; when a dial does not get through, at the newest meeting
/// that arrived meanwhile, until [`DIAL_TIMEOUT`].
async fn dial(
    runtime: &Arc<SyncRuntime>,
    rendezvous: &mut Rendezvous,
    mut addr: iroh::EndpointAddr,
) -> Result<iroh::endpoint::Connection, LinkError> {
    let deadline = tokio::time::Instant::now() + DIAL_TIMEOUT;
    loop {
        let attempt =
            DIAL_ATTEMPT.min(deadline.saturating_duration_since(tokio::time::Instant::now()));
        match tokio::time::timeout(attempt, runtime.node.dial_link(addr.clone())).await {
            Ok(Ok(connection)) => return Ok(connection),
            Ok(Err(error)) if tokio::time::Instant::now() >= deadline => {
                return Err(LinkError::List(error.to_string()))
            }
            Ok(Err(error)) => {
                log::info!("sync: dialing the new device failed, trying again: {error}");
                tokio::time::sleep(Duration::from_millis(500)).await;
            }
            Err(_) if tokio::time::Instant::now() >= deadline => return Err(LinkError::TimedOut),
            Err(_) => log::info!("sync: dialing the new device takes long, trying again"),
        }
        if let Some(newer) = rendezvous.latest() {
            addr = newer;
        }
    }
}

/// The code's rendezvous: where the new installation's meetings arrive.
struct Rendezvous {
    client: Client,
    notifications: Pin<Box<dyn Stream<Item = ClientNotification> + Send>>,
    rv_sk: nostr::key::SecretKey,
}

impl Rendezvous {
    async fn open(runtime: &Arc<SyncRuntime>, code: &LinkCode) -> Result<Self, LinkError> {
        let (rv_sk, rv_pk) = code
            .rendezvous_keys()
            .map_err(|e| LinkError::List(e.to_string()))?;
        crate::sync::presence::ensure_crypto_provider();
        let client = Client::new();
        for url in &runtime.nostr_relays {
            if let Err(error) = client.add_relay(url.as_str()).await {
                log::warn!("sync: link relay {url} is not a valid URL: {error}");
            }
        }
        client.connect().and_wait(RELAY_CONNECT_TIMEOUT).await;
        let notifications = client.notifications();
        client
            .subscribe(Filter::new().kind(GIFT_WRAP_KIND).pubkey(rv_pk))
            .await
            .map_err(|e| LinkError::List(e.to_string()))?;
        Ok(Self {
            client,
            notifications,
            rv_sk,
        })
    }

    /// Where the new installation can be reached, from a received event; `None` for anything else.
    fn meeting(&self, notification: ClientNotification) -> Option<iroh::EndpointAddr> {
        let ClientNotification::Event { event, .. } = notification else {
            return None;
        };
        let (_device, seen) = meeting::open(&event, &self.rv_sk).ok()?;
        if !seen.is_fresh(now_ms()) {
            return None;
        }
        let addr = seen.endpoint_addr().ok()?;
        log::info!(
            "sync: link meeting with {} direct addresses, relay: {}",
            addr.ip_addrs().count(),
            addr.relay_urls().next().is_some()
        );
        Some(addr)
    }

    /// Waits for the first fresh meeting, until the code expires.
    async fn first(&mut self, expires_at: u64) -> Result<iroh::EndpointAddr, LinkError> {
        let remaining = Duration::from_millis(expires_at.saturating_sub(now_ms()));
        let found = tokio::time::timeout(remaining, async {
            while let Some(notification) = self.notifications.next().await {
                if let Some(addr) = self.meeting(notification) {
                    return Some(addr);
                }
            }
            None
        })
        .await;
        match found {
            Ok(Some(addr)) => Ok(addr),
            _ => Err(LinkError::TimedOut),
        }
    }

    /// The newest meeting that arrived since the last look, without waiting.
    fn latest(&mut self) -> Option<iroh::EndpointAddr> {
        let mut newest = None;
        while let Some(Some(notification)) = self.notifications.next().now_or_never() {
            if let Some(addr) = self.meeting(notification) {
                newest = Some(addr);
            }
        }
        newest
    }

    async fn close(self) {
        self.client.shutdown().await;
    }
}

/// Returns milliseconds since the Unix epoch, or zero if the clock precedes it.
fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}
