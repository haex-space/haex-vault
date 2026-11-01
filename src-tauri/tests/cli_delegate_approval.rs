//! Integration tests for tasks.md T029/T030 (User Story 3): the live
//! per-tool-call approval round trip, exercised through each backend's own
//! wire protocol rather than the shared `approval_bridge.rs` unit tests
//! (`approval_bridge_tests.rs`, which only prove the in-process mechanism
//! both backends call into).
//!
//! T030 (Codex): a stub `codex app-server` sends a real
//! `item/commandExecution/requestApproval` server-request over the same
//! stdio `codex.rs` already owns (no child-process/socket hop needed on this
//! path, research.md §2) and asserts the reply is `{"decision":"accept"}`
//! after `respond_tool_permission` answers.
//!
//! T029 (Claude): a real Unix socket bound by `approval_bridge::bind_socket`/
//! `start_listener` (the exact code `claude.rs` sets up per invocation), with
//! the actual built `holzi` binary spawned as the bridge child process via
//! `--internal-cli-delegate-approval-bridge` (T034/T035) standing in for what
//! `claude` itself would spawn per `--mcp-config`. The test plays the role of
//! `claude`'s own MCP client, since that client's behavior is Anthropic's
//! code, not holzi's — what's under test here is holzi's server/bridge side.

#![cfg(unix)]

use std::collections::HashMap;
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::Path;
use std::process::Stdio;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use serde_json::{json, Value};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::process::Command;
use tokio::sync::mpsc;
use uuid::Uuid;

use holzi_lib::adapters::cli_delegate::approval_bridge::{bind_socket, start_listener};
use holzi_lib::adapters::cli_delegate::autonomy::AutonomyMode;
use holzi_lib::adapters::cli_delegate::{
    CliDelegateAdapter, DelegateChatContext, DelegateVendor, EventEmitter, PendingToolApprovals,
};
use holzi_lib::adapters::{ChatMessage, ChatRequest, ChatRole, ProviderAdapter, StreamChunk};
use holzi_lib::chat::tools::ApprovalDecision;

fn write_stub(dir: &Path, name: &str, script: &str) -> std::path::PathBuf {
    let path = dir.join(name);
    fs::write(&path, script).expect("write stub script");
    let mut perms = fs::metadata(&path).unwrap().permissions();
    perms.set_mode(0o755);
    fs::set_permissions(&path, perms).expect("chmod stub script");
    path
}

/// Forwards every emitted event's payload to an mpsc channel — same shape
/// `tests/common/tool_loop_fixture.rs` uses for the built-in tool loop.
fn channel_emitter() -> (EventEmitter, mpsc::UnboundedReceiver<(String, Value)>) {
    let (tx, rx) = mpsc::unbounded_channel();
    let emit: EventEmitter = Arc::new(move |event: &str, payload: Value| {
        let _ = tx.send((event.to_string(), payload));
    });
    (emit, rx)
}

#[tokio::test]
async fn codex_command_approval_round_trips_through_the_real_wire_protocol() {
    let dir = tempfile::tempdir().expect("tempdir");
    let decision_marker = dir.path().join("decision.txt");
    let stub = write_stub(
        dir.path(),
        "fake-codex",
        &format!(
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

send({{"jsonrpc": "2.0", "id": 100, "method": "item/commandExecution/requestApproval", "params": {{"command": "ls"}}}})
reply = read()
with open({marker:?}, "w") as f:
    f.write(json.dumps(reply["result"]))

send({{"jsonrpc": "2.0", "method": "item/agentMessage/delta", "params": {{"delta": "ok"}}}})
send({{"jsonrpc": "2.0", "method": "turn/completed", "params": {{}}}})
sys.stdin.readline()
"#,
            marker = decision_marker.to_str().unwrap(),
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
            pending_tool_approvals: Arc::clone(&pending),
            emit,
            database: None, // Manual mode => always Ask, forcing the live round trip
            holzi: None,
        }),
    );

    let mut stream = adapter
        .stream_chat(ChatRequest {
            sampling: Default::default(),
            model_id: "codex-delegate".to_string(),
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
            autonomy_mode: Default::default(),
            reasoning_option: Default::default(),
            capabilities: Default::default(),
        })
        .await
        .expect("stream_chat should start");

    // Drain the stream concurrently — `respond_to_server_request`'s reply
    // blocks on the same oneshot this test resolves below.
    let drain = tokio::spawn(async move {
        let mut text = String::new();
        let mut saw_done = false;
        while let Some(chunk) = stream.next().await {
            match chunk.expect("no stream error expected") {
                StreamChunk::Delta { content, .. } => text.push_str(&content),
                StreamChunk::Done { .. } => {
                    saw_done = true;
                    break;
                }
                StreamChunk::ToolCalls(_) => panic!("a delegate must never emit ToolCalls"),
                StreamChunk::AgentActivity { .. } => {}
            }
        }
        (text, saw_done)
    });

    let (event_name, payload) = tokio::time::timeout(Duration::from_secs(10), events.recv())
        .await
        .expect("tool-permission-request should arrive before timing out")
        .expect("event channel should not close");
    assert_eq!(event_name, "tool-permission-request");
    assert_eq!(payload["toolName"], "Bash");
    let request_id = payload["requestId"]
        .as_str()
        .and_then(|value| Uuid::parse_str(value).ok())
        .expect("event should carry a UUID requestId");

    let sender = pending
        .lock()
        .expect("pending lock")
        .remove(&request_id)
        .expect("a pending approval should be registered for this request");
    sender
        .send(ApprovalDecision::Allow)
        .expect("receiver still alive");

    let (text, saw_done) = tokio::time::timeout(Duration::from_secs(10), drain)
        .await
        .expect("stream should finish before timing out")
        .expect("drain task should not panic");
    assert!(saw_done);
    assert_eq!(text, "ok");

    let recorded = fs::read_to_string(&decision_marker).expect("stub should have recorded a reply");
    let recorded: Value = serde_json::from_str(&recorded).expect("recorded reply is JSON");
    assert_eq!(recorded, json!({"decision": "accept"}));
}

