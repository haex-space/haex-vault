//! Manual, opt-in smoke test against a *real* `codex` installation —
//! `#[ignore]`d by default since it needs a real, already-authenticated
//! `~/.codex/auth.json` and spends real API quota. Run explicitly with
//! `cargo test --test cli_delegate_codex_live -- --ignored`.
//!
//! This is not tasks.md's T012 (that's the pure unit test in
//! `codex_tests.rs`) — it exists because Phase 3 otherwise has no
//! integration-level check for the Codex path the way T010 covers
//! Claude Code with a stub, and a real spawn is cheap to verify by hand
//! given research.md §2's spike already proved the protocol live once.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use holzi_lib::adapters::cli_delegate::{CliDelegateAdapter, DelegateChatContext, DelegateVendor};
use holzi_lib::adapters::{ChatMessage, ChatRequest, ChatRole, ProviderAdapter, StreamChunk};

#[tokio::test]
#[ignore]
async fn spawn_codex_app_server_answers_a_real_question() {
    let auth = std::fs::read(dirs_auth_json_path())
        .expect("this machine's ~/.codex/auth.json (test requires a real login)");

    let adapter = CliDelegateAdapter::new(
        DelegateVendor::Codex,
        auth,
        "codex".to_string(),
        Some(DelegateChatContext {
            children: Default::default(),
            pending_tool_approvals: Arc::new(Mutex::new(HashMap::new())),
            emit: Arc::new(|_event: &str, _payload: serde_json::Value| {}),
            database: None,
            holzi: None,
        }),
    );

    let request = ChatRequest {
        sampling: Default::default(),
        model_id: "codex-delegate".to_string(),
        thread_id: None,
        system_prompt: None,
        messages: vec![ChatMessage {
            role: ChatRole::User,
            attachments: Vec::new(),
            content: "Reply with exactly the word: pong".to_string(),
        }],
        reasoning_requested: false,
        max_new_tokens: None,
        tools: Vec::new(),
        autonomy_mode: Default::default(),
        reasoning_option: Default::default(),
        capabilities: Default::default(),
    };

    let mut stream = adapter
        .stream_chat(request)
        .await
        .expect("stream_chat should start");

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

    assert!(saw_done, "expected a Done chunk");
    assert!(
        text.to_lowercase().contains("pong"),
        "expected the reply to contain \"pong\", got: {text:?}"
    );
}

fn dirs_auth_json_path() -> std::path::PathBuf {
    let home = std::env::var("HOME").expect("HOME set");
    std::path::PathBuf::from(home)
        .join(".codex")
        .join("auth.json")
}
