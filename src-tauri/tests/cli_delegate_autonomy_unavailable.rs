//! Integration tests for tasks.md T015 (User Story 1): an installed CLI
//! too old to support `Ungated` rejects the new flag/field, and that
//! rejection must surface as `AdapterError::Unavailable` — synchronously,
//! before any stream is returned — rather than a generic spawn/stream
//! failure or a silent fallback to `Standard` (spec FR-013/SC-006). The
//! command boundary's mapping of `AdapterError::Unavailable` to
//! `HolziError::InvalidInput` is 007's own existing, unrelated-to-this-
//! feature behavior and is not re-tested here.

#![cfg(unix)]

use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};

use holzi_lib::adapters::cli_delegate::autonomy::AutonomyMode;
use holzi_lib::adapters::cli_delegate::{CliDelegateAdapter, DelegateVendor};
use holzi_lib::adapters::{AdapterError, ChatMessage, ChatRequest, ChatRole, ProviderAdapter};

fn write_stub(dir: &Path, name: &str, script: &str) -> PathBuf {
    let path = dir.join(name);
    fs::write(&path, script).expect("write stub script");
    let mut perms = fs::metadata(&path).unwrap().permissions();
    perms.set_mode(0o755);
    fs::set_permissions(&path, perms).expect("chmod stub script");
    path
}

#[tokio::test]
async fn claude_ungated_reports_unavailable_when_the_cli_rejects_the_flag() {
    let dir = tempfile::tempdir().expect("tempdir");
    // Mimics an old `claude` binary's own argument parser: it rejects the
    // unrecognized `bypassPermissions` value and exits before emitting any
    // `stream-json` line at all — this is what forces the peek-ahead in
    // `claude.rs::spawn_claude_invocation` to actually classify a real
    // early exit rather than a synthetic one.
    let stub = write_stub(
        dir.path(),
        "fake-claude",
        r#"#!/bin/sh
for arg in "$@"; do
  if [ "$arg" = "bypassPermissions" ]; then
    echo "error: option '--permission-mode <mode>' argument 'bypassPermissions' is invalid." >&2
    exit 1
  fi
done
cat <<'EOF'
{"type":"result","is_error":false,"subtype":"success","result":"ok"}
EOF
"#,
    );

    let adapter = CliDelegateAdapter::new(
        DelegateVendor::Claude,
        b"fake-oauth-token".to_vec(),
        stub.to_string_lossy().into_owned(),
        Some(holzi_lib::adapters::cli_delegate::DelegateChatContext {
            children: Default::default(),
            pending_tool_approvals: std::sync::Arc::new(std::sync::Mutex::new(
                std::collections::HashMap::new(),
            )),
            emit: std::sync::Arc::new(|_: &str, _: serde_json::Value| {}),
            database: None,
            holzi: None,
        }),
    );

    let result = adapter
        .stream_chat(ChatRequest {
            sampling: Default::default(),
            model_id: "claude-delegate".to_string(),
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
            autonomy_mode: AutonomyMode::Ungated,
            reasoning_option: Default::default(),
            capabilities: Default::default(),
        })
        .await;

    match result {
        Err(AdapterError::Unavailable { reason }) => {
            assert!(
                reason.contains("claude"),
                "reason should name the vendor: {reason}"
            );
            assert!(
                reason.contains("ungated"),
                "reason should name the requested mode: {reason}"
            );
        }
        Ok(_) => panic!("expected AdapterError::Unavailable, got Ok(stream)"),
        Err(other) => panic!("expected AdapterError::Unavailable, got: {other}"),
    }
}

#[tokio::test]
async fn codex_ungated_reports_unavailable_when_the_app_server_rejects_the_field() {
    let dir = tempfile::tempdir().expect("tempdir");
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
send({"jsonrpc": "2.0", "id": req["id"], "error": {"code": -32602, "message": "unknown field `approvalPolicy`"}})
sys.stdin.readline()
"#,
    );

    let adapter = CliDelegateAdapter::new(
        DelegateVendor::Codex,
        b"fake-auth".to_vec(),
        stub.to_str().unwrap().to_string(),
        Some(holzi_lib::adapters::cli_delegate::DelegateChatContext {
            children: Default::default(),
            pending_tool_approvals: std::sync::Arc::new(std::sync::Mutex::new(
                std::collections::HashMap::new(),
            )),
            emit: std::sync::Arc::new(|_: &str, _: serde_json::Value| {}),
            database: None,
            holzi: None,
        }),
    );

    let result = adapter
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
            autonomy_mode: AutonomyMode::Ungated,
            reasoning_option: Default::default(),
            capabilities: Default::default(),
        })
        .await;

    match result {
        Err(AdapterError::Unavailable { reason }) => {
            assert!(
                reason.contains("codex"),
                "reason should name the vendor: {reason}"
            );
            assert!(
                reason.contains("ungated"),
                "reason should name the requested mode: {reason}"
            );
        }
        Ok(_) => panic!("expected AdapterError::Unavailable, got Ok(stream)"),
        Err(other) => panic!("expected AdapterError::Unavailable, got: {other}"),
    }
}
