//! The sync service of an open vault (spec 024, FR-031, FR-032).
//!
//! One service runs per vault session, as tracked work of the vault gate: it
//! ends when the close starts, within the drain limits of spec 013. It binds
//! this device's iroh endpoint ([`SyncNode`]) once, then wakes on every
//! committed `VaultDb` write (`VaultGate::sync_notify`) to tell already
//! connected devices this device's progress may have moved; wake-ups that
//! arrive close together collapse into one round. Presence (T037) and
//! reconnect (T039) run alongside the same wake loop.
//!
//! A relay that cannot be reached, or an endpoint that fails to bind, never
//! blocks the vault open or the close (Constitution VII, FR-031): both are
//! logged and this session simply does not sync.

use std::sync::Arc;

use iroh::RelayMode;
use tauri::{AppHandle, Manager, Runtime};
use tokio::sync::Notify;
use tokio_util::sync::CancellationToken;
use tokio_util::task::task_tracker::TaskTrackerToken;

use crate::error::{HolziError, Result};
use crate::files::peer::connect::Connections;
use crate::instances::paths::get_app_local_data;
use crate::state::AppState;
use crate::sync::endpoint::{NodeConfig, SyncNode};
use crate::sync::events::{self, LINK_HOST_STATE_CHANGED, SYNC_DEVICES_CHANGED};
use crate::sync::keys::{self, DeviceKeys};
use crate::sync::link::host_task::LinkHost;
use crate::sync::registry::{SyncRegistry, SyncRuntime};
use crate::sync::replica::Replica;
use crate::sync::resume::Wakeups;
use crate::sync::service_protocols::{
    file_protocols, files_deps, peer_protocols, FilesDeps, PeerDeps,
};
use crate::vault_gate::VaultGate;

/// Everything [`SyncService::start`] needs, already resolved by the caller:
/// keys and the vault pubkey come from genesis, which already ran when the
/// vault opened (spec 024); the relay mode comes from the settings
/// ([`crate::sync::servers`], T038). Generic over `R: Runtime` like
/// [`events::emit`], so a test can pass `tauri::test::MockRuntime`'s handle.
pub struct SyncDeps<R: Runtime> {
    pub replica: Arc<Replica>,
    pub keys: DeviceKeys,
    pub vault: [u8; 32],
    pub relay_mode: RelayMode,
    /// Nostr relays presence connects to (T037).
    pub nostr_relays: Vec<String>,
    /// [`crate::sync::endpoint::app_bind_addr`] in the app (every interface);
    /// tests pin this to loopback, like [`crate::sync::endpoint`]'s own tests do.
    pub bind_addr: Option<std::net::SocketAddr>,
    pub app: AppHandle<R>,
    /// What the channels between extensions need (spec 057); `None` runs the sync alone.
    pub peers: Option<PeerDeps>,
    /// What serving files to other devices needs (spec 056); `None` serves none.
    pub files: Option<FilesDeps>,
}

/// How often a session checks for devices presence knows an address for but
/// this session has not connected to yet (spec 024, FR-010).
const RECONNECT_INTERVAL: std::time::Duration = std::time::Duration::from_secs(30);

/// The least time between two reconnect passes. The end of a session wakes
/// reconnect, and a peer that refuses this device (as a duplicate while it
/// still holds the session of this device's earlier process) ends every new
/// session at once: without a gap that dials it again and again, about
/// fifty times a second.
const RECONNECT_MIN_GAP: std::time::Duration = std::time::Duration::from_secs(2);

/// Keeps reconnect passes [`RECONNECT_MIN_GAP`] apart. The first wake runs at
/// once; wakes during the gap are left to the pass at its end (a [`Notify`]
/// keeps at most one of them).
#[derive(Default)]
struct ReconnectPace {
    last: Option<tokio::time::Instant>,
}

impl ReconnectPace {
    /// Waits until a pass may start, and counts it as started.
    async fn ready(&mut self) {
        if let Some(last) = self.last {
            tokio::time::sleep_until(last + RECONNECT_MIN_GAP).await;
        }
        self.last = Some(tokio::time::Instant::now());
    }
}

/// Handle to the running service. Currently a marker: nothing outside this
/// module needs to reach the bound [`SyncNode`] yet (a later spec-024 story
/// adds `sync_status`).
#[derive(Clone)]
pub struct SyncService;

