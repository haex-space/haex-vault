//! Shared fixture for the `chat_tool_loop_*` integration binaries
//! (`../chat_tool_loop_core.rs`, `../chat_tool_loop_permissions.rs`,
//! `../chat_tool_loop_retry.rs`) — `StubAdapter`, `ScriptedTool`, `open_db`,
//! `seed_thread`, `session_with`, `spawn_turn` and `run_scripted_turn`, plus
//! the smaller helpers each of those builds on.
//!
//! Included per binary via `#[path = "common/tool_loop_fixture.rs"] mod
//! tool_loop_fixture;` — no binary uses every item here, so that `mod` is
//! marked `#[allow(dead_code)]` at each call site rather than here.

use std::collections::VecDeque;
use std::path::PathBuf;
use std::sync::{Arc, Mutex as StdMutex};

use async_trait::async_trait;
use haex_crdt::{Database, DatabaseConfig, NoopSignatureProvider, SqlCipherKey};
use serde_json::Value;
use tokio_util::sync::CancellationToken;
use uuid::Uuid;

use holzi_lib::adapters::types::{ChatRequest, ToolCall as LlmToolCall, ToolSpec};
use holzi_lib::adapters::{
    AdapterError, AdapterStream, ProviderAdapter, ProviderModel, StreamChunk, StreamError,
};
use holzi_lib::chat::session::{ActiveSession, ChatState};
use holzi_lib::chat::tools::{ApprovalDecision, RiskClass, Tool, ToolResult as ToolExecResult};
use holzi_lib::chat::turn::{run_turn, MAX_TOOL_ROUNDS};
use holzi_lib::identity::{
    holzi_migration_source, installation_id_path, HolziBootstrap, HOLZI_TRIGGER_VERSION,
};
use holzi_lib::storage::chat_messages::{
    self as msg_store, ChatMessage, FinishReason, MessageRole,
};
use holzi_lib::storage::chat_threads::{self as thread_store, ChatThread};
use holzi_lib::storage::preferences::{self, PrefScope};
use holzi_lib::storage::providers::ProviderKind;
use holzi_lib::storage::query;
use holzi_lib::vault_gate::{VaultDb, VaultGate};

/// Matches the private `chat.permission_mode` key in `chat/commands.rs`
/// (data-model.md) — there is no dedicated get/set command, only the
/// generic preferences store, so tests write it directly.
const PREF_PERMISSION_MODE: &str = "chat.permission_mode";

pub fn set_permission_mode(db: &Database, mode: &str) {
    db.write(|tx| {
        preferences::insert_or_update(tx, PrefScope::Vault, PREF_PERMISSION_MODE, mode).map(|_| ())
    })
    .unwrap();
}

// Test-only dummy key: the test vault is temporary and never contains user data.
const PASSPHRASE: &str = "chat-tool-loop";

fn make_config(db_path: PathBuf, installation_id: PathBuf) -> DatabaseConfig {
    DatabaseConfig {
        path: db_path,
        key: SqlCipherKey::new(PASSPHRASE),
        create_if_missing: true,
        bootstrap: Arc::new(HolziBootstrap::new(installation_id).with_alias("test")),
        signature_provider: Arc::new(NoopSignatureProvider),
        migration_source: holzi_migration_source(),
        trigger_version: HOLZI_TRIGGER_VERSION,
        max_transaction_bytes: haex_crdt::MAX_CRDT_TRANSACTION_BYTES,
        max_value_bytes: haex_crdt::MAX_VALUE_BYTES,
    }
}

pub fn open_db() -> Database {
    let tmp = tempfile::tempdir().expect("tmp dir");
    let db_path = tmp.path().join("vault.db");
    let installation_id_file = installation_id_path(tmp.path());
    let db = Database::open(make_config(db_path, installation_id_file)).expect("genesis open");
    std::mem::forget(tmp);
    db
}

/// Seeds a thread with a single persisted `user` message and returns its id.
pub fn seed_thread(db: &Database, thread_id: Uuid, user_message_id: Uuid) {
    db.write(|tx| {
        thread_store::insert_thread(
            tx,
            &ChatThread {
                id: thread_id,
                title: "test thread".to_string(),
                last_provider_id: None,
                last_model_id: None,
                created_at: 0,
                updated_at: 0,
            },
        )?;
        msg_store::insert_message(
            tx,
            &ChatMessage {
                id: user_message_id,
                thread_id,
                parent_id: None,
                role: MessageRole::User,
                content: "hi".to_string(),
                provider_id: None,
                model_id: None,
                prompt_tokens: None,
                completion_tokens: None,
                finish_reason: Some(FinishReason::Complete),
                created_at: 0,
                idempotency_key: None,
                tool_name: None,
                tool_call_id: None,
                tool_input: None,
                tool_is_error: None,
                tool_source: None,
                autonomy_mode: None,
                tool_origin: None,
            },
        )?;
        Ok(())
    })
    .unwrap();
}

