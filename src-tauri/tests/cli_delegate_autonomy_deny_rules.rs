//! Integration tests for tasks.md T028/T029 (User Story 3): a persisted
//! deny rule actually blocks the documented vendor signal it targets under
//! `gated-permissive`, including the Codex file-change fail-closed
//! asymmetry (spec FR-015).

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
use holzi_lib::storage::preferences::{self, PrefScope};
use holzi_lib::storage::query;
use holzi_lib::vault_gate::VaultGate;

const PASSPHRASE: &str = "cli-delegate-autonomy-deny-rules";
const PREF_DENY_RULES: &str = "cli_delegate.deny_rules";

fn open_vault_with_deny_rules(dir: &Path, categories: &[&str]) -> Database {
    let installation_id = installation_id_path(dir);
    let db = Database::open(DatabaseConfig {
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
    .expect("vault open");
    let json = serde_json::to_string(categories).unwrap();
    db.write(|tx| preferences::insert_or_update(tx, PrefScope::Vault, PREF_DENY_RULES, &json))
        .expect("set deny rules");
    db
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

fn codex_stub_script(method: &str, params_json: &str, marker: &Path) -> String {
    format!(
        r#"#!/usr/bin/env python3
import json, sys

def read():
    line = sys.stdin.readline()
    return json.loads(line) if line else None

def send(msg):
    sys.stdout.write(json.dumps(msg) + "\n")
    sys.stdout.flush()

req = read()  # initialize
send({{"jsonrpc": "2.0", "id": req["id"], "result": {{}}}})
req = read()  # thread/start
send({{"jsonrpc": "2.0", "id": req["id"], "result": {{"thread": {{"id": "fake-thread"}}}}}})
req = read()  # turn/start
send({{"jsonrpc": "2.0", "id": req["id"], "result": {{}}}})

send({{"jsonrpc": "2.0", "id": 100, "method": "{method}", "params": {params}}})
reply = read()
with open({marker:?}, "w") as f:
    f.write(json.dumps(reply["result"]))

send({{"jsonrpc": "2.0", "method": "item/agentMessage/delta", "params": {{"delta": "ok"}}}})
send({{"jsonrpc": "2.0", "method": "turn/completed", "params": {{}}}})
sys.stdin.readline()
"#,
        method = method,
        params = params_json,
        marker = marker.to_str().unwrap(),
    )
}

async fn run_codex_turn(
    db: &Arc<Database>,
    stub: &Path,
) -> (mpsc::UnboundedReceiver<(String, Value)>, Uuid) {
    let pending: PendingToolApprovals = Arc::new(Mutex::new(HashMap::new()));
    let (emit, events) = channel_emitter();
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
                    .vault_db(Arc::clone(db))
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
    (events, thread_id)
}

#[tokio::test]
async fn network_access_deny_rule_blocks_a_codex_command_with_network_context() {
    let dir = tempfile::tempdir().expect("tempdir");
    let db = Arc::new(open_vault_with_deny_rules(dir.path(), &["network_access"]));
    let marker = dir.path().join("decision.txt");
    let stub = write_stub(
        dir.path(),
        "fake-codex",
        &codex_stub_script(
            "item/commandExecution/requestApproval",
            r#"{"command": "curl https://example.com", "cwd": "/tmp", "networkApprovalContext": {"host": "example.com", "protocol": "https"}}"#,
            &marker,
        ),
    );

    let (_events, thread_id) = run_codex_turn(&db, &stub).await;

    let recorded = fs::read_to_string(&marker).expect("stub should have recorded the reply");
    let recorded: Value = serde_json::from_str(&recorded).expect("recorded reply is JSON");
    assert_eq!(
        recorded["decision"], "decline",
        "network_access must deny a networked command"
    );

    let messages = query::read(&db, |r| list_messages(r, thread_id)).expect("list_messages");
    let result_row = messages
        .iter()
        .find(|m| m.role == MessageRole::ToolResult)
        .expect("a tool_result audit row must exist");
    assert_eq!(result_row.tool_is_error, Some(true));
}

#[tokio::test]
async fn workspace_escape_deny_rule_fails_closed_on_a_codex_file_change() {
    let dir = tempfile::tempdir().expect("tempdir");
    let db = Arc::new(open_vault_with_deny_rules(
        dir.path(),
        &["workspace_escape"],
    ));
    let marker = dir.path().join("decision.txt");
    let stub = write_stub(
        dir.path(),
        "fake-codex",
        &codex_stub_script(
            "item/fileChange/requestApproval",
            r#"{"itemId": "item-1", "reason": "edit"}"#,
            &marker,
        ),
    );

    run_codex_turn(&db, &stub).await;

    let recorded = fs::read_to_string(&marker).expect("stub should have recorded the reply");
    let recorded: Value = serde_json::from_str(&recorded).expect("recorded reply is JSON");
    assert_eq!(
        recorded["decision"], "decline",
        "FR-015: an unevaluable Codex file-change approval must fail closed \
         when workspace_escape is enabled, not silently pass through"
    );
}

/// tasks.md T016/spec FR-010: a malformed persisted deny-rule value must
/// fail closed to `Deny`, not silently become an empty (fully permissive)
/// rule set — this synchronous path bypasses the `Ask`-branch's own
/// `receiver.await.unwrap_or(Deny)` safety net entirely, so it needs its
/// own.
#[tokio::test]
async fn malformed_deny_rules_preference_fails_closed_to_deny() {
    let dir = tempfile::tempdir().expect("tempdir");
    let db = Arc::new(open_vault_with_deny_rules(dir.path(), &[]));
    db.write(|tx| {
        preferences::insert_or_update(tx, PrefScope::Vault, PREF_DENY_RULES, "not valid json")
    })
    .expect("corrupt the deny rules preference");

    let marker = dir.path().join("decision.txt");
    let stub = write_stub(
        dir.path(),
        "fake-codex",
        &codex_stub_script(
            "item/commandExecution/requestApproval",
            r#"{"command": "ls", "cwd": "/tmp"}"#,
            &marker,
        ),
    );

    run_codex_turn(&db, &stub).await;

    let recorded = fs::read_to_string(&marker).expect("stub should have recorded the reply");
    let recorded: Value = serde_json::from_str(&recorded).expect("recorded reply is JSON");
    assert_eq!(
        recorded["decision"], "decline",
        "a malformed deny-rules preference must fail closed to Deny, never Allow"
    );
}

#[tokio::test]
async fn network_access_deny_rule_allows_a_codex_command_without_network_context() {
    let dir = tempfile::tempdir().expect("tempdir");
    let db = Arc::new(open_vault_with_deny_rules(dir.path(), &["network_access"]));
    let marker = dir.path().join("decision.txt");
    let stub = write_stub(
        dir.path(),
        "fake-codex",
        &codex_stub_script(
            "item/commandExecution/requestApproval",
            r#"{"command": "ls", "cwd": "/tmp"}"#,
            &marker,
        ),
    );

    run_codex_turn(&db, &stub).await;

    let recorded = fs::read_to_string(&marker).expect("stub should have recorded the reply");
    let recorded: Value = serde_json::from_str(&recorded).expect("recorded reply is JSON");
    assert_eq!(
        recorded["decision"], "accept",
        "a non-networked command must stay allowed even with network_access enabled"
    );
}