impl SyncService {
    /// Starts the service as tracked session work of `gate`.
    pub fn start<R: Runtime>(gate: &VaultGate, deps: SyncDeps<R>) -> Result<Self> {
        let notify = gate.sync_notify();
        let token = gate.token();
        let session = gate.tracker_token()?;
        gate.spawn(run(notify, token, session, deps))?;
        Ok(Self)
    }
}

/// Resolves [`SyncDeps`] for the instance `state` just published, and starts
/// the service on `gate`. Called right after `open_instance`/`create_instance`
/// publish the active instance (spec 024); a failure at any step is logged
/// and this vault session simply does not sync (Constitution VII, FR-031).
pub async fn start_for_active_instance<R: Runtime>(app: &AppHandle<R>, state: &AppState) {
    // Idempotent: a second vault session in the process finds it managed.
    let _ = app.manage(Arc::new(SyncRegistry::default()));
    let deps = match resolve_deps(app, state).await {
        Ok(deps) => deps,
        Err(error) => {
            log::warn!("sync: could not resolve sync state, this session will not sync: {error}");
            return;
        }
    };
    if let Err(error) = SyncService::start(state.gate(), deps) {
        log::warn!("sync: the sync service did not start: {error}");
    }
}

/// Reads the device keys, vault identity and server preferences genesis
/// left in the open vault, off the async runtime.
async fn resolve_deps<R: Runtime>(app: &AppHandle<R>, state: &AppState) -> Result<SyncDeps<R>> {
    let db = state.database()?.database();
    let app_local_data = get_app_local_data(app)?;
    let installation_id_file = crate::identity::installation_id_path(&app_local_data);
    let installation_uuid = crate::identity::read_or_mint_installation_uuid(&installation_id_file)?;
    let for_read = Arc::clone(&db);
    let (device_keys, vault, relay_mode, nostr_relays, device) =
        tauri::async_runtime::spawn_blocking(move || {
            crate::storage::query::read(&for_read, |r| {
                let device_keys =
                    keys::load_device_keys(r, installation_uuid)?.ok_or_else(|| {
                        haex_crdt::Error::consumer("sync: no device keys after genesis")
                    })?;
                let vault = keys::vault_pubkey(r)?.ok_or_else(|| {
                    haex_crdt::Error::consumer("sync: no vault identity after genesis")
                })?;
                let servers = crate::sync::servers::read(r)?;
                let device =
                    crate::storage::known_devices::get_vault_device_uuid(r, installation_uuid)?;
                Ok((
                    device_keys,
                    vault,
                    servers.relay_mode(),
                    servers.effective_nostr_relays(),
                    device,
                ))
            })
        })
        .await
        .map_err(|e| HolziError::CrdtInit {
            reason: format!("sync deps join: {e}"),
        })?
        .map_err(HolziError::from)?;
    Ok(SyncDeps {
        replica: Arc::new(Replica::tracked(db, state.gate().clone())),
        keys: device_keys,
        vault,
        relay_mode,
        nostr_relays,
        bind_addr: crate::sync::endpoint::app_bind_addr(),
        app: app.clone(),
        peers: Some(PeerDeps {
            host: state.extensions(),
            db: state.database()?,
        }),
        files: files_deps(app, state, device)?,
    })
}

