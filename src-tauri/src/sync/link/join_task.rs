//! The new installation's link in the running app (spec 024, FR-023 to
//! FR-025, research R11): creates its own vault, announces itself at the
//! code's rendezvous, accepts the main device's connection and runs
//! [`crate::sync::link::join`] on the stream. There is no vault session
//! yet, so this runs on its own, next to the start page.
//!
//! The vault it creates stays only when the link finished; on every other
//! end it is removed, and nothing of the main device stays with it.

use std::net::SocketAddr;
use std::sync::{Arc, Mutex};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use iroh::endpoint::{presets, Connection};
use iroh::{Endpoint, RelayMode, SecretKey};
use nostr_sdk::client::Client;
use tauri::{AppHandle, Runtime};
use tokio::sync::Notify;
use tokio_util::sync::CancellationToken;

use crate::error::HaexError;
use crate::instances::link_vault::LinkVault;
use crate::instances::passphrase::Passphrase;
use crate::sync::handshake::local_schema;
use crate::sync::link::code::{CodeError, LinkCode};
use crate::sync::link::error::LinkError;
use crate::sync::link::join::{self, Join, Stage};
use crate::sync::link::meeting::{self, LinkMeeting};
use crate::sync::link::status::{LinkFailure, LinkJoinState};
use crate::sync::link::wire::AbortReason;
use crate::sync::replica::Replica;
use crate::sync::wire::LINK_ALPN;

/// How long to wait for a main device to answer the meeting.
const SEARCH_TIMEOUT: Duration = Duration::from_secs(90);
/// How often the meeting is renewed while searching.
const MEETING_INTERVAL: Duration = Duration::from_secs(5);
/// How long the first meeting waits for the endpoint's relay (spec 043): the host dials the first
/// meeting it sees, and without a relay in it a device behind a NAT cannot be reached.
const ONLINE_TIMEOUT: Duration = Duration::from_secs(3);
/// How long the endpoint may take to bind.
const BIND_TIMEOUT: Duration = Duration::from_secs(15);
/// How long the relays get to connect before the search goes on without them.
const RELAY_CONNECT_TIMEOUT: Duration = Duration::from_secs(5);
/// How long the host may take to open its stream after connecting.
const STREAM_TIMEOUT: Duration = Duration::from_secs(30);
/// How long to wait for the host to close after `Done`, so the message is
/// not lost to an early close.
const CLOSE_TIMEOUT: Duration = Duration::from_secs(10);

/// Where a join looks for its main device and how it is reachable itself.
#[derive(Debug, Clone)]
pub struct JoinConfig {
    pub nostr_relays: Vec<String>,
    pub relay_mode: RelayMode,
    /// Bind only this address; tests use loopback.
    pub bind_addr: Option<SocketAddr>,
    /// How long to wait for a main device to answer.
    pub search_timeout: Duration,
}

impl JoinConfig {
    /// The built-in servers: a new installation has no settings yet.
    pub fn production() -> Self {
        Self::with_servers(crate::sync::servers::ServerConfig::default())
    }

    /// The given servers, besides the built-in ones unless switched off: for a
    /// vault whose devices use servers of their own, which a new installation
    /// cannot read from a vault it does not have yet.
    pub fn with_servers(servers: crate::sync::servers::ServerConfig) -> Self {
        Self {
            nostr_relays: servers.effective_nostr_relays(),
            relay_mode: servers.relay_mode(),
            bind_addr: crate::sync::endpoint::app_bind_addr(),
            search_timeout: SEARCH_TIMEOUT,
        }
    }
}

/// What the user typed on the start page.
pub struct JoinArgs {
    pub code: String,
    pub vault_name: String,
    pub device_name: String,
    pub passphrase: Passphrase,
}

/// Tells the frontend the join changed.
pub type Emit = Arc<dyn Fn(&LinkJoinState) + Send + Sync>;

