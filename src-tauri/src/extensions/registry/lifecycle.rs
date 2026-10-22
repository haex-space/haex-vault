//! Following the registry on this device (spec 017, T078, FR-008, FR-037–FR-039, research R11).
//!
//! Installs, updates and removals reach a device as ordinary synced rows. Whenever the registry
//! tables change, from this device or another one, every extension is brought in line here: a new
//! `purge_hlc` clears up ([`super::purge`]); an installed, enabled extension is started
//! ([`super::start::start`]: effective bundle, `transferring` while BLOBs are missing, signature
//! check, the applied migrations kept, pending migrations) and its state on this device written.
//! A disabled one only gets `disabled` as its state. The tables of an extension that does not run
//! here, disabled or removed with "keep data", follow its migrations in the vault all the same, so
//! its synced rows have somewhere to go (FR-008, FR-037). Then parked sync groups whose tables now exist are replayed. holzi's window hears
//! `extension-status-changed` for every changed state, with `reload` when the effective bundle of
//! a running extension changed, so its open tabs load the new one (FR-038).

use std::sync::Arc;

use haex_crdt::rusqlite::params;
use serde::Serialize;
use tauri::{AppHandle, Emitter, Runtime};
use tokio::sync::broadcast::error::{RecvError, TryRecvError};
use ts_rs::TS;
use uuid::Uuid;

use super::purge::{self, Removal};
use super::start::{self as starting, start, Effective, Registration};
use super::status::{self, DeviceStatus};
use crate::error::Result;
use crate::extensions::commands::{ExtensionsChanged, EXTENSIONS_CHANGED};
use crate::extensions::host::ExtensionHost;
use crate::extensions::ids::{device_status_id, ExtensionName, PublicKey, TablePrefix};
use crate::extensions::sql::migrate::{apply_pending, kept_migrations, MigrationError};
use crate::passwords::clock::unix_millis;
use crate::state::AppState;
use crate::storage::query::Query;
use crate::storage::wm_session_commands::current_device_uuid;
use crate::sync::inbound::park;
use crate::vault_gate::VaultDb;

/// Event for holzi's window: an extension's state on this device changed.
pub const EXTENSION_STATUS_CHANGED: &str = "extension-status-changed";

/// Changes to these tables can change what an extension is on this device.
pub const WATCHED: [&str; 5] = [
    "extensions",
    "extension_bundles",
    "extension_bundle_files",
    "extension_blobs",
    "extension_migrations",
];

#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/types/bindings/")]
pub struct ExtensionStatusChanged {
    pub extension_id: String,
    /// `transferring`, `ready`, `signature_failed`, `migration_failed` or `disabled`.
    pub status: String,
    /// The error kind, never data of the extension.
    #[ts(optional)]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    /// The effective bundle of a running extension changed: open tabs reload.
    pub reload: bool,
}

struct Registered {
    id: Uuid,
    prefix: Option<TablePrefix>,
    installed: bool,
    enabled: bool,
    purge_data: bool,
    purge_hlc: Option<String>,
}

/// Every registered extension; a row holzi cannot read is left out.
fn registered(q: &mut impl Query) -> haex_crdt::Result<Vec<Registered>> {
    let rows: Vec<Option<Registered>> = q.query_map(
        "SELECT id, public_key, name, state, enabled, purge_data, purge_hlc FROM extensions",
        &[],
        |r| {
            let (id, key, name): (String, String, String) = (r.get(0)?, r.get(1)?, r.get(2)?);
            let prefix = match (
                PublicKey::parse_case_insensitive(&key),
                ExtensionName::parse(&name),
            ) {
                (Ok(public_key), Ok(name)) => Some(TablePrefix { public_key, name }),
                _ => None,
            };
            Ok(Uuid::parse_str(&id).ok().map(|id| Registered {
                id,
                prefix,
                installed: r
                    .get::<_, String>(3)
                    .is_ok_and(|state| state == "installed"),
                enabled: r.get::<_, i64>(4).is_ok_and(|enabled| enabled != 0),
                purge_data: r.get::<_, i64>(5).is_ok_and(|purge| purge != 0),
                purge_hlc: r.get::<_, Option<String>>(6).ok().flatten(),
            }))
        },
    )?;
    Ok(rows.into_iter().flatten().collect())
}

type Shown = Option<(String, Option<String>, Option<String>)>;

/// The state row of `extension_id` on `device`: status, bundle, error.
fn shown(db: &VaultDb, extension_id: Uuid, device: Uuid) -> Result<Shown> {
    let id = device_status_id(extension_id, device).to_string();
    db.read_blocking(move |q| {
        q.query_row(
            "SELECT status, bundle_id, error FROM extension_device_status WHERE id = ?1",
            params![id],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
        )
    })
}

