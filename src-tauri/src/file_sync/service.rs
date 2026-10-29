//! Sync rules in a vault session (research R8): the control loop starts after the vault opened
//! (`instances/open.rs` and `instances/import.rs`) and ends with it. Places open as the window
//! opens them, so encrypted folders and other own devices work as there.

use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use serde::Serialize;
use tauri::{AppHandle, Emitter, Listener, Manager};
use tokio::sync::mpsc::{unbounded_channel, UnboundedSender};
use tokio_util::sync::CancellationToken;

use super::model::{SyncRule, SyncRuleDirection, SyncRulePlace};
use super::place::Place;
use super::place_device::DevicePlace;
use super::place_storage::StoragePlace;
use super::progress::{Reporter, SYNC_RULE_PROGRESS};
use super::run::{note, run, RunContext, RunReport, Started};
use super::run_log::SyncRuleRunCause;
use super::schedule::{control, Command, Runner, Shared};
use super::store;
use super::watch::watch_place;
use crate::file_sync::model::SyncRuleOutcome;
use crate::files::access::Want;
use crate::files::browser_commands::{place_of, window_storage, Place as WindowPlace};
use crate::files::state::FilesState;
use crate::files::{FilesError, SourceRef};
use crate::passwords::clock;
use crate::state::AppState;
use crate::sync::events::SYNC_DEVICES_CHANGED;
use crate::vault_gate::VaultDb;

/// The event the window hears when a rule or the list changed.
pub const SYNC_RULES_CHANGED: &str = "sync-rules-changed";

/// What the event carries.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SyncRulesChanged {
    pub rule_id: Option<String>,
}

/// The session's loop, as the commands see it.
pub struct Session {
    pub device: String,
    pub commands: UnboundedSender<Command>,
    pub shared: Arc<Mutex<Shared>>,
}

/// Managed state: the running session, if a vault is open.
#[derive(Default)]
pub struct FileSyncState {
    session: Mutex<Option<Arc<Session>>>,
}

impl FileSyncState {
    /// The running session.
    pub fn session(&self) -> Option<Arc<Session>> {
        self.session
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .clone()
    }

    fn set(&self, session: Option<Arc<Session>>) {
        *self.session.lock().unwrap_or_else(|e| e.into_inner()) = session;
    }

    /// Tells the loop something; a missing session hears nothing.
    pub fn send(&self, command: Command) {
        if let Some(session) = self.session() {
            let _ = session.commands.send(command);
        }
    }

    /// Cancels a rule and waits until the session has stopped its run, if any.
    pub async fn cancel_and_wait(&self, id: String) {
        let Some(session) = self.session() else {
            return;
        };
        {
            let mut shared = session.shared.lock().unwrap_or_else(|e| e.into_inner());
            shared.cancel_processed.remove(&id);
            shared.cancel_requested.insert(id.clone());
        }
        if session.commands.send(Command::Cancel(id.clone())).is_err() {
            let mut shared = session.shared.lock().unwrap_or_else(|e| e.into_inner());
            shared.cancel_requested.remove(&id);
            shared.cancel_processed.remove(&id);
            return;
        }
        loop {
            // Closing a vault can stop the control loop after the command was sent but before it
            // was received. Do not leave a command caller waiting for a marker that will never be
            // written; a replacement session must not satisfy the old cancellation either.
            match self.session() {
                Some(current) if Arc::ptr_eq(&current, &session) => {}
                _ => return,
            }
            let done = {
                let shared = session.shared.lock().unwrap_or_else(|e| e.into_inner());
                shared.cancel_processed.contains(&id) && !shared.running.contains(&id)
            };
            if done {
                return;
            }
            tokio::time::sleep(std::time::Duration::from_millis(5)).await;
        }
    }

    /// Releases the guard after the database operation and its reload command are queued.
    pub fn release_cancel(&self, id: String) {
        let Some(session) = self.session() else {
            return;
        };
        if session
            .commands
            .send(Command::ReleaseCancel(id.clone()))
            .is_err()
        {
            let mut shared = session.shared.lock().unwrap_or_else(|e| e.into_inner());
            shared.cancel_requested.remove(&id);
            shared.cancel_processed.remove(&id);
        }
    }
}

/// Starts the loop for the vault just opened.
pub fn start_for_active_instance(app: &AppHandle, state: &AppState) {
    let Ok(db) = state.database() else {
        return;
    };
    let device = match crate::storage::wm_session_commands::current_device_uuid(app, &db) {
        Ok(device) => device.to_string(),
        Err(error) => {
            log::warn!("file_sync: no device of this vault, rules do not run: {error}");
            return;
        }
    };
    let (commands, received) = unbounded_channel();
    // A rule that waits for another device tries again when the devices change (FR-024).
    let wake = commands.clone();
    let listener = app.listen(SYNC_DEVICES_CHANGED, move |_| {
        let _ = wake.send(Command::Wake);
    });
    let shared = Arc::new(Mutex::new(Shared::default()));
    let session = Arc::new(Session {
        device: device.clone(),
        commands,
        shared: Arc::clone(&shared),
    });
    let sync = app.state::<FileSyncState>();
    sync.set(Some(session));
    let runner = Arc::new(TauriRunner {
        app: app.clone(),
        db,
        device,
    });
    let gate = state.gate();
    let token = gate.token();
    let ended = app.clone();
    let started = gate.spawn(async move {
        control(runner, received, shared, token).await;
        ended.unlisten(listener);
        ended.state::<FileSyncState>().set(None);
    });
    if let Err(error) = started {
        log::warn!("file_sync: the rules could not start: {error}");
        app.unlisten(listener);
        sync.set(None);
    }
}