struct Running {
    state: LinkJoinState,
    cancel: CancellationToken,
    done: Arc<Notify>,
}

/// The join of this installation; one at a time.
#[derive(Clone, Default)]
pub struct LinkJoin {
    slot: Arc<Mutex<Option<Running>>>,
}

impl LinkJoin {
    /// The state of the current or last join, `None` if there was none.
    pub fn status(&self) -> Option<LinkJoinState> {
        self.slot().as_ref().map(|r| r.state.clone())
    }

    /// Starts a join. Input the user can fix (name taken, passphrase too
    /// short) comes back as an error; everything after that is reported
    /// through `emit` and [`LinkJoin::status`].
    pub async fn start<R: Runtime>(
        &self,
        app: &AppHandle<R>,
        emit: Emit,
        args: JoinArgs,
        config: JoinConfig,
    ) -> Result<LinkJoinState, HaexError> {
        if self.slot().as_ref().is_some_and(|r| !is_over(&r.state)) {
            return Err(HaexError::InvalidInput {
                reason: "a link is already in progress".into(),
            });
        }
        let code = match LinkCode::parse(&args.code) {
            Ok(code) => code,
            Err(CodeError::Length | CodeError::Character) => {
                let state = LinkJoinState::Failed {
                    reason: LinkFailure::WrongCode,
                };
                self.set(
                    &emit,
                    CancellationToken::new(),
                    Arc::new(Notify::new()),
                    state.clone(),
                );
                return Ok(state);
            }
        };
        let cancel = CancellationToken::new();
        let done = Arc::new(Notify::new());
        let state = LinkJoinState::Searching;
        self.set(&emit, cancel.clone(), done.clone(), state.clone());
        let vault = match LinkVault::create(
            app,
            &args.vault_name,
            &args.device_name,
            args.passphrase,
        )
        .await
        {
            Ok(vault) => vault,
            Err(error) => {
                self.clear_if(&done);
                return Err(error);
            }
        };
        if cancel.is_cancelled() {
            vault.discard();
            let state = LinkJoinState::Failed {
                reason: LinkFailure::ConnectionLost,
            };
            self.set(&emit, cancel, done, state.clone());
            return Ok(state);
        }

        let this = self.clone();
        tokio::spawn(async move {
            let outcome = drive(&this, &emit, &cancel, &vault, &code, &config).await;
            let name = vault.name.clone();
            let end = match outcome {
                Ok(()) if cancel.is_cancelled() => {
                    vault.discard();
                    LinkJoinState::Failed {
                        reason: LinkFailure::ConnectionLost,
                    }
                }
                Ok(()) => match vault.keep() {
                    Ok(()) => LinkJoinState::Done { vault_name: name },
                    Err(error) => {
                        log::warn!("sync: the linked vault could not be kept: {error}");
                        LinkJoinState::Failed {
                            reason: LinkFailure::ConnectionLost,
                        }
                    }
                },
                Err(failure) => {
                    vault.discard();
                    LinkJoinState::Failed { reason: failure }
                }
            };
            this.set(&emit, cancel, done, end);
        });
        Ok(state)
    }

    /// Cancels a join in progress; its vault is removed.
    pub async fn cancel(&self) {
        let Some((cancel, done)) = self
            .slot()
            .as_ref()
            .filter(|running| !is_over(&running.state))
            .map(|running| (running.cancel.clone(), Arc::clone(&running.done)))
        else {
            return;
        };
        cancel.cancel();
        loop {
            let notified = done.notified();
            let still_running = self.slot().as_ref().is_some_and(|running| {
                Arc::ptr_eq(&running.done, &done) && !is_over(&running.state)
            });
            if !still_running {
                return;
            }
            notified.await;
        }
    }

