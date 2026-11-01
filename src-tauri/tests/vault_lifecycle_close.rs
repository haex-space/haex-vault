//! The close protocol without a window (spec 013 US1, FR-001 to FR-009, SC-002).
//!
//! Phase 1 (`begin_close`) is synchronous and cannot fail; phase 2 (`finish_close`) drains the
//! work, drops the database and asks for the end of the process. The effects that touch the outside
//! world are recorded instead of performed, so each ordering claim can be checked.
//!
//! Maintainability exception (spaex 500-LoC rule): this integration test keeps
//! the close state machine and its recorded side effects together because the
//! assertions describe one lifecycle contract. When another close phase is
//! added, move the IPC harness into `vault_lifecycle_harness.rs` and retain
//! phase-specific tests here.

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{mpsc, Arc, Mutex, Weak};
use std::time::{Duration, Instant};

use async_trait::async_trait;
use haex_crdt::{Database, DatabaseConfig, NoopSignatureProvider, SqlCipherKey};
use tauri::http::HeaderMap;
use tauri::ipc::{CallbackFn, InvokeBody};
use tauri::test::{get_ipc_response, mock_builder, mock_context, noop_assets, INVOKE_KEY};
use tauri::webview::InvokeRequest;
use tokio_util::sync::CancellationToken;

use holzi_lib::adapters::types::{AdapterStream, ChatRequest};
use holzi_lib::adapters::{AdapterError, ProviderAdapter, ProviderModel};
use holzi_lib::chat::session::{ActiveSession, ChatState};
use holzi_lib::identity::{holzi_migration_source, installation_id_path, HolziBootstrap};
use holzi_lib::instances::close::{begin_close, finish_close, CloseContext, CloseTimings};
use holzi_lib::storage::providers::ProviderKind;
use holzi_lib::vault_gate::{CloseEffects, ClosePolicy, DrainOutcome, VaultDb, VaultGate};
use holzi_lib::voice::VoiceState;
use holzi_lib::{ActiveInstanceHandle, AppState};

const VAULT: &str = "vault-a";

/// Small deadlines with the production proportions, so the tests stay quick.
const TIMINGS: CloseTimings = CloseTimings {
    cooperative: Duration::from_millis(100),
    total: Duration::from_millis(300),
    grace: Duration::from_millis(100),
};

/// What the recorder saw at the moment an effect ran.
#[derive(Debug, Clone, Default)]
struct Probe {
    gate_closing: bool,
    gate_token_fired: bool,
    turn_token_fired: bool,
    preload_token_fired: bool,
    /// Strong references to the database at that moment (0 once it is dropped).
    database_refs: usize,
}

/// Stands in for the window and the process: it records what the close asks for.
struct Recorder {
    events: Mutex<Vec<String>>,
    probes: Mutex<HashMap<&'static str, Probe>>,
    gate: VaultGate,
    turn_token: Mutex<Option<CancellationToken>>,
    preload_token: Mutex<Option<CancellationToken>>,
    database: Mutex<Option<Weak<Database>>>,
}

impl Recorder {
    fn new(gate: &VaultGate) -> Arc<Self> {
        Arc::new(Self {
            events: Mutex::new(Vec::new()),
            probes: Mutex::new(HashMap::new()),
            gate: gate.clone(),
            turn_token: Mutex::new(None),
            preload_token: Mutex::new(None),
            database: Mutex::new(None),
        })
    }

    fn record(&self, effect: &'static str, event: String) {
        let fired = |slot: &Mutex<Option<CancellationToken>>| {
            slot.lock()
                .unwrap()
                .as_ref()
                .is_some_and(CancellationToken::is_cancelled)
        };
        let probe = Probe {
            gate_closing: self.gate.is_closing(),
            gate_token_fired: self.gate.token().is_cancelled(),
            turn_token_fired: fired(&self.turn_token),
            preload_token_fired: fired(&self.preload_token),
            database_refs: self
                .database
                .lock()
                .unwrap()
                .as_ref()
                .map_or(0, Weak::strong_count),
        };
        self.probes.lock().unwrap().insert(effect, probe);
        self.events.lock().unwrap().push(event);
    }

    fn events(&self) -> Vec<String> {
        self.events.lock().unwrap().clone()
    }

    fn probe(&self, effect: &str) -> Probe {
        self.probes
            .lock()
            .unwrap()
            .get(effect)
            .cloned()
            .unwrap_or_else(|| panic!("effect {effect} never ran"))
    }

    /// Waits for `event` to have been recorded.
    fn wait_for(&self, event: &str) {
        let deadline = Instant::now() + Duration::from_secs(5);
        while !self.events().iter().any(|seen| seen == event) {
            assert!(Instant::now() < deadline, "{event} never happened");
            std::thread::sleep(Duration::from_millis(5));
        }
    }
}

