//! Integration tests for tasks.md T023/T024 (User Story 2): a
//! `gated-permissive` run with 2+ tool calls persists 2+ labeled audit
//! rows, while an `ungated` run persists none at all — only its final
//! assistant response, also labeled (spec FR-005/FR-006).

#![cfg(unix)]

use std::collections::HashMap;
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use haex_crdt::{Database, DatabaseConfig, NoopSignatureProvider, SqlCipherKey};
use serde_json::Value;
use tokio::sync::mpsc;
use uuid::Uuid;

use holzi_lib::adapters::cli_delegate::autonomy::AutonomyMode;
use holzi_lib::adapters::cli_delegate::{
    CliDelegateAdapter, DelegateChatContext, DelegateVendor, EventEmitter, PendingToolApprovals,
};
use holzi_lib::adapters::{ChatMessage, ChatRequest, ChatRole, ProviderAdapter, StreamChunk};
use holzi_lib::identity::{holzi_migration_source, installation_id_path, HolziBootstrap};
use holzi_lib::storage::chat_messages::{list_messages, MessageRole};
use holzi_lib::storage::chat_threads::{self, ChatThread};
use holzi_lib::storage::query;
use holzi_lib::vault_gate::VaultGate;

const PASSPHRASE: &str = "cli-delegate-autonomy-audit-trail";

/// Opens an isolated vault containing the production migration set.
fn open_vault(dir: &Path) -> Database {
    let installation_id = installation_id_path(dir);
    Database::open(DatabaseConfig {
        path: dir.join("vault.db"),
        key: SqlCipherKey::new(PASSPHRASE),
        create_if_missing: true,
        bootstrap: Arc::new(HolziBootstrap::new(installation_id).with_alias("test")),
        signature_provider: Arc::new(NoopSignatureProvider),
        migration_source: holzi_migration_source(),
        trigger_version: haex_crdt::DEFAULT_TRIGGER_VERSION,
        max_transaction_bytes: haex_crdt::MAX_CRDT_TRANSACTION_BYTES,
        max_value_bytes: haex_crdt::MAX_VALUE_BYTES,
    })
    .expect("vault open")
}

/// Writes an executable delegate stub into the test directory.
fn write_stub(dir: &Path, name: &str, script: &str) -> PathBuf {
    let path = dir.join(name);
    fs::write(&path, script).expect("write stub script");
    let mut perms = fs::metadata(&path).unwrap().permissions();
    perms.set_mode(0o755);
    fs::set_permissions(&path, perms).expect("chmod stub script");
    path
}

/// Creates an event emitter backed by an inspectable channel.
fn channel_emitter() -> (EventEmitter, mpsc::UnboundedReceiver<(String, Value)>) {
    let (tx, rx) = mpsc::unbounded_channel();
    let emit: EventEmitter = Arc::new(move |event: &str, payload: Value| {
        let _ = tx.send((event.to_string(), payload));
    });
    (emit, rx)
}

/// Consumes a delegate stream and asserts that it terminates with `Done`.
async fn drain_to_done(mut stream: holzi_lib::adapters::AdapterStream) {
    while let Some(chunk) = stream.next().await {
        match chunk.expect("no stream error expected") {
            StreamChunk::Done { .. } => return,
            StreamChunk::ToolCalls(_) => panic!("a delegate must never emit ToolCalls"),
            StreamChunk::Delta { .. } => {}
            StreamChunk::AgentActivity { .. } => {}
        }
    }
    panic!("stream ended without a Done chunk");
}

#[tokio::test]
async fn gated_permissive_with_two_tool_calls_persists_two_labeled_audit_rows() {
    let dir = tempfile::tempdir().expect("tempdir");
    let db = Arc::new(open_vault(dir.path()));

    let stub = write_stub(
        dir.path(),
        "fake-codex",
        r#"#!/usr/bin/env python3
import json, sys

def read():
    line = sys.stdin.readline()
    return json.loads(line) if line else None

def send(msg):
    sys.stdout.write(json.dumps(msg) + "\n")
    sys.stdout.flush()

req = read()  # initialize
send({"jsonrpc": "2.0", "id": req["id"], "result": {}})
req = read()  # thread/start
send({"jsonrpc": "2.0", "id": req["id"], "result": {"thread": {"id": "fake-thread"}}})
req = read()  # turn/start
send({"jsonrpc": "2.0", "id": req["id"], "result": {}})

send({"jsonrpc": "2.0", "id": 100, "method": "item/commandExecution/requestApproval", "params": {"command": "ls", "cwd": "/tmp"}})
read()
send({"jsonrpc": "2.0", "id": 101, "method": "item/commandExecution/requestApproval", "params": {"command": "pwd", "cwd": "/tmp"}})
read()

send({"jsonrpc": "2.0", "method": "item/agentMessage/delta", "params": {"delta": "ok"}})
send({"jsonrpc": "2.0", "method": "turn/completed", "params": {}})
sys.stdin.readline()
"#,
    );

    let pending: PendingToolApprovals = Arc::new(Mutex::new(HashMap::new()));
    let (emit, _events) = channel_emitter();
    let thread_id = Uuid::new_v4();
    let adapter = CliDelegateAdapter::new(
        DelegateVendor::Codex,
        b"fake-auth".to_vec(),
        stub.to_str().unwrap().to_string(),
        Some(DelegateChatContext {
            children: Default::default(),
            pending_tool_approvals: pending,
            emit,
            database: Some(
                VaultGate::new()
                    .vault_db(Arc::clone(&db))
                    .expect("open gate"),
            ),
            holzi: None,
        }),
    );

    let stream = adapter
        .stream_chat(ChatRequest {
            sampling: Default::default(),
            model_id: "codex-delegate".to_string(),
            thread_id: Some(thread_id),
            system_prompt: None,
            messages: vec![ChatMessage {
                role: ChatRole::User,
                attachments: Vec::new(),
                content: "hi".to_string(),
            }],
            reasoning_requested: false,
            max_new_tokens: None,
            tools: Vec::new(),
            autonomy_mode: AutonomyMode::GatedPermissive,
            reasoning_option: Default::default(),
            capabilities: Default::default(),
        })
        .await
        .expect("stream_chat should start");
    drain_to_done(stream).await;

    let messages = query::read(&db, |r| list_messages(r, thread_id)).expect("list_messages");
    let call_rows: Vec<_> = messages
        .iter()
        .filter(|m| m.role == MessageRole::ToolCall)
        .collect();
    assert_eq!(call_rows.len(), 2, "two tool calls => two tool_call rows");
    for row in &call_rows {
        assert_eq!(row.autonomy_mode.as_deref(), Some("gated_permissive"));
    }
    let result_rows: Vec<_> = messages
        .iter()
        .filter(|m| m.role == MessageRole::ToolResult)
        .collect();
    assert_eq!(
        result_rows.len(),
        2,
        "two tool calls => two tool_result rows"
    );
}

