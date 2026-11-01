//! Integration tests for tasks.md T012 (User Story 1, `Ungated` mode):
//! both vendors' native full-autonomy mechanism is actually requested, no
//! bridge/approval callback ever fires, and — critically — 007's existing
//! host-isolation guarantee (`CLAUDE_CONFIG_DIR`/`CODEX_HOME` + disposable
//! `cwd`) survives the same `build_command`/`thread/start` edit that adds
//! the new permission flag/field (spec FR-011).

#![cfg(unix)]

use std::collections::HashMap;
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::Path;
use std::sync::{Arc, Mutex};

use serde_json::Value;
use tokio::sync::mpsc;

use holzi_lib::adapters::cli_delegate::autonomy::AutonomyMode;
use holzi_lib::adapters::cli_delegate::{
    CliDelegateAdapter, DelegateChatContext, DelegateVendor, EventEmitter, PendingToolApprovals,
};
use holzi_lib::adapters::{ChatMessage, ChatRequest, ChatRole, ProviderAdapter, StreamChunk};

fn write_stub(dir: &Path, name: &str, script: &str) -> std::path::PathBuf {
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

fn base_request(model_id: &str, autonomy_mode: AutonomyMode) -> ChatRequest {
    ChatRequest {
        sampling: Default::default(),
        model_id: model_id.to_string(),
        thread_id: None,
        system_prompt: None,
        messages: vec![ChatMessage {
            role: ChatRole::User,
            attachments: Vec::new(),
            content: "hi".to_string(),
        }],
        reasoning_requested: false,
        max_new_tokens: None,
        tools: Vec::new(),
        autonomy_mode,
        reasoning_option: Default::default(),
        capabilities: Default::default(),
    }
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
async fn ungated_claude_bypasses_permissions_and_keeps_host_isolation() {
    let dir = tempfile::tempdir().expect("tempdir");
    let argv_marker = dir.path().join("argv.txt");
    let env_marker = dir.path().join("env.txt");
    let stub = write_stub(
        dir.path(),
        "fake-claude",
        &format!(
            r#"#!/bin/sh
echo "$@" > {argv:?}
{{ echo "CLAUDE_CONFIG_DIR=$CLAUDE_CONFIG_DIR"; echo "PWD=$(pwd)"; }} > {env:?}
cat >/dev/null
cat <<'EOF'
{{"type":"stream_event","event":{{"type":"content_block_delta","delta":{{"type":"text_delta","text":"ok"}}}}}}
{{"type":"result","is_error":false,"subtype":"success","result":"ok","usage":{{"input_tokens":1,"output_tokens":1}}}}
EOF
"#,
            argv = argv_marker.to_string_lossy(),
            env = env_marker.to_string_lossy(),
        ),
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
            database: None,
            holzi: None,
        }),
    );

    let stream = adapter
        .stream_chat(base_request("claude-delegate", AutonomyMode::Ungated))
        .await
        .expect("stream_chat should start");
    drain_to_done(stream).await;

    assert!(
        events.try_recv().is_err(),
        "Ungated must never emit tool-permission-request"
    );

    let argv = fs::read_to_string(&argv_marker).expect("stub should have recorded argv");
    assert!(
        argv.contains("bypassPermissions"),
        "expected --permission-mode bypassPermissions, got: {argv}"
    );
    assert!(
        !argv.contains("--mcp-config"),
        "Ungated must omit --mcp-config, got: {argv}"
    );
    assert!(
        !argv.contains("--permission-prompt-tool"),
        "Ungated must omit --permission-prompt-tool, got: {argv}"
    );

    let env = fs::read_to_string(&env_marker).expect("stub should have recorded env");
    let config_dir = env
        .lines()
        .find_map(|l| l.strip_prefix("CLAUDE_CONFIG_DIR="))
        .expect("CLAUDE_CONFIG_DIR line");
    let pwd = env
        .lines()
        .find_map(|l| l.strip_prefix("PWD="))
        .expect("PWD line");
    assert!(
        !config_dir.is_empty(),
        "host isolation must still set CLAUDE_CONFIG_DIR under Ungated"
    );
    assert_eq!(
        pwd, config_dir,
        "the disposable cwd must still equal CLAUDE_CONFIG_DIR under Ungated"
    );
    assert_ne!(
        config_dir, "",
        "CLAUDE_CONFIG_DIR must not fall back to the real home directory"
    );
}

#[tokio::test]
async fn ungated_codex_sets_never_approval_policy_and_keeps_host_isolation() {
    let dir = tempfile::tempdir().expect("tempdir");
    let params_marker = dir.path().join("thread_start_params.json");
    let stub = write_stub(
        dir.path(),
        "fake-codex",
        &format!(
            r#"#!/usr/bin/env python3
import json, os, sys

def read():
    line = sys.stdin.readline()
    return json.loads(line) if line else None

def send(msg):
    sys.stdout.write(json.dumps(msg) + "\n")
    sys.stdout.flush()

req = read()  # initialize
send({{"jsonrpc": "2.0", "id": req["id"], "result": {{}}}})
req = read()  # thread/start
with open({marker:?}, "w") as f:
    json.dump({{"params": req["params"], "codex_home": os.environ.get("CODEX_HOME", "")}}, f)
send({{"jsonrpc": "2.0", "id": req["id"], "result": {{"thread": {{"id": "fake-thread"}}}}}})
req = read()  # turn/start
send({{"jsonrpc": "2.0", "id": req["id"], "result": {{}}}})

send({{"jsonrpc": "2.0", "method": "item/agentMessage/delta", "params": {{"delta": "ok"}}}})
send({{"jsonrpc": "2.0", "method": "turn/completed", "params": {{}}}})
sys.stdin.readline()
"#,
            marker = params_marker.to_str().unwrap(),
        ),
    );

    let pending: PendingToolApprovals = Arc::new(Mutex::new(HashMap::new()));
    let (emit, mut events) = channel_emitter();
    let adapter = CliDelegateAdapter::new(
        DelegateVendor::Codex,
        b"fake-auth".to_vec(),
        stub.to_str().unwrap().to_string(),
        Some(DelegateChatContext {
            children: Default::default(),
            pending_tool_approvals: pending,
            emit,
            database: None,
            holzi: None,
        }),
    );

    let stream = adapter
        .stream_chat(base_request("codex-delegate", AutonomyMode::Ungated))
        .await
        .expect("stream_chat should start");
    drain_to_done(stream).await;

    assert!(
        events.try_recv().is_err(),
        "Ungated must never emit tool-permission-request"
    );

    let recorded = fs::read_to_string(&params_marker).expect("stub should have recorded params");
    let recorded: Value = serde_json::from_str(&recorded).expect("recorded params is JSON");
    assert_eq!(recorded["params"]["approvalPolicy"], "never");
    assert_eq!(recorded["params"]["sandbox"], "workspace-write");
    let codex_home = recorded["codex_home"]
        .as_str()
        .expect("codex_home recorded");
    assert!(
        !codex_home.is_empty(),
        "host isolation must still set CODEX_HOME under Ungated"
    );
}
