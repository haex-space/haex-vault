//! The MCP exchange of a tool connection as transcripts (spec 018, contracts/mcp-port.md). Each
//! scenario runs holzi's client against the in-memory server through JSON and writes every message
//! with its direction to `specs/018-extension-agent-tools/contracts/transcripts/<name>.jsonl`. The
//! vault-sdk replays them against its own server, so both sides speak the same protocol.
//!
//! The committed files must equal what this records; `HAEX_WRITE_MCP_TRANSCRIPTS=1` rewrites them.
//! Versions in `clientInfo` and `serverInfo` are replaced by `<version>`, so a release of holzi or
//! rmcp does not change a transcript.

use std::path::{Path, PathBuf};
use std::time::Duration;

use rmcp::model::{CallToolRequest, CallToolRequestParams, ClientRequest};
use rmcp::service::PeerRequestOptions;
use serde_json::{json, Map, Value};

use super::test_support::{pipe, Direction, Pipe};

fn transcript_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../specs/018-extension-agent-tools/contracts/transcripts")
}

fn normalized(mut message: Value) -> Value {
    for (outer, inner) in [("params", "clientInfo"), ("result", "serverInfo")] {
        if let Some(info) = message
            .get_mut(outer)
            .and_then(|value| value.get_mut(inner))
            .and_then(Value::as_object_mut)
        {
            if info.contains_key("version") {
                info.insert("version".to_owned(), json!("<version>"));
            }
        }
    }
    message
}

fn jsonl(messages: &[(Direction, Value)]) -> String {
    messages
        .iter()
        .map(|(direction, message)| {
            let line = json!({ "dir": direction.as_str(), "message": normalized(message.clone()) });
            format!("{line}\n")
        })
        .collect()
}

/// Writes or checks one transcript.
fn settle(name: &str, messages: &[(Direction, Value)]) {
    let path = transcript_dir().join(format!("{name}.jsonl"));
    let recorded = jsonl(messages);
    if std::env::var_os("HAEX_WRITE_MCP_TRANSCRIPTS").is_some() {
        std::fs::create_dir_all(transcript_dir()).expect("transcript dir");
        std::fs::write(&path, recorded).expect("write transcript");
        return;
    }
    let committed = std::fs::read_to_string(&path).unwrap_or_default();
    assert_eq!(
        committed, recorded,
        "{name}.jsonl is stale; rerun with HAEX_WRITE_MCP_TRANSCRIPTS=1"
    );
}

/// The messages after the handshake (initialize, its result, `notifications/initialized`).
fn after_handshake(pipe: &Pipe) -> Vec<(Direction, Value)> {
    pipe.messages().into_iter().skip(3).collect()
}

fn arguments(value: Value) -> Map<String, Value> {
    value.as_object().expect("object").clone()
}

async fn call(pipe: &Pipe, name: &'static str, args: Value) {
    pipe.link
        .peer()
        .call_tool(CallToolRequestParams::new(name).with_arguments(arguments(args)))
        .await
        .expect("tools/call");
}

#[tokio::test]
async fn handshake() {
    let pipe = pipe().await;
    let messages = pipe.messages();
    assert_eq!(messages.len(), 3, "{messages:?}");
    assert_eq!(messages[0].1["method"], "initialize");
    assert_eq!(messages[2].1["method"], "notifications/initialized");
    settle("handshake", &messages);
}

#[tokio::test]
async fn list() {
    let pipe = pipe().await;
    pipe.link.peer().list_all_tools().await.expect("tools/list");
    settle("list", &after_handshake(&pipe));
}

#[tokio::test]
async fn call_ok() {
    let pipe = pipe().await;
    call(&pipe, "echo", json!({ "text": "hallo" })).await;
    settle("call-ok", &after_handshake(&pipe));
}

#[tokio::test]
async fn call_invalid_input() {
    let pipe = pipe().await;
    call(&pipe, "echo", json!({})).await;
    let messages = after_handshake(&pipe);
    assert_eq!(messages[1].1["result"]["isError"], true);
    settle("call-invalid-input", &messages);
}

#[tokio::test]
async fn call_error() {
    let pipe = pipe().await;
    call(&pipe, "fail", json!({})).await;
    let messages = after_handshake(&pipe);
    assert_eq!(messages[1].1["result"]["isError"], true);
    settle("call-error", &messages);
}

#[tokio::test]
async fn call_cancelled() {
    let pipe = pipe().await;
    let request = ClientRequest::CallToolRequest(CallToolRequest::new(
        CallToolRequestParams::new("slow").with_arguments(arguments(json!({ "text": "x" }))),
    ));
    let handle = pipe
        .link
        .peer()
        .send_cancellable_request(request, PeerRequestOptions::no_options())
        .await
        .expect("send");
    handle
        .cancel(Some("the turn was stopped".to_owned()))
        .await
        .expect("cancel");
    // The server may answer the cancelled call or not; give it the chance, then keep only what
    // holzi sent, which is what the contract fixes.
    tokio::time::sleep(Duration::from_millis(100)).await;
    let messages: Vec<_> = after_handshake(&pipe)
        .into_iter()
        .filter(|(direction, _)| *direction == Direction::ClientToServer)
        .collect();
    assert_eq!(messages[1].1["method"], "notifications/cancelled");
    settle("call-cancelled", &messages);
}

#[tokio::test]
async fn server_request_rejected() {
    let pipe = pipe().await;
    let id = json!(901);
    pipe.inject(json!({
        "jsonrpc": "2.0", "id": 901, "method": "sampling/createMessage",
        "params": { "messages": [{ "role": "user", "content": { "type": "text", "text": "hi" } }],
                    "maxTokens": 10 }
    }));
    pipe.client_reply(&id).await;
    settle("server-request-rejected", &after_handshake(&pipe));
}