/// This adapter-level test proves the structural half of FR-006 — an
/// `Ungated` delegate stream itself never produces a tool-call-shaped
/// chunk, so nothing routes into the turn loop's tool-call persistence.
/// The other half — the persisted final assistant row actually carrying
/// `autonomy_mode: "ungated"` — is `chat/turn/mod.rs::finish_turn`'s own
/// concern, unit-tested directly via `autonomy_mode_label` in
/// `chat/turn/persist_tests.rs` (building a full `TurnRunner` here would
/// duplicate `tests/chat_tool_loop_permissions.rs`'s fixture for no new
/// coverage).
#[tokio::test]
async fn ungated_delegate_stream_never_produces_tool_call_rows() {
    let dir = tempfile::tempdir().expect("tempdir");
    let db = Arc::new(open_vault(dir.path()));
    let thread_id = Uuid::new_v4();
    db.write(|tx| {
        chat_threads::insert_thread(
            tx,
            &ChatThread {
                id: thread_id,
                title: "test".to_string(),
                last_provider_id: None,
                last_model_id: None,
                created_at: 0,
                updated_at: 0,
            },
        )
    })
    .expect("insert thread");

    let stub = write_stub(
        dir.path(),
        "fake-codex",
        r#"#!/usr/bin/env python3
import json, sys

def read():
    line = sys.stdin.readline()
    return json.loads(line) if line else None

def send(msg):
    sys.stdout.write(json.dumps(msg) + "\n")
    sys.stdout.flush()

req = read()  # initialize
send({"jsonrpc": "2.0", "id": req["id"], "result": {}})
req = read()  # thread/start
send({"jsonrpc": "2.0", "id": req["id"], "result": {"thread": {"id": "fake-thread"}}})
req = read()  # turn/start
send({"jsonrpc": "2.0", "id": req["id"], "result": {}})

send({"jsonrpc": "2.0", "method": "item/agentMessage/delta", "params": {"delta": "ok"}})
send({"jsonrpc": "2.0", "method": "turn/completed", "params": {}})
sys.stdin.readline()
"#,
    );

    let pending: PendingToolApprovals = Arc::new(Mutex::new(HashMap::new()));
    let (emit, _events) = channel_emitter();
    let adapter = CliDelegateAdapter::new(
        DelegateVendor::Codex,
        b"fake-auth".to_vec(),
        stub.to_str().unwrap().to_string(),
        Some(DelegateChatContext {
            children: Default::default(),
            pending_tool_approvals: pending,
            emit,
            database: Some(
                VaultGate::new()
                    .vault_db(Arc::clone(&db))
                    .expect("open gate"),
            ),
            holzi: None,
        }),
    );

    // `spawn_codex_app_server` itself never writes chat_messages rows —
    // that is the turn loop's job (`chat/turn/mod.rs::finish_turn`), which
    // this unit-level test does not run. This test only proves the
    // delegate stream itself never surfaces a tool-call-shaped chunk (the
    // structural half of FR-006); the storage half — the persisted
    // assistant row carrying `autonomy_mode` — is covered directly by
    // `chat/turn/persist.rs`'s own `autonomy_mode_label` unit test path
    // via `cargo test --lib`, since building a full `TurnRunner` here
    // would duplicate `tests/chat_tool_loop_permissions.rs`'s fixture.
    let stream = adapter
        .stream_chat(ChatRequest {
            sampling: Default::default(),
            model_id: "codex-delegate".to_string(),
            thread_id: Some(thread_id),
            system_prompt: None,
            messages: vec![ChatMessage {
                role: ChatRole::User,
                attachments: Vec::new(),
                content: "hi".to_string(),
            }],
            reasoning_requested: false,
            max_new_tokens: None,
            tools: Vec::new(),
            autonomy_mode: AutonomyMode::Ungated,
            reasoning_option: Default::default(),
            capabilities: Default::default(),
        })
        .await
        .expect("stream_chat should start");
    drain_to_done(stream).await;

    let messages = query::read(&db, |r| list_messages(r, thread_id)).expect("list_messages");
    assert!(
        messages
            .iter()
            .all(|m| m.role != MessageRole::ToolCall && m.role != MessageRole::ToolResult),
        "an ungated delegate stream must never itself produce tool_call/tool_result rows"
    );
}