/// Scripted adapter: each `stream_chat` call pops the next `Vec` of
/// pre-baked chunks off the front and streams them in order. Panics if
/// asked for more steps than were scripted — that is a test-authoring bug,
/// not a case to handle gracefully.
pub struct StubAdapter {
    steps: StdMutex<VecDeque<Vec<Result<StreamChunk, StreamError>>>>,
    requests: Arc<StdMutex<Vec<ChatRequest>>>,
}

impl StubAdapter {
    pub fn new(steps: Vec<Vec<Result<StreamChunk, StreamError>>>) -> Self {
        Self {
            steps: StdMutex::new(steps.into()),
            requests: Arc::default(),
        }
    }

    /// Like `new`, plus a log of every `ChatRequest` the adapter receives, in order.
    pub fn recording(
        steps: Vec<Vec<Result<StreamChunk, StreamError>>>,
    ) -> (Self, Arc<StdMutex<Vec<ChatRequest>>>) {
        let adapter = Self::new(steps);
        let log = adapter.requests.clone();
        (adapter, log)
    }

    /// An adapter that always answers with the same tool call, forever —
    /// used to exercise the round limit (T011).
    pub fn always_calls_tool(name: &str) -> Self {
        let mut steps = Vec::new();
        for _ in 0..(MAX_TOOL_ROUNDS + 2) {
            steps.push(vec![Ok(StreamChunk::ToolCalls(vec![LlmToolCall {
                id: Uuid::new_v4().to_string(),
                name: name.to_string(),
                input: serde_json::json!({}),
            }]))]);
        }
        Self::new(steps)
    }
}

#[async_trait]
impl ProviderAdapter for StubAdapter {
    async fn list_models(&self) -> Result<Vec<ProviderModel>, AdapterError> {
        Ok(Vec::new())
    }

    async fn stream_chat(&self, req: ChatRequest) -> Result<AdapterStream, AdapterError> {
        self.requests.lock().unwrap().push(req);
        let chunks = self
            .steps
            .lock()
            .unwrap()
            .pop_front()
            .expect("stub adapter ran out of scripted steps");
        let (tx, rx) = tokio::sync::mpsc::unbounded_channel();
        let task = tokio::spawn(async move {
            for c in chunks {
                if tx.send(c).is_err() {
                    return;
                }
            }
        });
        Ok(AdapterStream::new(rx, task.abort_handle()))
    }
}

/// A trivial in-test tool. `behavior` controls whether it succeeds or
/// reports a tool-level error, without ever panicking either way.
pub struct ScriptedTool {
    pub name: &'static str,
    pub risk_class: RiskClass,
    pub fails: bool,
}

#[async_trait]
impl Tool for ScriptedTool {
    fn name(&self) -> &str {
        self.name
    }

    fn description(&self) -> &str {
        "test-only scripted tool"
    }

    fn source(&self) -> &'static str {
        "cli"
    }

    fn input_schema(&self) -> Value {
        serde_json::json!({ "type": "object" })
    }

    fn risk_class(&self) -> RiskClass {
        self.risk_class
    }

    async fn execute(&self, input: Value, _cancel: CancellationToken) -> ToolExecResult {
        if self.fails {
            ToolExecResult::error("scripted failure")
        } else {
            ToolExecResult::ok(format!("scripted result for {input}"))
        }
    }
}

pub fn base_request() -> ChatRequest {
    ChatRequest {
        sampling: Default::default(),
        model_id: "stub-model".to_string(),
        thread_id: None,
        system_prompt: None,
        messages: Vec::new(),
        reasoning_requested: false,
        max_new_tokens: None,
        tools: vec![ToolSpec {
            name: "echo".to_string(),
            description: "echoes".to_string(),
            input_schema: serde_json::json!({ "type": "object" }),
        }],
        autonomy_mode: Default::default(),
        reasoning_option: Default::default(),
        capabilities: Default::default(),
    }
}