    fn slot(&self) -> std::sync::MutexGuard<'_, Option<Running>> {
        self.slot.lock().unwrap_or_else(|e| e.into_inner())
    }

    fn set(&self, emit: &Emit, cancel: CancellationToken, done: Arc<Notify>, state: LinkJoinState) {
        let terminal = is_over(&state);
        *self.slot() = Some(Running {
            state: state.clone(),
            cancel,
            done: Arc::clone(&done),
        });
        emit(&state);
        if terminal {
            done.notify_one();
        }
    }

    fn clear_if(&self, done: &Arc<Notify>) {
        let mut slot = self.slot();
        if slot
            .as_ref()
            .is_some_and(|running| Arc::ptr_eq(&running.done, done))
        {
            slot.take();
            done.notify_one();
        }
    }

    fn update(&self, emit: &Emit, state: LinkJoinState) {
        if let Some(running) = self.slot().as_mut() {
            running.state = state.clone();
        }
        emit(&state);
    }
}

fn is_over(state: &LinkJoinState) -> bool {
    matches!(
        state,
        LinkJoinState::Done { .. } | LinkJoinState::Failed { .. }
    )
}

/// Search, exchange and the wait for the host's close; the error says why
/// the link did not finish.
async fn drive(
    this: &LinkJoin,
    emit: &Emit,
    cancel: &CancellationToken,
    vault: &LinkVault,
    code: &LinkCode,
    config: &JoinConfig,
) -> Result<(), LinkFailure> {
    let endpoint = bind(vault, config).await.map_err(|error| {
        log::warn!("sync: the link endpoint did not bind: {error}");
        LinkFailure::ConnectionLost
    })?;
    let result = exchange(this, emit, cancel, vault, code, config, &endpoint).await;
    endpoint.close().await;
    result
}

async fn exchange(
    this: &LinkJoin,
    emit: &Emit,
    cancel: &CancellationToken,
    vault: &LinkVault,
    code: &LinkCode,
    config: &JoinConfig,
    endpoint: &Endpoint,
) -> Result<(), LinkFailure> {
    let (_, rv_pk) = code.rendezvous_keys().map_err(|_| LinkFailure::WrongCode)?;
    crate::sync::presence::ensure_crypto_provider();
    let client = Client::new();
    for url in &config.nostr_relays {
        if let Err(error) = client.add_relay(url.as_str()).await {
            log::warn!("sync: link relay {url} is not a valid URL: {error}");
        }
    }
    client.connect().and_wait(RELAY_CONNECT_TIMEOUT).await;
    // The host dials the first meeting it sees. A phone on mobile data, or the emulator of the e2e
    // suite (spec 043), is reachable only through its relay, so the meeting waits a moment for it;
    // without a reachable relay it goes out with the direct addresses alone.
    if !matches!(config.relay_mode, RelayMode::Disabled) {
        let _ = tokio::time::timeout(ONLINE_TIMEOUT, endpoint.online()).await;
    }

    let announce = async {
        loop {
            match meeting::build(&vault.keys, &LinkMeeting::new(&endpoint.addr()), &rv_pk) {
                Ok(event) => {
                    if let Err(error) = client.send_event(&event).await {
                        log::debug!("sync: the link meeting was not sent yet: {error}");
                    }
                }
                Err(error) => log::warn!("sync: the link meeting could not be built: {error}"),
            }
            tokio::time::sleep(MEETING_INTERVAL).await;
        }
    };
    let accept = async {
        loop {
            let incoming = endpoint.accept().await?;
            // The endpoint only speaks the link protocol, so nothing else
            // can arrive.
            if let Ok(connection) = incoming.await {
                return Some(connection);
            }
        }
    };
    let connection = tokio::select! {
        biased;
        _ = cancel.cancelled() => None,
        _ = tokio::time::sleep(config.search_timeout) => {
            client.shutdown().await;
            return Err(LinkFailure::Expired);
        }
        _ = announce => None,
        connection = accept => connection,
    };
    client.shutdown().await;
    let connection = connection.ok_or(LinkFailure::ConnectionLost)?;

    let replica = Arc::new(Replica::new(Arc::clone(&vault.db)));
    let joining = Join {
        replica: &replica,
        keys: &vault.keys,
        vault_device_uuid: vault.db.device_id(),
        code,
        name: &device_name(&replica, vault),
        schema: local_schema(),
        now_ms: now_ms(),
    };
    let on_stage = |stage: Stage| {
        let state = match stage {
            Stage::WaitingForConfirmation => LinkJoinState::WaitingForConfirmation,
            Stage::Transferring { pages } => LinkJoinState::Transferring {
                progress: u32::try_from(pages).unwrap_or(u32::MAX),
            },
        };
        this.update(emit, state);
    };
    let remote = *connection.remote_id().as_bytes();
    let run = async {
        let (mut send, mut recv) = tokio::time::timeout(STREAM_TIMEOUT, connection.accept_bi())
            .await
            .map_err(|_| LinkError::TimedOut)?
            .map_err(|_| LinkError::TimedOut)?;
        join::run(&mut send, &mut recv, remote, &joining, on_stage).await
    };
    let result = tokio::select! {
        biased;
        _ = cancel.cancelled() => Err(LinkError::Aborted(AbortReason::Cancelled)),
        result = run => result,
    };
    match result {
        Ok(_) => {
            wait_for_close(&connection, cancel).await;
            Ok(())
        }
        Err(error) => {
            log::info!("sync: joining ended without a link: {error}");
            connection.close(0u32.into(), b"ended");
            Err(failure_of(&error))
        }
    }
}

