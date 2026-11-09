//! What the user chose for the agent browser belongs to the vault (spec 084 FR-009, ADR 0001):
//! the tracker blocking is one vault-wide row of `preferences`, the DNS over HTTPS choice one row
//! of this device. Both are JSON, so they sync as every preference does (the DNS row only for the
//! device that wrote it) and a vault that moves to another computer takes its choices along.
//!
//! The values are validated on the way in and out: a row that cannot be read or does not pass
//! validation counts as absent, which means the defaults.
//!
//! Before spec 084 both choices were files of this device (`filter-lists/settings.json`,
//! `agent-browser/dns.json`). [`import_legacy`] writes them into the first vault that is opened
//! without a row of its own and then removes the file, so no choice is lost by the update.

use std::path::Path;
use std::sync::Arc;

use haex_crdt::CrdtTransaction;
use serde::de::DeserializeOwned;
use tauri::{AppHandle, Manager, Runtime};
use tokio::sync::broadcast::error::RecvError;
use uuid::Uuid;

use super::dns::DnsSettings;
use super::filter_lists::{FilterLists, TrackerLists};
use super::filter_settings::TrackerSettings;
use crate::error::HolziError;
use crate::state::AppState;
use crate::storage::preferences::{self, PrefScope};
use crate::storage::query::Query;
use crate::vault_gate::VaultDb;

/// The vault-wide row of the tracker blocking.
pub const KEY_TRACKERS: &str = "agentBrowser.trackers";
/// The row of the DNS choice of one device.
pub const KEY_DNS: &str = "agentBrowser.dns";

/// The file of the tracker choice before spec 084, in the folder of the filter lists.
const LEGACY_TRACKERS_FILE: &str = "settings.json";
/// The file of the DNS choice before spec 084, in the agent browser's folder of the device.
const LEGACY_DNS_FILE: &str = "dns.json";

fn parse<T: DeserializeOwned>(text: &str, validated: impl FnOnce(T) -> Option<T>) -> Option<T> {
    serde_json::from_str::<T>(text).ok().and_then(validated)
}

fn trackers_of(text: &str) -> Option<TrackerSettings> {
    parse(text, |settings: TrackerSettings| settings.validated().ok())
}

fn dns_of(text: &str) -> Option<DnsSettings> {
    parse(text, |settings: DnsSettings| settings.validated().ok())
}

/// The tracker choice of the vault; `None` while it has none (or an unusable one).
pub fn read_trackers(q: &mut impl Query) -> haex_crdt::Result<Option<TrackerSettings>> {
    Ok(preferences::get(q, PrefScope::Vault, KEY_TRACKERS)?
        .as_deref()
        .and_then(trackers_of))
}

/// Keeps the tracker choice in the vault. The caller has validated it.
pub fn write_trackers(
    tx: &mut CrdtTransaction<'_>,
    settings: &TrackerSettings,
) -> haex_crdt::Result<()> {
    let text = serde_json::to_string(settings)
        .map_err(|error| haex_crdt::Error::consumer(error.to_string()))?;
    preferences::insert_or_update(tx, PrefScope::Vault, KEY_TRACKERS, &text)?;
    Ok(())
}

/// The DNS choice of `device`; `None` while it has none (or an unusable one).
pub fn read_dns(q: &mut impl Query, device: Uuid) -> haex_crdt::Result<Option<DnsSettings>> {
    Ok(preferences::get(q, PrefScope::Device(device), KEY_DNS)?
        .as_deref()
        .and_then(dns_of))
}

/// Keeps the DNS choice of `device` in the vault. The caller has validated it.
pub fn write_dns(
    tx: &mut CrdtTransaction<'_>,
    device: Uuid,
    settings: &DnsSettings,
) -> haex_crdt::Result<()> {
    let text = serde_json::to_string(settings)
        .map_err(|error| haex_crdt::Error::consumer(error.to_string()))?;
    preferences::insert_or_update(tx, PrefScope::Device(device), KEY_DNS, &text)?;
    Ok(())
}

/// Keeps a new tracker choice in the vault and applies it at once (`settings.agentBrowser.
/// trackers.set`). Checked first: a choice that is not valid changes nothing. Answers the choice as
/// it counts.
pub async fn set_trackers(
    lists: &Arc<FilterLists>,
    db: &VaultDb,
    settings: TrackerSettings,
) -> crate::error::Result<TrackerSettings> {
    let settings = settings
        .validated()
        .map_err(|reason| HolziError::InvalidInput { reason })?;
    let kept = settings.clone();
    db.write(move |tx| write_trackers(tx, &kept)).await?;
    lists
        .apply(settings)
        .await
        .map_err(|reason| HolziError::InvalidInput { reason })
}

/// The DNS choice of this device in the open vault; the default while there is none.
pub async fn dns(db: &VaultDb) -> crate::error::Result<DnsSettings> {
    let device = db.device_id();
    Ok(db
        .read(move |q| read_dns(q, device))
        .await?
        .unwrap_or_default())
}

