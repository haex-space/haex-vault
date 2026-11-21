//! The diagnostic log (spec 098): a log in every build, in the folder of the vault that is open.
//!
//! A swappable sink ([`sink::DiagnosticSink`]) is the one target of the process logger. It is
//! attached when a vault is unlocked ([`begin`], the first thing the session services do) and
//! detached when the vault closes ([`end`]); before that nothing is written, kept or buffered
//! (FR-008), and the lines go to `logs/` in that vault's own folder (FR-007, FR-010). The level,
//! "Standard" or "Ausführlich", lives in a table of the vault that does not sync ([`expiry`]) and
//! falls back to "Standard" after 24 hours or at the next start of the app.

pub mod commands;
pub(crate) mod expiry;
pub mod level;
pub mod rotation;
pub mod sink;

use std::path::Path;
use std::sync::{Arc, LazyLock, Mutex, PoisonError};
use std::time::{Instant, SystemTime, UNIX_EPOCH};

use log::{LevelFilter, Log, Metadata, Record};
use serde::Serialize;
use tauri::{AppHandle, Emitter as _, Runtime};
use tauri_plugin_log::{Builder, Target, TargetKind};
use ts_rs::TS;

use self::expiry::{LevelState, DEADLINE};
use self::level::LogLevel;
use self::sink::DiagnosticSink;
use crate::state::AppState;
use crate::vault_gate::{VaultDb, VaultGate};

/// The level or the end time changed, or a vault was unlocked (the interface learns its session
/// token from this); payload [`DiagnosticLogStatus`].
pub const EVENT_CHANGED: &str = "haex:diagnostic-log-changed";
/// The first write that failed after the earlier ones went through.
pub const EVENT_UNWRITABLE: &str = "haex:diagnostic-log-unwritable";

static SINK: LazyLock<DiagnosticSink> = LazyLock::new(DiagnosticSink::new);
static CURRENT: Mutex<LevelState> = Mutex::new(LevelState::STANDARD);
/// The task that ends "Ausführlich", while one runs.
static EXPIRY: Mutex<Option<tokio::task::AbortHandle>> = Mutex::new(None);

/// Serializes the tests that touch the statics above and the global max level.
#[cfg(test)]
pub(crate) static TEST_LOCK: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

/// What the setting shows and the interface plugin follows.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/types/bindings/")]
pub struct DiagnosticLogStatus {
    pub level: LogLevel,
    /// Unix milliseconds; only with "Ausführlich".
    #[ts(type = "number | null")]
    pub until_ms: Option<i64>,
    /// `false` after a failed write or a failed start of the log.
    pub writable: bool,
    /// Fresh and opaque per unlock; the interface sends it with every message, so a message
    /// queued for an earlier vault is dropped. `None` while no vault is attached.
    pub session_id: Option<String>,
}

/// Hands a status to whoever shows it (the window); a test hands in a recorder.
pub type Emitter = Arc<dyn Fn(DiagnosticLogStatus) + Send + Sync>;

/// The status now.
pub fn status() -> DiagnosticLogStatus {
    let current = *CURRENT.lock().unwrap_or_else(PoisonError::into_inner);
    DiagnosticLogStatus {
        level: current.level,
        until_ms: current.until_ms,
        writable: !SINK.unwritable(),
        session_id: SINK.session(),
    }
}

/// The sink as the `log` facade sees it.
struct GlobalSink;

impl Log for GlobalSink {
    fn enabled(&self, metadata: &Metadata<'_>) -> bool {
        SINK.enabled(metadata)
    }

    fn log(&self, record: &Record<'_>) {
        SINK.log(record);
    }

    fn flush(&self) {
        SINK.flush();
    }
}

/// The sink as the process logger, for a test binary that has no plugin: `log::set_logger` with
/// this, then [`attach_folder`].
pub fn logger() -> &'static dyn Log {
    &GlobalSink
}

/// Attaches the sink to `folder` at `level` and returns the token of the attachment: what
/// [`begin`] does once it knows the folder and the level.
pub fn attach_folder(folder: &Path, level: LogLevel) -> std::io::Result<String> {
    let session = SINK.attach(folder, level)?;
    apply_state(LevelState {
        level,
        until_ms: None,
    });
    Ok(session)
}

/// The log plugin of every build: the sink is its one target (the debug build keeps the terminal
/// beside it). The plugin's own formatter would wrap every message, so it passes it through and
/// the sink writes its own line. Call [`LogLevel::apply`] right after registering it: the plugin
/// sets the global level to `Trace`.
pub fn plugin<R: Runtime>() -> tauri::plugin::TauriPlugin<R> {
    let dispatch =
        tauri_plugin_log::fern::Dispatch::new().chain(Box::new(GlobalSink) as Box<dyn Log>);
    let mut builder = Builder::default()
        .clear_targets()
        .level(LevelFilter::Trace)
        .format(|out, message, _| out.finish(format_args!("{message}")))
        .target(Target::new(TargetKind::Dispatch(dispatch)));
    for module in level::DAMPED {
        builder = builder.level_for(module, LevelFilter::Warn);
    }
    if cfg!(debug_assertions) {
        builder = builder.target(
            Target::new(TargetKind::Stdout).format(|out, message, record| {
                out.finish(format_args!(
                    "[{}][{}] {message}",
                    record.level(),
                    record.target()
                ));
            }),
        );
    }
    builder.build()
}

