//! Integration tests for tasks.md T013 (User Story 1, `GatedPermissive`
//! mode): with an empty deny-rule set, no `tool-permission-request` event
//! ever fires for either vendor — but unlike `Ungated`, the approval
//! bridge genuinely runs. Its own audit-trail persistence (spec
//! FR-005/US2, `approval_bridge.rs::persist_gated_permissive_record`) is
//! the observable, external proof of that: `Ungated` writes nothing per
//! call, so a persisted `tool_call`/`tool_result` row pair here is what
//! actually distinguishes "the bridge ran and decided Allow" from
//! "nothing happened". Exercised through Codex (its approval callback
//! runs directly over the same stdio `codex.rs` already owns, no
//! child-process/socket hop needed, per `cli_delegate_approval.rs`'s own
//! precedent) — Claude's equivalent round trip needs a real MCP-client
//! simulation and is covered instead by `cli_delegate_approval.rs`.

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
use holzi_lib::storage::query;
use holzi_lib::vault_gate::VaultGate;

const PASSPHRASE: &str = "cli-delegate-autonomy-gated-permissive";

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

fn write_stub(dir: &Path, name: &str, script: &str) -> PathBuf {
    let path = dir.join(name);
    fs::write(&path, script).expect("write stub script");
    let mut perms = fs::metadata(&path).unwrap().permissions();
    perms.set_mode(0o755);
    fs::set_permissions(&path, perms).expect("chmod stub script");
    path
}

fn channel_emitter() -> (EventEmitter, mpsc::UnboundedReceiver<(String, Value)>) {
    let (tx, rx) = mpsc::unbounded_channel();
    let emit: EventEmitter = Arc::new(move |event: &str, payload: Value| {
        let _ = tx.send((event.to_string(), payload));
    });
    (emit, rx)
}

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
async fn claude_gated_permissive_with_no_deny_rules_emits_no_approval_prompt() {
    let dir = tempfile::tempdir().expect("tempdir");
    let db = Arc::new(open_vault(dir.path()));
    // No `cli_delegate.deny_rules` preference written — the documented
    // fully-permissive empty-set edge case.

    let stub = write_stub(
        dir.path(),
        "fake-claude",
        r#"#!/bin/sh
cat >/dev/null
cat <<'EOF'
{"type":"stream_event","event":{"type":"content_block_delta","delta":{"type":"text_delta","text":"ok"}}}
{"type":"result","is_error":false,"subtype":"success","result":"ok","usage":{"input_tokens":1,"output_tokens":1}}
EOF
"#,
    );

    let pending: PendingToolApprovals = Arc::new(Mutex::new(HashMap::new()));
    let (emit, mut events) = channel_emitter();
    let adapter = CliDelegateAdapter::new(
        DelegateVendor::Claude,
        b"fake-oauth-token".to_vec(),
        stub.to_string_lossy().into_owned(),
        Some(DelegateChatContext {
            children: Default::default(),
            pending_tool_approvals: pending,
            emit,
            database: Some(VaultGate::new().vault_db(db).expect("open gate")),
            holzi: None,
        }),
    );

    let stream = adapter
        .stream_chat(ChatRequest {
            sampling: Default::default(),
            model_id: "claude-delegate".to_string(),
            thread_id: Some(Uuid::new_v4()),
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

    assert!(
        events.try_recv().is_err(),
        "GatedPermissive must never emit tool-permission-request"
    );
}

#[tokio::test]
async fn codex_gated_permissive_with_no_deny_rules_is_silent_but_records_the_call() {
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
read()  # the reply; content unchecked here (cli_delegate_approval.rs already covers the wire shape)

send({"jsonrpc": "2.0", "method": "item/agentMessage/delta", "params": {"delta": "ok"}})
send({"jsonrpc": "2.0", "method": "turn/completed", "params": {}})
sys.stdin.readline()
"#,
    );

    let pending: PendingToolApprovals = Arc::new(Mutex::new(HashMap::new()));
    let (emit, mut events) = channel_emitter();
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

    assert!(
        events.try_recv().is_err(),
        "GatedPermissive must never emit tool-permission-request"
    );

    let messages = query::read(&db, |r| list_messages(r, thread_id)).expect("list_messages");
    let call_row = messages
        .iter()
        .find(|m| m.role == MessageRole::ToolCall)
        .expect("gated-permissive must persist a tool_call audit row even with no deny rules");
    assert_eq!(
        call_row.autonomy_mode.as_deref(),
        Some("gated_permissive"),
        "the audit row must be labeled with its autonomy mode"
    );
    let result_row = messages
        .iter()
        .find(|m| m.role == MessageRole::ToolResult)
        .expect("gated-permissive must persist a matching tool_result audit row");
    assert_eq!(
        result_row.tool_is_error,
        Some(false),
        "no deny rule => Allow"
    );
}