struct TauriRunner {
    app: AppHandle,
    db: VaultDb,
    device: String,
}

/// The name of `device` in conflict copies: its alias, or "holzi".
pub(super) async fn device_name(db: &VaultDb, device: &str) -> String {
    let device = device.to_owned();
    db.read(move |r| {
        Ok(crate::storage::query::Query::query_row(
            r,
            "SELECT alias FROM known_devices WHERE vault_device_uuid = ?1",
            &[&device],
            |row| row.get::<_, Option<String>>(0),
        )?
        .flatten())
    })
    .await
    .ok()
    .flatten()
    .filter(|name| !name.trim().is_empty())
    .unwrap_or_else(|| "holzi".to_owned())
}

#[async_trait]
impl Runner for TauriRunner {
    async fn rules(&self) -> Vec<SyncRule> {
        let device = self.device.clone();
        match self.db.read(move |r| Ok(store::rules(r, &device))).await {
            Ok(Ok(rules)) => rules,
            Ok(Err(error)) | Err(error) => {
                log::warn!("file_sync: the rules could not be read: {error}");
                Vec::new()
            }
        }
    }

    async fn run(
        &self,
        rule: SyncRule,
        cause: SyncRuleRunCause,
        cancel: CancellationToken,
    ) -> RunReport {
        let started = Started::now();
        let run_id = uuid::Uuid::new_v4().to_string();
        let name = device_name(&self.db, &self.device).await;
        let today: String = clock::now().chars().take(10).collect();
        let now = clock::now;
        let app = self.app.clone();
        let progress = Reporter::new(&rule.id, move |progress| {
            if let Err(error) = app.emit(SYNC_RULE_PROGRESS, progress) {
                log::warn!("file_sync: emit {SYNC_RULE_PROGRESS} failed: {error}");
            }
        });
        let ctx = RunContext {
            db: &self.db,
            device: &self.device,
            device_name: &name,
            today: &today,
            now: &now,
            run_id: &run_id,
            cause,
            cancel: &cancel,
            progress: &progress,
        };
        let (a, b) = match (
            open_place(&self.app, &rule.place_a).await,
            open_place(&self.app, &rule.place_b).await,
        ) {
            (Ok(a), Ok(b)) => (a, b),
            (Err(error), _) | (_, Err(error)) => {
                return failed_open(&ctx, &rule, error, started).await;
            }
        };
        run(&ctx, &rule, a.as_ref(), b.as_ref()).await
    }

    fn changed(&self, rule_id: Option<String>) {
        if let Err(error) = self
            .app
            .emit(SYNC_RULES_CHANGED, SyncRulesChanged { rule_id })
        {
            log::warn!("file_sync: emit {SYNC_RULES_CHANGED} failed: {error}");
        }
    }

    fn watch(&self, rule: &SyncRule, notify: Arc<dyn Fn() + Send + Sync>) -> Option<Box<dyn Send>> {
        // A one-way rule changes when its source does, a two-way rule when either place does.
        let a = watch_place(&rule.place_a, Arc::clone(&notify));
        if rule.direction != SyncRuleDirection::Both {
            return a;
        }
        match (a, watch_place(&rule.place_b, notify)) {
            (None, None) => None,
            (a, b) => Some(Box::new((a, b))),
        }
    }
}

/// A place of a rule as the window opens it, to write.
pub(super) async fn open_place(
    app: &AppHandle,
    place: &SyncRulePlace,
) -> Result<Box<dyn Place>, FilesError> {
    open_place_for(app, place, Want::Write).await
}

/// A place of a rule as the window opens it for `want`.
pub(super) async fn open_place_for(
    app: &AppHandle,
    place: &SyncRulePlace,
    want: Want,
) -> Result<Box<dyn Place>, FilesError> {
    let (state, files) = (app.state::<AppState>(), app.state::<FilesState>());
    match &place.source {
        SourceRef::Device => {
            match place_of(app, &state, &files, &place.source, &place.path, want).await? {
                WindowPlace::Device => Ok(Box::new(DevicePlace::new(&place.path))),
                WindowPlace::Storage { files, path, .. } => {
                    Ok(Box::new(StoragePlace::new(Arc::new(files), path)))
                }
            }
        }
        SourceRef::Storage { storage_id } => {
            let opened = window_storage(app, &state, &files, storage_id).await?;
            Ok(Box::new(StoragePlace::new(
                Arc::new(opened),
                place.path.clone(),
            )))
        }
    }
}

/// The report of a run whose place could not be opened, noted like a run.
async fn failed_open(
    ctx: &RunContext<'_>,
    rule: &SyncRule,
    error: FilesError,
    started: Started,
) -> RunReport {
    let outcome = match error.code {
        crate::files::FilesErrorCode::Unreachable => SyncRuleOutcome::Waiting,
        _ => SyncRuleOutcome::Error,
    };
    let report = RunReport::stopped(outcome, error.message);
    note(ctx, rule, &report, started).await;
    report
}