/// The name this installation gave itself, read back from its alias.
fn device_name(replica: &Replica, vault: &LinkVault) -> String {
    let installation = vault.keys.device_pubkey;
    let _ = installation;
    crate::storage::query::read(replica.db(), |r| {
        use crate::storage::query::Query;
        let alias: Option<Option<String>> = r.query_row(
            "SELECT alias FROM known_devices WHERE vault_device_uuid = ?1",
            haex_crdt::rusqlite::params![vault.db.device_id().to_string()],
            |row| row.get(0),
        )?;
        Ok(alias.flatten())
    })
    .ok()
    .flatten()
    .filter(|alias| !alias.trim().is_empty())
    .unwrap_or_else(|| "HaexVault".to_string())
}

/// Gives the host the chance to finish: it closes after publishing.
async fn wait_for_close(connection: &Connection, cancel: &CancellationToken) {
    tokio::select! {
        _ = cancel.cancelled() => connection.close(0u32.into(), b"cancelled"),
        _ = tokio::time::timeout(CLOSE_TIMEOUT, connection.closed()) => {}
    }
}

/// What the user is told for a failed link.
fn failure_of(error: &LinkError) -> LinkFailure {
    match error {
        LinkError::BadProof => LinkFailure::WrongCode,
        LinkError::Declined | LinkError::Aborted(AbortReason::Rejected) => LinkFailure::Rejected,
        LinkError::Incompatible | LinkError::Aborted(AbortReason::Incompatible) => {
            LinkFailure::Incompatible
        }
        _ => LinkFailure::ConnectionLost,
    }
}

async fn bind(vault: &LinkVault, config: &JoinConfig) -> Result<Endpoint, String> {
    let mut builder = Endpoint::builder(presets::Minimal)
        .secret_key(SecretKey::from_bytes(&vault.keys.endpoint_secret))
        .alpns(vec![LINK_ALPN.to_vec()])
        .relay_mode(config.relay_mode.clone());
    if let Some(addr) = config.bind_addr {
        builder = builder
            .clear_ip_transports()
            .bind_addr(addr)
            .map_err(|e| e.to_string())?;
    }
    tokio::time::timeout(BIND_TIMEOUT, builder.bind())
        .await
        .map_err(|_| "binding timed out".to_string())?
        .map_err(|e| e.to_string())
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}
