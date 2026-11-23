//! Integration test for the Abacus delegate (spec 096, T017): a request through
//! `CliDelegateAdapter` with a stub program standing in for the vault's copy of `abacusai` and a
//! vault that holds a placeholder credential. The reply is labelled with its backend and the program
//! that runs is the one the runtime names, by its own path. No real Abacus account is used.
#![cfg(unix)]

#[path = "common/marketplace_fixture.rs"]
mod marketplace_fixture;

use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use haex_lib::adapters::cli_delegate::{CliDelegateAdapter, DelegateChatContext, DelegateVendor};
use haex_lib::adapters::{ChatMessage, ChatRequest, ChatRole, ProviderAdapter, StreamChunk};
use haex_lib::cli_tools::runtime::DelegateRuntime;
use haex_lib::providers::delegate_credential::{self, KIND_DEVICE_CODE, VAULT_SCOPE};

use marketplace_fixture::vault;

const CODE: &str = "placeholder-device-code-of-the-run-test";
const TRANSCRIPT: &str = r#"{"type":"system","subtype":"init","permission_mode":"supervise","protocol_version":1}
{"type":"event","event":{"type":"text_delta","content":"Hello"}}
{"type":"event","event":{"type":"text_delta","content":" from Abacus"}}
{"type":"result","subtype":"success","is_error":false,"result":"Hello from Abacus","token_usage":{"inputTokens":3,"outputTokens":4},"errors":[]}
"#;

fn executable(path: &Path, script: &str) {
    fs::write(path, script).expect("write a stub");
    fs::set_permissions(path, fs::Permissions::from_mode(0o755)).expect("chmod a stub");
}

/// The stub of the vault's copy: reads the user line, then answers with the fixed transcript.
fn vault_program(dir: &Path) -> PathBuf {
    let path = dir.join("abacusai-in-the-vault");
    executable(
        &path,
        &format!(
            "#!/bin/sh\necho \"$0\" > '{}'\nIFS= read -r first\ncat <<'EOF'\n{TRANSCRIPT}EOF\n",
            dir.join("who-ran").display()
        ),
    );
    path
}

fn request() -> ChatRequest {
    ChatRequest {
        sampling: Default::default(),
        model_id: "abacusai".to_string(),
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
        autonomy_deny: Default::default(),
        reasoning_option: Default::default(),
        capabilities: Default::default(),
    }
}

#[tokio::test]
async fn a_request_is_answered_by_the_program_the_runtime_names() {
    let vault = vault();
    delegate_credential::save(&vault.db, "abacusai", KIND_DEVICE_CODE, CODE, VAULT_SCOPE)
        .await
        .expect("save the credential");
    let dir = tempfile::tempdir().expect("tempdir");
    let program = vault_program(dir.path());

    let adapter = CliDelegateAdapter::new(
        DelegateVendor::Abacus,
        Vec::new(),
        DelegateRuntime::for_stub("abacusai", &program, dir.path()),
        Some(DelegateChatContext {
            children: Default::default(),
            pending_tool_approvals: Default::default(),
            emit: Arc::new(|_event: &str, _payload: serde_json::Value| {}),
            database: Some(vault.db.clone()),
            haex: None,
            deny_rules: Vec::new(),
            tools: None,
        }),
    );

    let mut stream = adapter
        .stream_chat(request())
        .await
        .expect("the request starts");
    let mut text = String::new();
    let mut done = false;
    while let Some(chunk) = stream.next().await {
        match chunk.expect("no stream error") {
            StreamChunk::Delta { content, .. } => text.push_str(&content),
            StreamChunk::Done { .. } => {
                done = true;
                break;
            }
            StreamChunk::ToolCalls(_) => panic!("a delegate never emits ToolCalls"),
            StreamChunk::AgentActivity { .. } => {}
        }
    }

    assert_eq!(text, "Hello from Abacus");
    assert!(done, "the stream ends with a Done frame");
    assert_eq!(adapter.delegate_vendor(), Some(DelegateVendor::Abacus));
    // The program that ran is the one the runtime names, started by its own path: no lookup on the
    // host's PATH can have picked another one.
    assert_eq!(
        fs::read_to_string(dir.path().join("who-ran"))
            .unwrap()
            .trim(),
        program.to_string_lossy()
    );
}

#[tokio::test]
async fn the_adapter_lists_the_one_default_model_without_a_credential() {
    let adapter = CliDelegateAdapter::new(
        DelegateVendor::Abacus,
        Vec::new(),
        DelegateRuntime::for_stub(
            "abacusai",
            "/nonexistent/abacusai",
            tempfile::tempdir().unwrap().path(),
        ),
        None,
    );

    let models = adapter.list_models().await.expect("the list is local");

    assert_eq!(models.len(), 1);
    assert_eq!(models[0].remote_id, "abacusai");
}