impl CloseEffects for Recorder {
    fn show_closing_page(&self) {
        self.record("page", "page".into());
    }

    fn announce_closed(&self, name: Option<String>) {
        self.record("announce", format!("announce:{}", name.unwrap_or_default()));
    }

    fn request_end(&self, policy: ClosePolicy) {
        self.record("end", format!("end:{policy:?}"));
    }

    fn force_end(&self, policy: ClosePolicy) {
        self.record("force", format!("force:{policy:?}"));
    }
}

fn open_database(dir: &std::path::Path) -> Database {
    Database::open(DatabaseConfig {
        path: dir.join("vault.db"),
        key: SqlCipherKey::new("vault-lifecycle-close"),
        create_if_missing: true,
        bootstrap: Arc::new(HolziBootstrap::new(installation_id_path(dir)).with_alias("test")),
        signature_provider: Arc::new(NoopSignatureProvider),
        migration_source: holzi_migration_source(),
        trigger_version: haex_crdt::DEFAULT_TRIGGER_VERSION,
        max_transaction_bytes: haex_crdt::MAX_CRDT_TRANSACTION_BYTES,
        max_value_bytes: haex_crdt::MAX_VALUE_BYTES,
    })
    .expect("vault open")
}

/// A process with an active vault: the gate, the state and the recorder that stands in for the
/// window.
struct Session {
    gate: VaultGate,
    app_state: AppState,
    chat: ChatState,
    voice: VoiceState,
    recorder: Arc<Recorder>,
    _dir: tempfile::TempDir,
}

impl Session {
    fn active() -> Self {
        let session = Self::idle();
        let dir = session._dir.path();
        let database = Arc::new(open_database(dir));
        *session.recorder.database.lock().unwrap() = Some(Arc::downgrade(&database));
        session
            .app_state
            .install(
                ActiveInstanceHandle {
                    name: VAULT.into(),
                    database,
                },
                || Ok(()),
            )
            .expect("publish the vault, and move the gate Idle to Active with it");
        session
    }

    fn idle() -> Self {
        let gate = match tokio::runtime::Handle::try_current() {
            Ok(runtime) => VaultGate::with_runtime(runtime),
            Err(_) => VaultGate::new(),
        };
        let chat = ChatState::with_children(gate.children());
        let recorder = Recorder::new(&gate);
        Self {
            app_state: AppState::new(gate.clone()),
            gate,
            chat,
            voice: VoiceState::new(),
            recorder,
            _dir: tempfile::tempdir().expect("temp dir"),
        }
    }

    fn context(&self, policy: ClosePolicy) -> CloseContext<'_> {
        CloseContext::new(
            &self.gate,
            &self.app_state,
            &self.chat,
            &self.voice,
            self.recorder.clone(),
            policy,
            TIMINGS,
        )
    }
}

/// A plain thread holding `hold` until released, standing in for work that cannot be aborted.
fn hold_on_a_thread<T: Send + 'static>(hold: T) -> (mpsc::Sender<()>, std::thread::JoinHandle<()>) {
    let (release, released) = mpsc::channel::<()>();
    let thread = std::thread::spawn(move || {
        let _held = hold;
        let _ = released.recv();
    });
    (release, thread)
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn close_returns_at_once_while_work_never_finishes_and_the_operation_slot_is_held() {
    let session = Session::active();
    // The old failure: `close_instance` took the operation slot and was refused while a turn held it.
    let _operation = session.chat.acquire_operation().expect("take the slot");
    session
        .gate
        .spawn(std::future::pending::<()>())
        .expect("open gate");
    let context = session.context(ClosePolicy::Exit);

    let started = Instant::now();
    let first = begin_close(&context);

    assert!(first);
    assert!(
        started.elapsed() < Duration::from_millis(100),
        "phase 1 returned in {:?}",
        started.elapsed()
    );
    // Phase 2 ends the process even though the task never finished.
    assert_eq!(
        finish_close(&context).await,
        DrainOutcome::DrainedAfterAbort
    );
    session.recorder.wait_for("end:Exit");
}

#[tokio::test]
async fn a_second_close_repeats_no_effect() {
    let session = Session::active();
    let context = session.context(ClosePolicy::Exit);

    assert!(begin_close(&context));
    assert!(
        !begin_close(&context),
        "the repeat reports it changed nothing"
    );
    assert!(!begin_close(&context));

    assert_eq!(
        session.recorder.events(),
        vec!["page".to_string(), format!("announce:{VAULT}")]
    );
}