/// Keeps a new DNS choice of this device in the vault (`settings.agentBrowser.dns.set`). Checked
/// first; answers the choice as it counts.
pub async fn set_dns(db: &VaultDb, settings: DnsSettings) -> crate::error::Result<DnsSettings> {
    let settings = settings
        .validated()
        .map_err(|reason| HolziError::InvalidInput { reason })?;
    let (device, kept) = (db.device_id(), settings.clone());
    db.write(move |tx| write_dns(tx, device, &kept)).await?;
    Ok(settings)
}

/// Which legacy files went into the vault.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct Imported {
    pub trackers: bool,
    pub dns: bool,
}

fn read_legacy<T>(file: &Path, of: impl FnOnce(&str) -> Option<T>) -> Option<T> {
    std::fs::read_to_string(file).ok().as_deref().and_then(of)
}

/// Writes the choices of the legacy files into `db` where it has no row of its own, and removes
/// the files it took. A vault that has a row never reads them (its file stays for another vault);
/// a file that cannot be read stays untouched.
pub async fn import_legacy(
    db: &VaultDb,
    device: Uuid,
    trackers_file: &Path,
    dns_file: &Path,
) -> crate::error::Result<Imported> {
    let trackers = read_legacy(trackers_file, trackers_of);
    let dns = read_legacy(dns_file, dns_of);
    if trackers.is_none() && dns.is_none() {
        return Ok(Imported::default());
    }
    let imported = db
        .write(move |tx| {
            let mut imported = Imported::default();
            if let Some(settings) = trackers {
                if read_trackers(tx)?.is_none() {
                    write_trackers(tx, &settings)?;
                    imported.trackers = true;
                }
            }
            if let Some(settings) = dns {
                if read_dns(tx, device)?.is_none() {
                    write_dns(tx, device, &settings)?;
                    imported.dns = true;
                }
            }
            Ok(imported)
        })
        .await?;
    for (took, file) in [(imported.trackers, trackers_file), (imported.dns, dns_file)] {
        if took {
            if let Err(error) = std::fs::remove_file(file) {
                log::warn!("agent browser: could not remove {file:?} after its import: {error}");
            }
        }
    }
    Ok(imported)
}

/// Makes the choices of the vault that was just opened, created or imported the ones that count:
/// the legacy files are imported once, and the tracker engine takes the vault's tracker choice
/// (the defaults while it has none). The DNS choice is read when the browser starts.
pub async fn apply_vault_choices<R: Runtime>(app: &AppHandle<R>, db: &VaultDb) {
    let device = db.device_id();
    match crate::instances::paths::vault_paths(app) {
        Ok(paths) => {
            let trackers_file = paths.filter_lists().join(LEGACY_TRACKERS_FILE);
            let dns_file = paths.agent_browser_root().join(LEGACY_DNS_FILE);
            if let Err(error) = import_legacy(db, device, &trackers_file, &dns_file).await {
                log::warn!(
                    "agent browser: the choices of the old files were not imported: {error}"
                );
            }
        }
        Err(error) => log::warn!("agent browser: no folder of the old choices: {error}"),
    }
    let chosen = match db.read(|q| read_trackers(q)).await {
        Ok(chosen) => chosen.unwrap_or_default(),
        Err(error) => {
            log::warn!("agent browser: the tracker choice could not be read: {error}");
            return;
        }
    };
    if let Some(lists) = app
        .try_state::<TrackerLists>()
        .and_then(|lists| lists.0.clone())
    {
        lists.set_settings(chosen);
    }
}

/// Applies tracker preferences received from another device while this vault is open.
/// `apply_vault_choices` handles the initial load and legacy import; this feed keeps the cached
/// tracker engine in step with later CRDT updates to the generic preferences table.
pub fn follow_tracker_preferences<R: Runtime>(app: &AppHandle<R>, state: &AppState) {
    let db = match state.database() {
        Ok(db) => db,
        Err(error) => {
            log::warn!("agent browser: tracker preferences are not followed: {error}");
            return;
        }
    };
    let Some(lists) = app
        .try_state::<TrackerLists>()
        .and_then(|lists| lists.0.clone())
    else {
        return;
    };
    let mut changes = state.vault_changes().subscribe();
    let gate = state.gate().clone();
    let token = gate.token();
    let follow = async move {
        // Read once after subscribing, so changes racing startup are either read here or delivered
        // below (the update feed is broadcast and may already contain a preference change).
        refresh_trackers(&db, &lists).await;
        loop {
            let refresh = tokio::select! {
                () = token.cancelled() => return,
                received = changes.recv() => match received {
                    Ok(tables) => tables.iter().any(|table| table == "preferences"),
                    Err(RecvError::Lagged(_)) => true,
                    Err(RecvError::Closed) => return,
                },
            };
            if refresh {
                refresh_trackers(&db, &lists).await;
            }
        }
    };
    if let Err(error) = gate.spawn(follow) {
        log::warn!("agent browser: tracker preferences are not followed: {error}");
    }
}

async fn refresh_trackers(db: &VaultDb, lists: &Arc<FilterLists>) {
    match db.read(|q| read_trackers(q)).await {
        Ok(settings) => lists.set_settings(settings.unwrap_or_default()),
        Err(error) => {
            log::warn!("agent browser: the tracker choice could not be refreshed: {error}")
        }
    }
}