/// The service body: binds the endpoint, then runs the commit-notify,
/// presence and reconnect loops until `token` is cancelled, and shuts the
/// endpoint down.
async fn run<R: Runtime>(
    notify: Arc<Notify>,
    token: CancellationToken,
    session: TaskTrackerToken,
    deps: SyncDeps<R>,
) {
    let replica = Arc::clone(&deps.replica);
    let presence_keys = deps.keys.clone();
    let vault = deps.vault;
    let nostr_relays = deps.nostr_relays;
    let bind_addr = deps.bind_addr;
    let devices_app = deps.app.clone();
    let link_app = deps.app.clone();
    let registry = deps
        .app
        .try_state::<Arc<SyncRegistry>>()
        .map(|state| Arc::clone(&*state));
    // Presence also wants to know when a pulled device list changed the content key, so it
    // subscribes to the new mailbox at once instead of on its next tick.
    let (changed_tx, changed_rx) = tokio::sync::watch::channel(0u64);
    let changed_tx = Arc::new(changed_tx);
    let on_applied = applied_event_sink(
        deps.app,
        Arc::clone(&replica),
        presence_keys.clone(),
        vault,
        Arc::clone(&changed_tx),
    );
    let config = NodeConfig {
        relay_mode: deps.relay_mode,
        bind_addr,
        session: Some(session),
    };
    let peer_host = deps.peers.as_ref().map(|peers| Arc::clone(&peers.host));
    let (protocols, files) = file_protocols(
        peer_protocols(deps.peers, &deps.keys, vault, &devices_app),
        deps.files,
        &deps.keys,
        vault,
    );
    let node = match SyncNode::bind_with(
        deps.replica,
        deps.keys,
        deps.vault,
        config,
        on_applied,
        protocols,
    )
    .await
    {
        Ok(node) => node,
        Err(error) => {
            log::warn!("sync: the endpoint did not bind, this session will not sync: {error}");
            token.cancelled().await;
            return;
        }
    };

    let node = Arc::new(node);
    node.on_devices_changed(Arc::new(move || {
        events::emit(&devices_app, SYNC_DEVICES_CHANGED, ());
    }));
    // Presence, the end of a session and the app coming back (spec 043) wake reconnect, so a
    // device that just appeared or came back is dialed without waiting out the tick.
    let session_cancel = token.child_token();
    let wakeups = Wakeups::new(session_cancel.clone());
    let reconnect_now = Arc::clone(&wakeups.reconnect);
    let reconnect_on_end = Arc::clone(&reconnect_now);
    node.on_session_ended(Arc::new(move || reconnect_on_end.notify_one()));

    // Commands reach the running node through the registry (linking a
    // device); it is cleared again when this service ends.
    let runtime = Arc::new(SyncRuntime {
        node: Arc::clone(&node),
        replica: Arc::clone(&replica),
        keys: presence_keys.clone(),
        vault,
        nostr_relays: nostr_relays.clone(),
        cancel: session_cancel,
        link: LinkHost::new(Arc::new(move |status| {
            events::emit(&link_app, LINK_HOST_STATE_CHANGED, status);
        })),
        wake: {
            let (node, changed_tx) = (Arc::clone(&node), Arc::clone(&changed_tx));
            Arc::new(move || {
                node.local_changed();
                changed_tx.send_modify(|n| *n = n.wrapping_add(1));
            })
        },
        wakeups: wakeups.clone(),
        files: files.map(|ctx| Arc::new(Connections::new(Arc::clone(&node), ctx))),
    });
    if let Some(registry) = &registry {
        registry.set(Arc::clone(&runtime));
    }
    // Cleared on every way out, an aborted drain included: a runtime left in the registry holds
    // the node, and with it the gate's tracker, so the drain would end as stuck.
    let registered = ClearOnDrop(registry);
    finish_pending_links(&replica, &presence_keys, vault).await;
    // `notify` (the gate's shared commit signal) wakes at most one waiter
    // per commit, so it gets exactly one consumer here; fanning that out to
    // presence (which also wants to know about a local commit, e.g. a
    // device-list change this session just issued) goes through the
    // `watch` channel instead.
    let notify_loop = async {
        loop {
            notify.notified().await;
            node.local_changed();
            changed_tx.send_modify(|n| *n = n.wrapping_add(1));
        }
    };
    let presence_loop = crate::sync::presence_loop::run(
        &node,
        &replica,
        &presence_keys,
        vault,
        nostr_relays,
        changed_rx,
        &wakeups,
    );
    // iroh does not see Android's network changes by itself (research R5).
    let network_loop = async {
        loop {
            wakeups.network.notified().await;
            node.network_change().await;
        }
    };
    let reconnect_loop = async {
        let mut tick = tokio::time::interval(RECONNECT_INTERVAL);
        tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
        let mut pace = ReconnectPace::default();
        loop {
            tokio::select! {
                _ = tick.tick() => {}
                _ = reconnect_now.notified() => {}
            }
            pace.ready().await;
            crate::sync::reconnect_missing(&node, &replica);
        }
    };

    tokio::select! {
        biased;
        _ = token.cancelled() => {}
        _ = notify_loop => {}
        _ = presence_loop => {}
        _ = reconnect_loop => {}
        _ = network_loop => {}
    }
    runtime.cancel.cancel();
    drop(registered);
    node.shutdown().await;
    // Spec 057: nothing of the session's channels outlives it.
    if let Some(host) = peer_host {
        host.peers.clear();
    }
}

/// Clears the registry when dropped.
struct ClearOnDrop(Option<Arc<SyncRegistry>>);

impl Drop for ClearOnDrop {
    fn drop(&mut self) {
        if let Some(registry) = &self.0 {
            registry.clear();
        }
    }
}

