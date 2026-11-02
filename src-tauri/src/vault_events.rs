//! Tells the frontend which tables of the open vault changed, whatever changed them.
//!
//! haex-crdt reports the tables of every committed transaction: a local write from any window,
//! an agent or an extension, a batch received from another device, a resync. This module turns
//! those reports into the one [`VAULT_DATA_CHANGED`] event, so a view that shows vault data
//! needs no knowledge of where a change came from (spec 024 FR-032, extended to every writer).
//!
//! Reports are collected for [`WINDOW`] and sent as one event with the union of their tables, so
//! a burst of commits (a streaming chat reply, a received page of thousands of rows) wakes the
//! frontend once.

use std::collections::BTreeSet;
use std::sync::Arc;
use std::time::Duration;

use haex_crdt::Database;
use serde::Serialize;
use tauri::{AppHandle, Emitter, Runtime};
use tokio::sync::broadcast;
use tokio::sync::mpsc::{unbounded_channel, UnboundedReceiver};
use tokio::time::{sleep_until, Instant};
use tokio_util::sync::CancellationToken;

use crate::state::AppState;

/// Something in the open vault's database changed.
pub const VAULT_DATA_CHANGED: &str = "vault-data-changed";

/// How long after the first report of a burst further reports are still collected into the same
/// event. Short enough that a change shows up at once; the window is fixed, not restarted by
/// each report, so a steady stream of commits still yields an event every window.
pub const WINDOW: Duration = Duration::from_millis(50);

/// How many announcements a slow subscriber of [`VaultChanges`] may fall behind before it skips
/// the oldest. A subscriber that lagged must treat that as "anything may have changed".
pub const CHANGES_CAPACITY: usize = 256;

/// The same announcements as [`VAULT_DATA_CHANGED`], for Rust subscribers: the extension host
/// (spec 017) filters them per extension frame and follows registry changes from other devices.
/// One sender lives in [`AppState`]; subscribing never touches the database observer, of which
/// haex-crdt keeps only one (`observe_committed_changes` replaces an earlier one).
pub type VaultChanges = broadcast::Sender<Arc<Vec<String>>>;

/// A new, unconnected [`VaultChanges`] sender.
pub fn changes_channel() -> VaultChanges {
    broadcast::channel(CHANGES_CAPACITY).0
}

/// Hands one window's tables to every Rust subscriber. Having none is normal.
pub(crate) fn publish(changes: &VaultChanges, tables: &[String]) {
    let _ = changes.send(Arc::new(tables.to_vec()));
}

/// Payload of [`VAULT_DATA_CHANGED`]: the tables that changed, so a view reloads only what it
/// shows from them.
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct VaultDataChanged {
    pub tables: Vec<String>,
}

/// Starts the feed for the vault session `state` just published: observes the open database and
/// emits [`VAULT_DATA_CHANGED`] until the session ends. A failure is logged and the frontend
/// then only sees its own changes (Constitution VII: never blocks the session).
pub fn start_for_active_instance<R: Runtime>(app: &AppHandle<R>, state: &AppState) {
    let db = match state.database() {
        Ok(db) => db,
        Err(error) => {
            log::warn!("vault events: no open vault, changes will not be announced: {error}");
            return;
        }
    };
    let received = observe(&db.database());
    let gate = state.gate();
    let app = app.clone();
    let changes = state.vault_changes().clone();
    if let Err(error) = gate.spawn(run(received, gate.token(), move |tables| {
        publish(&changes, &tables);
        if let Err(error) = app.emit(VAULT_DATA_CHANGED, VaultDataChanged { tables }) {
            log::warn!("vault events: emit {VAULT_DATA_CHANGED} failed: {error}");
        }
    })) {
        log::warn!("vault events: the feed did not start: {error}");
    }
}

/// Queues the tables of every transaction `db` commits from now on, whatever wrote them.
pub(crate) fn observe(db: &Database) -> UnboundedReceiver<BTreeSet<String>> {
    let (reports, received) = unbounded_channel();
    db.observe_committed_changes(move |tables| {
        // Runs inside the commit: only queue. A closed channel means the session is over.
        let _ = reports.send(tables.clone());
    });
    received
}

/// Collects reports into windows and hands each window's tables (sorted) to `emit`, until
/// `token` is cancelled or every sender is gone.
async fn run(
    mut reports: UnboundedReceiver<BTreeSet<String>>,
    token: CancellationToken,
    emit: impl Fn(Vec<String>),
) {
    loop {
        let first = tokio::select! {
            () = token.cancelled() => return,
            first = reports.recv() => first,
        };
        let Some(mut tables) = first else { return };
        let deadline = Instant::now() + WINDOW;
        let open = loop {
            tokio::select! {
                () = sleep_until(deadline) => break true,
                more = reports.recv() => match more {
                    Some(more) => tables.extend(more),
                    None => break false,
                },
            }
        };
        emit(tables.into_iter().collect());
        if !open {
            return;
        }
    }
}

#[cfg(test)]
#[path = "vault_events_tests.rs"]
mod tests;
