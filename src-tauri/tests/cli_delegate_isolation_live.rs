//! Manual, opt-in live tests (`#[ignore]`d, like `cli_delegate_codex_live.rs`)
//! proving isolation is genuine, not a false positive that happens to work
//! because it silently falls back to this machine's real, already-
//! authenticated `~/.claude`/`~/.codex` session.
//!
//! Every other isolation test in this feature (`cli_delegate_claude.rs`,
//! `cli_delegate_codex_live.rs`, `cli_delegate_connect.rs`) copies a *valid*
//! credential into the isolated temp dir — a pass there can't distinguish
//! "used my isolated copy" from "ignored the isolated dir and used the
//! host's real login", since both would produce the same successful
//! answer. These tests close that gap with a negative control: a
//! deliberately *wrong* credential in the isolated dir must make the
//! invocation fail, proving the isolated dir is what's actually consulted.
//!
//! Run explicitly with `cargo test --test cli_delegate_isolation_live --
//! --ignored` (needs `claude`/`codex` installed; the Codex half needs no
//! valid login at all, the Claude half doesn't either).

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use holzi_lib::adapters::cli_delegate::{CliDelegateAdapter, DelegateChatContext, DelegateVendor};
use holzi_lib::adapters::{ChatMessage, ChatRequest, ChatRole, ProviderAdapter, StreamChunk};

fn sample_request() -> ChatRequest {
    ChatRequest {
        sampling: Default::default(),
        model_id: "delegate".to_string(),
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
    }
}

fn no_op_context() -> DelegateChatContext {
    DelegateChatContext {
        children: Default::default(),
        pending_tool_approvals: Arc::new(Mutex::new(HashMap::new())),
        emit: Arc::new(|_event: &str, _payload: serde_json::Value| {}),
        database: None,
        holzi: None,
    }
}

/// Live-verified 2026-09-16 (research.md §6): a garbage `auth.json` in the
/// isolated `CODEX_HOME` produces a genuine `401 Unauthorized` from
/// `api.openai.com` after a few retries -- it does not quietly succeed via
/// this machine's real, valid `~/.codex/auth.json`. Also exercises the
/// `turn_completed_failure` fix: codex reports this failure via
/// `turn/completed` with `status: "failed"`, not a separate `turn/failed`
/// notification, and `codex.rs` must surface it as a `StreamError`, not a
/// silently-empty `Done`.
#[tokio::test]
#[ignore]
async fn codex_with_a_wrong_credential_fails_instead_of_using_the_hosts_real_login() {
    let adapter = CliDelegateAdapter::new(
        DelegateVendor::Codex,
        br#"{"tokens":{"access_token":"garbage-not-a-real-token"}}"#.to_vec(),
        "codex".to_string(),
        Some(no_op_context()),
    );

    let mut stream = adapter
        .stream_chat(sample_request())
        .await
        .expect("stream_chat should start even with a bad credential");

    let mut saw_error = false;
    while let Some(chunk) = stream.next().await {
        match chunk {
            Err(_) => {
                saw_error = true;
                break;
            }
            Ok(StreamChunk::Done { .. }) => {
                panic!(
                    "a wrong credential must not produce a normal Done -- \
                     this would mean the isolated CODEX_HOME was ignored \
                     and the host's real ~/.codex/auth.json was used instead"
                );
            }
            Ok(StreamChunk::ToolCalls(_)) => panic!("a delegate must never emit ToolCalls"),
            Ok(StreamChunk::Delta { .. }) => {}
            Ok(StreamChunk::AgentActivity { .. }) => {}
        }
    }
    assert!(
        saw_error,
        "expected a StreamError from the 401 auth failure"
    );
}

/// Live-verified 2026-09-16 (research.md §6): an isolated `CLAUDE_CONFIG_DIR`
/// with a garbage `CLAUDE_CODE_OAUTH_TOKEN` produces a genuine `401`/
/// `authentication_failed` from Anthropic's API -- it does not quietly
/// succeed via this machine's real, valid Claude Code login (which this very
/// test suite runs under).
#[tokio::test]
#[ignore]
async fn claude_with_a_wrong_token_fails_instead_of_using_the_hosts_real_login() {
    let adapter = CliDelegateAdapter::new(
        DelegateVendor::Claude,
        b"sk-ant-oat01-this-is-definitely-not-a-real-token".to_vec(),
        "claude".to_string(),
        Some(no_op_context()),
    );

    let mut stream = adapter
        .stream_chat(sample_request())
        .await
        .expect("stream_chat should start even with a bad token");

    let mut saw_error = false;
    while let Some(chunk) = stream.next().await {
        match chunk {
            Err(_) => {
                saw_error = true;
                break;
            }
            Ok(StreamChunk::Done { .. }) => {
                panic!(
                    "a wrong token must not produce a normal Done -- this \
                     would mean the isolated CLAUDE_CONFIG_DIR was ignored \
                     and the host's real ~/.claude session was used instead"
                );
            }
            Ok(StreamChunk::ToolCalls(_)) => panic!("a delegate must never emit ToolCalls"),
            Ok(StreamChunk::Delta { .. }) => {}
            Ok(StreamChunk::AgentActivity { .. }) => {}
        }
    }
    assert!(
        saw_error,
        "expected a StreamError from the 401 auth failure"
    );
}
