//! Sync rules in a vault session (research R8): the control loop starts after the vault opened
//! (`instances/open.rs` and `instances/import.rs`) and ends with it. Places open as the window
//! opens them, so encrypted folders and other own devices work as there.

use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager};
use tokio::sync::mpsc::{unbounded_channel, UnboundedSender};
use tokio_util::sync::CancellationToken;

use super::model::{SyncRule, SyncRulePlace};
use super::place::Place;
use super::place_device::DevicePlace;
use super::place_storage::StoragePlace;
use super::run::{run, RunContext, RunReport};
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
    let app = app.clone();
    let started = gate.spawn(async move {
        control(runner, received, shared, token).await;
        app.state::<FileSyncState>().set(None);
    });
    if let Err(error) = started {
        log::warn!("file_sync: the rules could not start: {error}");
        sync.set(None);
    }
}

struct TauriRunner {
    app: AppHandle,
    db: VaultDb,
    device: String,
}

impl TauriRunner {
    async fn device_name(&self) -> String {
        let device = self.device.clone();
        self.db
            .read(move |r| {
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

    async fn run(&self, rule: SyncRule, cancel: CancellationToken) -> RunReport {
        let run_id = uuid::Uuid::new_v4().to_string();
        let name = self.device_name().await;
        let today: String = clock::now().chars().take(10).collect();
        let now = clock::now;
        let ctx = RunContext {
            db: &self.db,
            device: &self.device,
            device_name: &name,
            today: &today,
            now: &now,
            run_id: &run_id,
            cancel: &cancel,
        };
        let (a, b) = match (
            open_place(&self.app, &rule.place_a).await,
            open_place(&self.app, &rule.place_b).await,
        ) {
            (Ok(a), Ok(b)) => (a, b),
            (Err(error), _) | (_, Err(error)) => {
                return failed_open(&ctx, &rule, error).await;
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
        // A one-way rule changes when its source does.
        watch_place(&rule.place_a, notify)
    }
}

/// A place of a rule as the window opens it.
async fn open_place(app: &AppHandle, place: &SyncRulePlace) -> Result<Box<dyn Place>, FilesError> {
    let (state, files) = (app.state::<AppState>(), app.state::<FilesState>());
    match &place.source {
        SourceRef::Device => {
            match place_of(app, &state, &files, &place.source, &place.path, Want::Write).await? {
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

/// The report of a run whose place could not be opened, noted on the rule like a run.
async fn failed_open(ctx: &RunContext<'_>, rule: &SyncRule, error: FilesError) -> RunReport {
    let outcome = match error.code {
        crate::files::FilesErrorCode::Unreachable => SyncRuleOutcome::Waiting,
        _ => SyncRuleOutcome::Error,
    };
    let (device, id, at, reason) = (
        ctx.device.to_owned(),
        rule.id.clone(),
        (ctx.now)(),
        error.message.clone(),
    );
    if let Err(error) = ctx
        .db
        .write(move |tx| {
            store::set_result(tx, &device, &id, &at, outcome, Some(&reason))?;
            Ok(())
        })
        .await
    {
        log::warn!("file_sync: the result of a run could not be written: {error}");
    }
    RunReport {
        outcome,
        error: Some(error.message),
        counts: Default::default(),
        failures: Vec::new(),
        skipped: Vec::new(),
        ignore_errors: Vec::new(),
        ignored: 0,
        trashed: Vec::new(),
    }
}
