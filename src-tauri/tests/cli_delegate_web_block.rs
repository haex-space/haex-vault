//! Spec 102: a delegate whose program reports a state without the web block ends the request with
//! a reason the person can read, before anything of the answer reaches the chat (FR-007, SC-005).
//! Stub programs stand in for `claude` and `codex`; their lines follow the 2.1.311 and 0.169.0
//! probes of research R1 and R2.

#![cfg(unix)]

use std::collections::HashMap;
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use haex_lib::adapters::cli_delegate::{CliDelegateAdapter, DelegateChatContext, DelegateVendor};
use haex_lib::adapters::{
    AdapterError, AdapterStream, ChatMessage, ChatRequest, ChatRole, ProviderAdapter, StreamChunk,
};
use haex_lib::cli_tools::runtime::DelegateRuntime;

fn write_stub(dir: &Path, name: &str, script: &str) -> PathBuf {
    let path = dir.join(name);
    fs::write(&path, script).expect("write stub");
    let mut perms = fs::metadata(&path).unwrap().permissions();
    perms.set_mode(0o755);
    fs::set_permissions(&path, perms).expect("chmod stub");
    path
}

fn context() -> DelegateChatContext {
    DelegateChatContext {
        children: Default::default(),
        pending_tool_approvals: Arc::new(Mutex::new(HashMap::new())),
        emit: Arc::new(|_: &str, _: serde_json::Value| {}),
        database: None,
        haex: None,
        deny_rules: Vec::new(),
        tools: None,
    }
}

fn request(model_id: &str) -> ChatRequest {
    ChatRequest {
        sampling: Default::default(),
        model_id: model_id.to_string(),
        thread_id: None,
        system_prompt: None,
        messages: vec![ChatMessage {
            role: ChatRole::User,
            attachments: Vec::new(),
            content: "open example.org".to_string(),
        }],
        reasoning_requested: false,
        max_new_tokens: None,
        tools: Vec::new(),
        autonomy_mode: Default::default(),
        autonomy_deny: Default::default(),
        reasoning_option: Default::default(),
        capabilities: Default::default(),
    }
}

async fn start(
    vendor: DelegateVendor,
    stub: &Path,
    dir: &Path,
) -> Result<AdapterStream, AdapterError> {
    let (name, credentials, model) = match vendor {
        DelegateVendor::Claude => ("claude", b"fake-oauth-token".to_vec(), "claude-delegate"),
        _ => ("codex", b"fake-auth".to_vec(), "codex-delegate"),
    };
    CliDelegateAdapter::new(
        vendor,
        credentials,
        DelegateRuntime::for_stub(name, stub, dir),
        Some(context()),
    )
    .stream_chat(request(model))
    .await
}

fn expect_refused(result: Result<AdapterStream, AdapterError>, what: &str) {
    match result {
        Err(AdapterError::Unavailable { reason }) => {
            assert!(reason.contains("without its web block"), "{reason}");
            assert!(reason.contains(what), "{reason}");
        }
        Ok(_) => panic!("expected a refused start, got a stream"),
        Err(other) => panic!("expected AdapterError::Unavailable, got: {other}"),
    }
}

async fn answer_text(mut stream: AdapterStream) -> String {
    let mut text = String::new();
    while let Some(chunk) = stream.next().await {
        match chunk.expect("no stream error") {
            StreamChunk::Delta { content, .. } => text.push_str(&content),
            StreamChunk::Done { .. } => break,
            _ => {}
        }
    }
    text
}

fn claude_stub(dir: &Path, init: &str) -> PathBuf {
    write_stub(
        dir,
        "fake-claude",
        &format!(
            "#!/bin/sh\ncat >/dev/null\ncat <<'EOF'\n{init}\n\
{{\"type\":\"stream_event\",\"event\":{{\"type\":\"content_block_delta\",\"delta\":{{\"type\":\"text_delta\",\"text\":\"Hello\"}}}}}}\n\
{{\"type\":\"result\",\"is_error\":false,\"subtype\":\"success\",\"result\":\"Hello\",\"usage\":{{\"input_tokens\":1,\"output_tokens\":1}}}}\n\
EOF\n"
        ),
    )
}

#[tokio::test]
async fn claude_reporting_its_own_web_tool_is_refused_before_any_answer() {
    let dir = tempfile::tempdir().unwrap();
    let stub = claude_stub(
        dir.path(),
        r#"{"type":"system","subtype":"init","tools":["Task","Bash","WebFetch"],"mcp_servers":[]}"#,
    );
    expect_refused(
        start(DelegateVendor::Claude, &stub, dir.path()).await,
        "WebFetch",
    );
}