#[tokio::test]
async fn phase_one_runs_its_effects_once_and_in_order() {
    let session = Session::active();
    // A turn is running: its cancellation slot holds a live token.
    let turn = CancellationToken::new();
    *session.chat.turn_cancellation.lock().unwrap() = Some(turn.clone());
    *session.recorder.turn_token.lock().unwrap() = Some(turn);
    // A preload is running: it has its own token.
    let preload = CancellationToken::new();
    *session.recorder.preload_token.lock().unwrap() = Some(preload.clone());
    session
        .chat
        .install_preload_handle(preload, tokio::spawn(std::future::pending::<()>()));
    let context = session.context(ClosePolicy::Exit);

    assert!(begin_close(&context));

    let page = session.recorder.probe("page");
    assert!(page.gate_closing, "the gate shut before the page changed");
    assert!(
        page.gate_token_fired,
        "the token fired before the page changed"
    );
    assert!(
        page.turn_token_fired,
        "the turn was cancelled before the page changed"
    );
    assert!(page.preload_token_fired, "so was the preload");
    assert_eq!(
        session.recorder.events(),
        vec!["page".to_string(), format!("announce:{VAULT}")],
        "the page changes first, then the list-changed event"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn the_end_is_requested_once_and_the_forced_end_follows_once_for_every_outcome() {
    for (outcome, policy) in [
        (DrainOutcome::Drained, ClosePolicy::Relaunch),
        (DrainOutcome::DrainedAfterAbort, ClosePolicy::Exit),
        (DrainOutcome::Stuck, ClosePolicy::Relaunch),
    ] {
        let session = Session::active();
        let mut held = None;
        match outcome {
            DrainOutcome::Drained => {}
            DrainOutcome::DrainedAfterAbort => {
                session
                    .gate
                    .spawn(async {
                        loop {
                            tokio::time::sleep(Duration::from_millis(5)).await;
                        }
                    })
                    .expect("open gate");
            }
            DrainOutcome::Stuck => {
                held = Some(hold_on_a_thread(
                    session.gate.tracker_token().expect("gate"),
                ));
            }
        }
        let context = session.context(policy);
        assert!(begin_close(&context));

        assert_eq!(finish_close(&context).await, outcome, "{outcome:?}");

        let end = format!("end:{policy:?}");
        let force = format!("force:{policy:?}");
        session.recorder.wait_for(&force);
        // Outlast the outer deadline, which shares the forced end and must not run it twice.
        std::thread::sleep(TIMINGS.total + TIMINGS.grace + Duration::from_millis(150));
        let events = session.recorder.events();
        assert_eq!(
            &events[..2],
            &["page".to_string(), format!("announce:{VAULT}")],
            "{outcome:?}: phase-one effects must stay first"
        );
        // At the total deadline, the phase-one safety timer and the post-end grace timer can
        // become runnable together. Their callbacks must each run once, but their order is not
        // observable or guaranteed under scheduler contention.
        assert_eq!(events.len(), 4, "{outcome:?}: exactly two end effects");
        assert_eq!(
            events[2..].iter().filter(|event| *event == &end).count(),
            1,
            "{outcome:?}: one end request"
        );
        assert_eq!(
            events[2..].iter().filter(|event| *event == &force).count(),
            1,
            "{outcome:?}: one forced end"
        );
        if let Some((release, thread)) = held {
            release.send(()).expect("release");
            thread.join().expect("thread ends");
        }
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn the_forced_end_also_covers_a_close_whose_second_phase_never_runs() {
    let session = Session::active();
    let context = session.context(ClosePolicy::Exit);

    // Only phase 1: as if the runtime were blocked and the background task never got to run.
    assert!(begin_close(&context));

    session.recorder.wait_for("force:Exit");
    assert!(
        !session.recorder.events().contains(&"end:Exit".to_string()),
        "the normal end was never requested"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn the_database_is_dropped_after_every_clone_and_before_the_end() {
    let session = Session::active();
    let clone = session.app_state.database().expect("a tracked handle");
    let context = session.context(ClosePolicy::Exit);
    assert!(begin_close(&context));
    // The work that holds the clone finishes inside the cooperative window.
    let finished = std::thread::spawn(move || {
        std::thread::sleep(Duration::from_millis(30));
        drop(clone);
    });

    let outcome = finish_close(&context).await;
    finished.join().expect("work ends");

    assert_eq!(outcome, DrainOutcome::Drained);
    assert_eq!(
        session.recorder.probe("end").database_refs,
        0,
        "the database was gone when the end was requested"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_clone_that_outlives_the_limit_keeps_the_database_but_not_the_state() {
    let session = Session::active();
    let clone = session.app_state.database().expect("a tracked handle");
    let (release, thread) = hold_on_a_thread(clone);
    let context = session.context(ClosePolicy::Exit);
    assert!(begin_close(&context));

    let outcome = finish_close(&context).await;

    assert_eq!(outcome, DrainOutcome::Stuck);
    assert_eq!(
        session.recorder.probe("end").database_refs,
        1,
        "only the stuck clone still refers to it; the state let go"
    );
    assert!(
        session.app_state.database().is_err(),
        "nothing can reach the vault any more"
    );
    release.send(()).expect("release");
    thread.join().expect("thread ends");
}

/// A loaded model whose adapter keeps a database handle, as a delegated CLI does for its tools.
struct AdapterHoldingTheDatabase {
    _database: VaultDb,
}

#[async_trait]
impl ProviderAdapter for AdapterHoldingTheDatabase {
    async fn list_models(&self) -> Result<Vec<ProviderModel>, AdapterError> {
        Ok(Vec::new())
    }

    async fn stream_chat(&self, _request: ChatRequest) -> Result<AdapterStream, AdapterError> {
        std::future::pending().await
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_database_handle_kept_by_the_loaded_model_does_not_hold_the_close_up() {
    let session = Session::active();
    *session.chat.session.lock().unwrap() = Some(ActiveSession {
        model_id: "delegate".into(),
        provider_id: None,
        provider_kind: ProviderKind::Local,
        adapter: Arc::new(AdapterHoldingTheDatabase {
            _database: session.app_state.database().expect("a tracked handle"),
        }),
        tokenizer_repo: String::new(),
        context_window: None,
        engine: None,
    });
    let context = session.context(ClosePolicy::Exit);
    assert!(begin_close(&context));

    // The session is let go before the drain waits, so the handle inside it cannot make the
    // drain run into its limit.
    let outcome = finish_close(&context).await;

    assert_eq!(outcome, DrainOutcome::Drained);
    assert_eq!(session.recorder.probe("end").database_refs, 0);
}

#[tokio::test]
async fn closing_before_any_vault_is_open_still_ends_the_process() {
    let session = Session::idle();
    let context = session.context(ClosePolicy::Exit);

    assert!(begin_close(&context));
    assert_eq!(finish_close(&context).await, DrainOutcome::Drained);

    session.recorder.wait_for("end:Exit");
    assert_eq!(
        session.recorder.events()[..2],
        ["page".to_string(), "announce:".to_string()]
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_request_running_when_the_close_starts_ends_with_vault_closed() {
    let session = Session::active();
    let gate = session.gate.clone();
    let running = tokio::spawn(async move { gate.run(std::future::pending::<()>()).await });
    tokio::time::sleep(Duration::from_millis(20)).await;
    let context = session.context(ClosePolicy::Exit);

    assert!(begin_close(&context));

    let result = tokio::time::timeout(Duration::from_secs(1), running)
        .await
        .expect("the request ended at once")
        .expect("the task ended");
    assert!(matches!(result, Err(holzi_lib::HolziError::VaultClosed)));
}

/// Set by the command below; a vault command must never run once the close has started.
static VAULT_COMMAND_RAN: AtomicBool = AtomicBool::new(false);

#[tauri::command]
async fn list_threads() -> &'static str {
    VAULT_COMMAND_RAN.store(true, Ordering::SeqCst);
    "vault data"
}

#[test]
fn after_the_close_the_gateway_rejects_a_vault_command() {
    let session = Session::active();
    let app = mock_builder()
        .manage(session.gate.clone())
        .invoke_handler(session.gate.wrap(tauri::generate_handler![list_threads]))
        .build(mock_context(noop_assets()))
        .expect("mock app");
    let webview = tauri::WebviewWindowBuilder::new(&app, "main", Default::default())
        .build()
        .expect("mock webview");
    let invoke = || {
        get_ipc_response(
            &webview,
            InvokeRequest {
                cmd: "list_threads".into(),
                callback: CallbackFn(0),
                error: CallbackFn(1),
                url: "tauri://localhost".parse().expect("url"),
                body: InvokeBody::default(),
                headers: HeaderMap::new(),
                invoke_key: INVOKE_KEY.to_string(),
            },
        )
        .map(|body| body.deserialize::<serde_json::Value>().expect("json body"))
    };
    assert_eq!(
        invoke(),
        Ok("vault data".into()),
        "answered before the close"
    );
    VAULT_COMMAND_RAN.store(false, Ordering::SeqCst);

    assert!(begin_close(&session.context(ClosePolicy::Exit)));

    assert_eq!(invoke(), Err(serde_json::json!({ "kind": "VaultClosed" })));
    assert!(
        !VAULT_COMMAND_RAN.load(Ordering::SeqCst),
        "the command body never ran"
    );
}