/// Starts the log of the vault that was just unlocked: its folder, its level, the end of
/// "Ausführlich". The first call of the session services; never fails the unlock.
pub async fn begin<R: Runtime>(app: &AppHandle<R>, state: &AppState) {
    let Ok(Some(name)) = state.active_name() else {
        return;
    };
    let Ok(paths) = crate::instances::paths::vault_paths(app) else {
        return;
    };
    let Ok(db) = state.database() else {
        return;
    };
    let handle = app.clone();
    SINK.on_unwritable(move || {
        let _ = handle.emit(EVENT_UNWRITABLE, ());
    });
    start(&db, state.gate(), &paths.logs_dir(&name), &emitter(app)).await;
}

/// The window as the receiver of status changes.
pub fn emitter<R: Runtime>(app: &AppHandle<R>) -> Emitter {
    let app = app.clone();
    Arc::new(move |status| {
        if let Err(error) = app.emit(EVENT_CHANGED, status) {
            log::warn!("diagnostic log: could not announce the status: {error}");
        }
    })
}

/// [`begin`] without the app: attaches `folder` at the level the vault stored, resets one that
/// ended or belongs to another run, and starts the timer of "Ausführlich".
pub(crate) async fn start(db: &VaultDb, gate: &VaultGate, folder: &Path, emit: &Emitter) {
    let now = now_ms();
    let read = db
        .read(move |q| expiry::read(q, now, expiry::run_id()).map_err(Into::into))
        .await;
    let (judged, read_error) = match read {
        Ok(judged) => (judged, None),
        // A vault whose level cannot be read logs at "Standard", the safe side.
        Err(error) => (
            expiry::Judged {
                state: LevelState::STANDARD,
                rewrite: false,
            },
            Some(error),
        ),
    };
    let state = judged.state;
    let log_folder = folder.to_path_buf();
    let folder = log_folder.clone();
    let attached = tokio::task::spawn_blocking(move || SINK.attach(&folder, state.level))
        .await
        .unwrap_or_else(|error| Err(std::io::Error::other(error)));
    apply_state(state);
    if let Err(error) = &attached {
        // The sink is detached, so this goes nowhere; the status says the log is not writable.
        log::warn!("diagnostic log: could not start in {log_folder:?}: {error}");
    }
    if let Some(error) = read_error {
        log::warn!("diagnostic log: could not read the level, using Standard: {error}");
    }
    if judged.rewrite {
        let rewritten = db
            .write(move |tx| {
                expiry::switch(tx, LogLevel::Standard, expiry::run_id(), now).map_err(Into::into)
            })
            .await;
        if let Err(error) = rewritten {
            log::warn!("diagnostic log: could not reset the level to Standard: {error}");
        }
    }
    match state.until_ms {
        Some(until_ms) if state.level == LogLevel::Verbose => {
            schedule_expiry(db.clone(), gate, until_ms, emit);
        }
        _ => DEADLINE.clear(),
    }
    emit(status());
}

/// Stops the log of the vault that closes, after its last lines: flushes and detaches. Later
/// records are dropped again.
pub fn end() {
    cancel_expiry();
    DEADLINE.clear();
    SINK.detach();
    apply_state(LevelState::STANDARD);
}

/// Makes `state` the level in force: for the status, the sink's files and the `log` macros.
pub(crate) fn apply_state(state: LevelState) {
    *CURRENT.lock().unwrap_or_else(PoisonError::into_inner) = state;
    state.level.apply();
}

/// Ends "Ausführlich" after what remains of its 24 hours, as session work: it ends with the
/// close. A timer that runs is replaced.
pub(crate) fn schedule_expiry(db: VaultDb, gate: &VaultGate, until_ms: i64, emit: &Emitter) {
    let left = expiry::remaining(until_ms, now_ms(), DEADLINE.left(Instant::now()));
    let token = gate.token();
    let task_gate = gate.clone();
    let emit = Arc::clone(emit);
    let spawned = gate.spawn(async move {
        tokio::select! {
            () = token.cancelled() => return,
            () = tokio::time::sleep(left) => {}
        }
        // This task's own handle: let go of it without aborting the task that is running.
        EXPIRY.lock().unwrap_or_else(PoisonError::into_inner).take();
        if let Err(error) = commands::switch_level(&db, &task_gate, LogLevel::Standard, &emit).await
        {
            log::warn!("diagnostic log: \"Ausführlich\" could not be ended: {error}");
        }
    });
    match spawned {
        Ok(handle) => replace_expiry(Some(handle.abort_handle())),
        Err(error) => log::warn!("diagnostic log: the timer did not start: {error}"),
    }
}

/// Stops the timer, if one runs.
pub(crate) fn cancel_expiry() {
    replace_expiry(None);
}

fn replace_expiry(next: Option<tokio::task::AbortHandle>) {
    let mut slot = EXPIRY.lock().unwrap_or_else(PoisonError::into_inner);
    if let Some(old) = std::mem::replace(&mut *slot, next) {
        old.abort();
    }
}

/// Whether the timer of "Ausführlich" runs.
#[cfg(test)]
pub(crate) fn expiry_running() -> bool {
    EXPIRY
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .as_ref()
        .is_some_and(|handle| !handle.is_finished())
}

pub(crate) fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |elapsed| elapsed.as_millis() as i64)
}