#[tokio::test]
async fn claude_with_chrome_connected_is_refused() {
    let dir = tempfile::tempdir().unwrap();
    let stub = claude_stub(
        dir.path(),
        r#"{"type":"system","subtype":"init","tools":["Task"],"mcp_servers":[{"name":"claude-in-chrome","status":"connected"}]}"#,
    );
    expect_refused(
        start(DelegateVendor::Claude, &stub, dir.path()).await,
        "claude-in-chrome",
    );
}

#[tokio::test]
async fn claude_answering_without_an_init_line_is_refused() {
    let dir = tempfile::tempdir().unwrap();
    let stub = claude_stub(dir.path(), r#"{"type":"system","subtype":"status"}"#);
    expect_refused(
        start(DelegateVendor::Claude, &stub, dir.path()).await,
        "no init line",
    );
}

#[tokio::test]
async fn claude_with_the_block_answers_and_the_lines_before_init_are_kept() {
    let dir = tempfile::tempdir().unwrap();
    let stub = claude_stub(
        dir.path(),
        "{\"type\":\"system\",\"subtype\":\"hook_started\"}\n\
{\"type\":\"system\",\"subtype\":\"init\",\"tools\":[\"Task\",\"Bash\"],\"mcp_servers\":[]}",
    );
    let stream = start(DelegateVendor::Claude, &stub, dir.path())
        .await
        .expect("a stream");
    assert_eq!(answer_text(stream).await, "Hello");
}

const CODEX_HEAD: &str = r#"#!/usr/bin/env python3
import json, sys

def read():
    line = sys.stdin.readline()
    return json.loads(line) if line else None

def send(msg):
    sys.stdout.write(json.dumps(msg) + "\n")
    sys.stdout.flush()

FEATURES = ("browser_use", "browser_use_external", "browser_use_full_cdp_access", "browser_annotation_api", "in_app_browser")

req = read()  # initialize
send({"jsonrpc": "2.0", "id": req["id"], "result": {}})
req = read()  # config/read
send({"jsonrpc": "2.0", "id": req["id"], "result": {"config": {"web_search": "disabled"}}})
"#;

const CODEX_TURN: &str = r#"
req = read()  # thread/start
send({"jsonrpc": "2.0", "id": req["id"], "result": {"thread": {"id": "fake-thread"}}})
req = read()  # turn/start
send({"jsonrpc": "2.0", "id": req["id"], "result": {}})
send({"jsonrpc": "2.0", "method": "item/agentMessage/delta", "params": {"delta": "Hello"}})
send({"jsonrpc": "2.0", "method": "turn/completed", "params": {}})
sys.stdin.readline()
"#;

#[tokio::test]
async fn codex_reporting_a_browser_feature_on_is_refused() {
    let dir = tempfile::tempdir().unwrap();
    let stub = write_stub(
        dir.path(),
        "fake-codex",
        &format!(
            r#"{CODEX_HEAD}req = read()  # experimentalFeature/list
send({{"jsonrpc": "2.0", "id": req["id"], "result": {{"data": [{{"name": n, "enabled": n == "browser_use"}} for n in FEATURES], "nextCursor": None}}}})
sys.stdin.readline()
"#
        ),
    );
    expect_refused(
        start(DelegateVendor::Codex, &stub, dir.path()).await,
        "browser_use is enabled",
    );
}

#[tokio::test]
async fn codex_feature_pages_are_all_read() {
    let dir = tempfile::tempdir().unwrap();
    let stub = write_stub(
        dir.path(),
        "fake-codex",
        &format!(
            r#"{CODEX_HEAD}req = read()  # experimentalFeature/list, page 1
assert "cursor" not in req["params"]
send({{"jsonrpc": "2.0", "id": req["id"], "result": {{"data": [{{"name": n, "enabled": False}} for n in FEATURES[:2]], "nextCursor": "page-2"}}}})
req = read()  # experimentalFeature/list, page 2
assert req["params"]["cursor"] == "page-2"
send({{"jsonrpc": "2.0", "id": req["id"], "result": {{"data": [{{"name": n, "enabled": False}} for n in FEATURES[2:]], "nextCursor": None}}}})
{CODEX_TURN}"#
        ),
    );
    let stream = start(DelegateVendor::Codex, &stub, dir.path())
        .await
        .expect("a stream");
    assert_eq!(answer_text(stream).await, "Hello");
}

#[tokio::test]
async fn codex_refusing_a_switch_at_start_shows_its_own_words() {
    let dir = tempfile::tempdir().unwrap();
    let stub = write_stub(
        dir.path(),
        "fake-codex",
        "#!/bin/sh\necho 'Error: Unknown feature flag: browser_use' >&2\nexit 1\n",
    );
    match start(DelegateVendor::Codex, &stub, dir.path()).await {
        Err(error) => assert!(
            error
                .to_string()
                .contains("Unknown feature flag: browser_use"),
            "{error}"
        ),
        Ok(_) => panic!("expected a failed start"),
    }
}