#[tokio::test]
async fn claude_approval_round_trips_through_the_real_socket_and_bridge_child() {
    let dir = tempfile::tempdir().expect("tempdir");
    let socket_path = dir.path().join("approve.sock");

    let pending: PendingToolApprovals = Arc::new(Mutex::new(HashMap::new()));
    let (emit, mut events) = channel_emitter();
    let context = DelegateChatContext {
        children: Default::default(),
        pending_tool_approvals: Arc::clone(&pending),
        emit,
        database: None, // Manual mode => always Ask
        holzi: None,
    };

    let listener = bind_socket(&socket_path).expect("bind approval socket");
    let listener_task = start_listener(
        listener,
        context,
        Some(Uuid::nil()),
        AutonomyMode::Standard,
        dir.path().to_path_buf(),
    );

    // The real hidden entrypoint (T034), run as the actual compiled `holzi`
    // binary — exactly what `claude.rs` points `--mcp-config` at, not a
    // simplified in-process stand-in.
    let holzi_bin = env!("CARGO_BIN_EXE_holzi");
    let mut bridge = Command::new(holzi_bin)
        .arg("--internal-cli-delegate-approval-bridge")
        .arg("--socket")
        .arg(&socket_path)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .expect("spawn the real holzi binary as the bridge child");

    let mut stdin = bridge.stdin.take().expect("piped stdin");
    let stdout = bridge.stdout.take().expect("piped stdout");
    let mut lines = BufReader::new(stdout).lines();

    async fn send(stdin: &mut tokio::process::ChildStdin, value: Value) {
        let mut bytes = serde_json::to_vec(&value).unwrap();
        bytes.push(b'\n');
        stdin.write_all(&bytes).await.unwrap();
    }
    async fn recv(lines: &mut tokio::io::Lines<BufReader<tokio::process::ChildStdout>>) -> Value {
        let line = lines
            .next_line()
            .await
            .unwrap()
            .expect("bridge should not exit early");
        serde_json::from_str(&line).unwrap()
    }

    send(
        &mut stdin,
        json!({"jsonrpc": "2.0", "id": 1, "method": "initialize", "params": {"protocolVersion": "2025-06-18"}}),
    )
    .await;
    let init_response = recv(&mut lines).await;
    assert_eq!(init_response["id"], 1);

    send(
        &mut stdin,
        json!({"jsonrpc": "2.0", "method": "notifications/initialized", "params": {}}),
    )
    .await;

    send(
        &mut stdin,
        json!({
            "jsonrpc": "2.0",
            "id": 2,
            "method": "tools/call",
            "params": {
                "name": "approve",
                "arguments": {"tool_name": "Bash", "input": {"command": "touch x"}},
            },
        }),
    )
    .await;

    // The bridge's `tools/call` reply blocks on the socket round trip, which
    // blocks on the same oneshot `respond_tool_permission` resolves — so the
    // approval event must arrive before that reply does.
    let (event_name, payload) = tokio::time::timeout(Duration::from_secs(10), events.recv())
        .await
        .expect("tool-permission-request should arrive before timing out")
        .expect("event channel should not close");
    assert_eq!(event_name, "tool-permission-request");
    assert_eq!(payload["toolName"], "Bash");
    let request_id = payload["requestId"]
        .as_str()
        .and_then(|value| Uuid::parse_str(value).ok())
        .expect("event should carry a UUID requestId");

    let sender = pending
        .lock()
        .expect("pending lock")
        .remove(&request_id)
        .expect("a pending approval should be registered for this request");
    sender
        .send(ApprovalDecision::Allow)
        .expect("receiver still alive");

    let call_response = tokio::time::timeout(Duration::from_secs(10), recv(&mut lines))
        .await
        .expect("tools/call response should arrive before timing out");
    assert_eq!(call_response["id"], 2);
    let text = call_response["result"]["content"][0]["text"]
        .as_str()
        .expect("MCP text content block");
    let behavior: Value = serde_json::from_str(text).expect("behavior payload is JSON");
    assert_eq!(behavior["behavior"], "allow");

    drop(stdin);
    let _ = bridge.kill().await;
    listener_task.abort();
}