pub async fn session_with(adapter: StubAdapter) -> ActiveSession {
    ActiveSession {
        model_id: "stub-model".to_string(),
        provider_id: None,
        provider_kind: ProviderKind::Local,
        adapter: Arc::new(adapter),
        tokenizer_repo: String::new(),
        context_window: None,
    }
}

/// Spawns `run_turn` on its own task so the caller can respond to
/// `tool-permission-request` events while it is still awaiting them, and
/// returns every emitted `(event, payload)` in arrival order via a channel
/// instead of a `Vec` (unlike [`run_scripted_turn`], nothing here can wait
/// for the whole turn to finish before observing events). Stashes a fresh
/// `CancellationToken` into `chat_state.turn_cancellation` first, mirroring
/// what `send_message` does in production, so a test can call
/// `abort_turn(&chat_state)` exactly like a real `abort_current_generation`
/// invocation (T032).
#[allow(clippy::too_many_arguments)]
pub fn spawn_turn(
    db: Database,
    chat_state: Arc<ChatState>,
    session: ActiveSession,
    thread_id: Uuid,
    user_message_id: Uuid,
    assistant_message_id: Uuid,
    request: ChatRequest,
    stream: AdapterStream,
) -> (
    tokio::task::JoinHandle<()>,
    tokio::sync::mpsc::UnboundedReceiver<(String, Value)>,
) {
    let cancel_token = CancellationToken::new();
    *chat_state.turn_cancellation.lock().unwrap() = Some(cancel_token.clone());

    let (tx, rx) = tokio::sync::mpsc::unbounded_channel();
    let handle = tokio::spawn(async move {
        let mut emit = move |name: &'static str, payload: Value| {
            let _ = tx.send((name.to_string(), payload));
        };
        run_turn(
            &vault_db(&db),
            &chat_state,
            &session,
            thread_id,
            user_message_id,
            assistant_message_id,
            request,
            stream,
            0,
            cancel_token,
            &mut emit,
        )
        .await;
    });
    (handle, rx)
}

/// The tracked handle `run_turn` writes through, on a gate of its own.
pub fn vault_db(db: &Database) -> VaultDb {
    VaultGate::new()
        .vault_db(Arc::new(db.clone()))
        .expect("open gate")
}

pub fn extract_request_id(payload: &Value) -> Uuid {
    payload["requestId"]
        .as_str()
        .expect("requestId is a string")
        .parse()
        .expect("requestId is a uuid")
}

/// Resolves a pending approval exactly like the `respond_tool_permission`
/// Tauri command would — that command is not itself callable without a
/// live `AppHandle`/`State`, so tests exercise the same underlying
/// `ChatState.pending_tool_approvals` mechanism directly.
pub fn respond(chat_state: &ChatState, request_id: Uuid, decision: ApprovalDecision) {
    let sender = chat_state
        .pending_tool_approvals
        .lock()
        .unwrap()
        .remove(&request_id)
        .expect("pending request must exist");
    let _ = sender.send(decision);
}

/// Runs a turn end-to-end against a scripted adapter/tool and returns the
/// persisted rows (oldest first) plus every emitted `(event, payload)`.
pub async fn run_scripted_turn(
    db: &Database,
    chat_state: &ChatState,
    session: &ActiveSession,
    thread_id: Uuid,
    user_message_id: Uuid,
    assistant_message_id: Uuid,
) -> (Vec<ChatMessage>, Vec<(String, Value)>) {
    run_scripted_turn_with(
        db,
        chat_state,
        session,
        thread_id,
        user_message_id,
        assistant_message_id,
        base_request(),
    )
    .await
}

/// [`run_scripted_turn`] with a request of the test's own (capabilities, tools).
pub async fn run_scripted_turn_with(
    db: &Database,
    chat_state: &ChatState,
    session: &ActiveSession,
    thread_id: Uuid,
    user_message_id: Uuid,
    assistant_message_id: Uuid,
    request: ChatRequest,
) -> (Vec<ChatMessage>, Vec<(String, Value)>) {
    let stream = session.adapter.stream_chat(request.clone()).await.unwrap();

    let mut events: Vec<(String, Value)> = Vec::new();
    let mut emit = |name: &'static str, payload: Value| {
        events.push((name.to_string(), payload));
    };

    run_turn(
        &vault_db(db),
        chat_state,
        session,
        thread_id,
        user_message_id,
        assistant_message_id,
        request,
        stream,
        0,
        CancellationToken::new(),
        &mut emit,
    )
    .await;

    let rows = query::read(db, |r| msg_store::list_messages(r, thread_id)).unwrap();
    (rows, events)
}