/// Brings every extension on `device` in line with the registry; returns the states that
/// changed. One extension failing does not stop the others. Blocking.
pub fn reconcile(
    db: &VaultDb,
    host: &ExtensionHost,
    device: Uuid,
    now_ms: i64,
) -> Result<Vec<ExtensionStatusChanged>> {
    let mut changed = Vec::new();
    for extension in db.read_blocking(|q| registered(q))? {
        match follow(db, host, &extension, device, now_ms) {
            Ok(Some(event)) => changed.push(event),
            Ok(None) => {}
            Err(error) => log::warn!("extension {}: following failed: {error}", extension.id),
        }
    }
    if let Err(error) = park::replay_ready(&db.database(), &|| false) {
        log::warn!("extensions: replaying parked sync groups failed: {error}");
    }
    Ok(changed)
}

/// What an extension that does not run here leaves behind in this process: its file and mail
/// watches, shells and channels end, its notifications close and, once removed, its held permission decisions go. This runs on every
/// reconcile, so also when it was disabled or removed on another device and the change came
/// through sync; with nothing left each step does nothing. Its registration is read again, not
/// taken from the list the reconcile began with: enabled or installed again here meanwhile, it
/// keeps what it has (the next reconcile follows that change). A development version has ids of
/// its own (`dev_extension_id`) and is not touched. A failed read leaves everything as it is.
fn stopped_here(db: &VaultDb, host: &ExtensionHost, extension_id: Uuid) {
    let removed = match db.read_blocking(move |q| Ok(starting::registration(q, extension_id))) {
        Ok(Ok(Registration::Enabled)) => return,
        Ok(Ok(Registration::Disabled)) => false,
        Ok(Ok(Registration::Missing)) => true,
        Ok(Err(error)) | Err(error) => {
            log::warn!("extension {extension_id}: its registration could not be read: {error}");
            return;
        }
    };
    host.fs.watches.end_all(extension_id);
    host.mail_watches.end_all(extension_id);
    host.shells.end_all(extension_id);
    host.peers.end_all(extension_id);
    host.notifications.close_all(extension_id);
    if removed {
        host.permissions.forget_extension(extension_id);
    }
}

/// Brings one extension on `device` in line; returns its state when that changed.
fn follow(
    db: &VaultDb,
    host: &ExtensionHost,
    extension: &Registered,
    device: Uuid,
    now_ms: i64,
) -> Result<Option<ExtensionStatusChanged>> {
    if let (Some(prefix), Some(purge_hlc)) = (&extension.prefix, &extension.purge_hlc) {
        let removal = Removal {
            extension_id: extension.id,
            prefix: prefix.clone(),
            purge_data: extension.purge_data,
            purge_hlc: purge_hlc.clone(),
        };
        let check = removal.clone();
        // The tables of that prefix here are a development version's (US12): the clear-up waits
        // until it is unloaded, which reconciles again.
        if db.read_blocking(move |q| purge::due(q, &check))?
            && !starting::dev_prefix_here(db, prefix).unwrap_or(true)
        {
            purge::run(db, &removal, device)?;
            host.forget_effective(extension.id);
        }
    }
    if !extension.installed {
        if !extension.purge_data {
            keep_tables(db, extension, now_ms);
        }
        stopped_here(db, host, extension.id);
        return Ok(None);
    }
    let before = shown(db, extension.id, device)?;
    let started = if extension.enabled {
        start(db, extension.id, device, now_ms).ok()
    } else {
        keep_tables(db, extension, now_ms);
        stopped_here(db, host, extension.id);
        let id = extension.id;
        db.write_blocking(move |tx| {
            status::set(tx, id, device, DeviceStatus::Disabled, None, None, now_ms)
                .map_err(Into::into)
        })?;
        None
    };
    let after = shown(db, extension.id, device)?;
    let mut reload = false;
    if let Some(started) = started {
        let previous = host.note_effective(extension.id, started.bundle_id);
        reload = previous.is_some_and(|previous| previous != started.bundle_id);
        host.remember_started(started);
    }
    Ok(after
        .clone()
        .filter(|_| after != before || reload)
        .map(|(status, _, error)| ExtensionStatusChanged {
            extension_id: extension.id.to_string(),
            status,
            error,
            reload,
        }))
}