/// Finishes links this device began before it last stopped: a main device
/// publishes a list whose new device already confirmed, a new installation
/// drops its record once the list names it. Best-effort, and idempotent.
async fn finish_pending_links(replica: &Arc<Replica>, keys: &DeviceKeys, vault: [u8; 32]) {
    // Counted with the vault's close like the follow-ups in `applied_event_sink`: an aborted
    // drain drops this future, but not the blocking thread, which goes on writing. Skipped once
    // the close has started.
    let Ok(held) = replica.hold() else {
        return;
    };
    let (replica, keys) = (Arc::clone(replica), keys.clone());
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| i64::try_from(d.as_millis()).unwrap_or(i64::MAX))
        .unwrap_or(0);
    let result = tokio::task::spawn_blocking(move || {
        // Moved in after the token, so they drop before it: the close waits until the database
        // reference is gone.
        let _held = held;
        let (replica, keys) = (replica, keys);
        crate::sync::link::host::finish_pending(&replica, &keys, vault, now)?;
        crate::sync::link::join::finish_pending(&replica, now)?;
        crate::sync::admission::sweep_now(&replica, u64::try_from(now).unwrap_or(0))?;
        Ok::<_, haex_crdt::Error>(())
    })
    .await;
    match result {
        Ok(Ok(())) => {}
        Ok(Err(error)) => log::warn!("sync: finishing pending links failed: {error}"),
        Err(error) => log::warn!("sync: finishing pending links did not run: {error}"),
    }
}

/// Builds the closure [`SyncNode::bind`] calls after applying a pull: finishes
/// join records when a remote device-list update is evidence of host
/// publication and tells the device view when the device list changed. Announcing the changed
/// data to the frontend is not its job: `crate::vault_events` does that for every writer
/// (FR-032).
fn applied_event_sink<R: Runtime>(
    app: AppHandle<R>,
    replica: Arc<Replica>,
    keys: DeviceKeys,
    vault: [u8; 32],
    changed: Arc<tokio::sync::watch::Sender<u64>>,
) -> Arc<dyn Fn(std::collections::BTreeSet<String>) + Send + Sync> {
    Arc::new(move |tables| {
        // The writes below are counted with the vault's close (`Replica::hold`), taken before their
        // task starts so the drain cannot end in between; once the close has started they are
        // skipped.
        if tables.contains("device_lists") {
            changed.send_modify(|n| *n = n.wrapping_add(1));
        }
        if let Some(Ok(held)) = tables.contains("device_lists").then(|| replica.hold()) {
            let replica = Arc::clone(&replica);
            let keys = keys.clone();
            let now = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| i64::try_from(d.as_millis()).unwrap_or(i64::MAX))
                .unwrap_or(0);
            tokio::spawn(async move {
                match tokio::task::spawn_blocking(move || {
                    let _held = held;
                    let (replica, keys) = (replica, keys);
                    crate::sync::link::host::drop_listed(&replica, vault)?;
                    crate::sync::link::join::finish_pending_after_host_publication(
                        &replica, &keys, vault, now,
                    )
                })
                .await
                {
                    Ok(Ok(_)) => {}
                    Ok(Err(error)) => {
                        log::warn!("sync: finishing published join links failed: {error}")
                    }
                    Err(error) => {
                        log::warn!("sync: finishing published join links did not run: {error}")
                    }
                }
            });
        }
        // Requests to join and the list they are measured against meet here: the same merged set
        // ends in the same state on every device (R20).
        let sweep = tables.contains("device_lists") || tables.contains("admission_requests");
        if let Some(Ok(held)) = sweep.then(|| replica.hold()) {
            let replica = Arc::clone(&replica);
            tokio::spawn(async move {
                let now = u64::try_from(
                    std::time::SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH)
                        .map(|d| d.as_millis())
                        .unwrap_or(0),
                )
                .unwrap_or(0);
                match tokio::task::spawn_blocking(move || {
                    let _held = held;
                    let replica = replica;
                    crate::sync::admission::sweep_now(&replica, now)
                })
                .await
                {
                    Ok(Ok(_)) => {}
                    Ok(Err(error)) => {
                        log::warn!("sync: sweeping admission requests failed: {error}")
                    }
                    Err(error) => {
                        log::warn!("sync: sweeping admission requests did not run: {error}")
                    }
                }
            });
        }
        // A name, the device list, a request to join or who belongs to the vault changed: the
        // device view reloads.
        if tables.contains("known_devices")
            || tables.contains("device_lists")
            || tables.contains("admission_requests")
        {
            events::emit(&app, SYNC_DEVICES_CHANGED, ());
        }
    })
}

#[cfg(test)]
#[path = "service_tests.rs"]
mod tests;