/// Applies the pending migrations of an extension that does not run on this device, so its tables
/// follow and rows for them do not wait parked: a disabled one from its verified effective bundle
/// (FR-003), one removed with "keep data" from its kept migration rows (FR-008), since no bundle
/// is left. A failure is logged: rows for its tables wait parked until a later run succeeds.
fn keep_tables(db: &VaultDb, extension: &Registered, now_ms: i64) {
    let id = extension.id;
    // The tables of that prefix here are a development version's (US12, research R16).
    if let Some(prefix) = &extension.prefix {
        if starting::dev_prefix_here(db, prefix).unwrap_or(true) {
            return;
        }
    }
    let result = if extension.installed {
        match starting::effective(db, id) {
            Ok(Effective::Ready(prepared)) => starting::migrate(db, id, &prepared, now_ms),
            Ok(Effective::Transferring(_) | Effective::SignatureFailed(..)) => return,
            Err(error) => {
                log::warn!("extension {id}: its bundle could not be read: {error}");
                return;
            }
        }
    } else {
        let Some(prefix) = &extension.prefix else {
            return;
        };
        db.read_blocking(move |q| Ok(kept_migrations(q, id)))
            .unwrap_or(Err(MigrationError::Unavailable))
            .and_then(|kept| apply_pending(db, id, prefix, &kept, now_ms))
    };
    if let Err(error) = result {
        log::warn!("extension {id}: the migrations of its kept tables failed: {error:?}");
    }
}

/// Whether the vault changes waiting in `changes` touch the registry. Takes every waiting
/// message, so one reconcile answers a burst of writes (each BLOB of a transfer is one).
fn drain_watched(changes: &mut tokio::sync::broadcast::Receiver<Arc<Vec<String>>>) -> bool {
    let mut due = false;
    loop {
        match changes.try_recv() {
            Ok(tables) => due |= touches_registry(&tables),
            Err(TryRecvError::Lagged(_)) => due = true,
            Err(TryRecvError::Empty | TryRecvError::Closed) => return due,
        }
    }
}

fn touches_registry(tables: &[String]) -> bool {
    tables.iter().any(|t| WATCHED.contains(&t.as_str()))
}

/// Follows the registry for the vault session `state` just published, until it ends: once at the
/// start, then after every change to one of the [`WATCHED`] tables. A failure is logged; the
/// extensions then start when a frame opens (Constitution VII: never blocks the session).
pub fn start_for_active_instance<R: Runtime>(app: &AppHandle<R>, state: &AppState) {
    let db = match state.database() {
        Ok(db) => db,
        Err(error) => {
            log::warn!("extensions: no open vault, the registry is not followed: {error}");
            return;
        }
    };
    let device = match current_device_uuid(app, &db) {
        Ok(device) => device,
        Err(error) => {
            log::warn!("extensions: this device is unknown, the registry is not followed: {error}");
            return;
        }
    };
    let mut changes = state.vault_changes().subscribe();
    let host = state.extensions();
    let gate = state.gate().clone();
    let token = gate.token();
    let app = app.clone();
    let follow = async move {
        let mut due = true;
        let mut first = true;
        loop {
            if due {
                let (db, host) = (db.clone(), host.clone());
                let run = gate.spawn_blocking(move || {
                    reconcile(
                        &db,
                        &host,
                        device,
                        unix_millis(std::time::SystemTime::now()),
                    )
                });
                match run {
                    Ok(handle) => match handle.await {
                        Ok(Ok(changed)) => announce(&app, changed, !first),
                        Ok(Err(error)) => log::warn!("extensions: following failed: {error}"),
                        Err(error) => log::warn!("extensions: following stopped: {error}"),
                    },
                    Err(_) => return,
                }
                first = false;
            }
            due = tokio::select! {
                () = token.cancelled() => return,
                received = changes.recv() => match received {
                    Ok(tables) => touches_registry(&tables),
                    Err(RecvError::Lagged(_)) => true,
                    Err(RecvError::Closed) => return,
                },
            };
            due |= drain_watched(&mut changes);
        }
    };
    if let Err(error) = state.gate().spawn(follow) {
        log::warn!("extensions: following the registry did not start: {error}");
    }
}

/// Tells holzi's window what changed; after a registry change the extension list is read again
/// even when no state here changed (a removal on another device).
fn announce<R: Runtime>(
    app: &AppHandle<R>,
    changed: Vec<ExtensionStatusChanged>,
    registry_changed: bool,
) {
    if changed.is_empty() && !registry_changed {
        return;
    }
    let extension_ids = changed.iter().map(|c| c.extension_id.clone()).collect();
    for event in changed {
        if let Err(error) = app.emit(EXTENSION_STATUS_CHANGED, event) {
            log::warn!("{EXTENSION_STATUS_CHANGED} not delivered: {error}");
        }
    }
    if let Err(error) = app.emit(EXTENSIONS_CHANGED, ExtensionsChanged { extension_ids }) {
        log::warn!("{EXTENSIONS_CHANGED} not delivered: {error}");
    }
}

#[cfg(test)]
#[path = "lifecycle_test_support.rs"]
mod test_support;

#[cfg(test)]
#[path = "lifecycle_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "lifecycle_us7_tests.rs"]
mod us7_tests;

#[cfg(test)]
#[path = "lifecycle_us12_tests.rs"]
mod us12_tests;

#[cfg(test)]
#[path = "lifecycle_migration_tests.rs"]
mod migration_tests;
