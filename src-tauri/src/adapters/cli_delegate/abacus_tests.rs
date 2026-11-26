//! The Abacus delegate adapter against a shell stub that speaks the JSON-lines protocol of
//! `data-model.md` §4 (spec 096, T016). No real Abacus account is used: the stub stands in for the
//! program, the credential is a placeholder entry of a test vault.
#![cfg(unix)]

use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use serde_json::{json, Value};
use tempfile::TempDir;

use super::{
    approval_answer, build_command, chosen_model, fatal_reason, parse_line,
    spawn_abacus_invocation_with_coverage, startup_error, user_line, write_attachments, Line,
    APPROVALS_UNVERIFIED, APPROVAL_SETTINGS_FILE, CREDENTIAL_REFUSED, NOT_CONNECTED,
};
use crate::adapters::cli_delegate::autonomy::AutonomyMode;
use crate::adapters::cli_delegate::{CliDelegateAdapter, DelegateChatContext, DelegateVendor};
use crate::adapters::types::{
    Attachment, AttachmentKind, ChatMessage, ChatRequest, ChatRole, StreamChunk, StreamError,
};
use crate::adapters::{AdapterError, ProviderAdapter};
use crate::chat::tools::ApprovalDecision;
use crate::cli_tools::installed::CONFIRMED_FILE;
use crate::cli_tools::runtime::{DelegateRuntime, RunFolder};
use crate::instances::vault_paths::VaultPaths;
use crate::passwords::test_support::open_test_vault;
use crate::providers::delegate_credential::{self, KIND_DEVICE_CODE, VAULT_SCOPE};
use crate::storage::preferences::{self, PrefScope};
use crate::vault_gate::{VaultDb, VaultGate};

const CODE: &str = "placeholder-device-code-of-abacus-tests";

#[tokio::test]
async fn attachment_names_cannot_escape_the_run_folder() {
    let temp = TempDir::new().unwrap();
    let run = RunFolder::create(&temp.path().join("run"), &temp.path().join("home")).unwrap();
    let mut req = request(DelegateVendor::Abacus.as_str());
    req.messages[0].attachments.push(Attachment {
        name: "../../outside.txt".to_string(),
        kind: AttachmentKind::Text,
        media_type: "text/plain".to_string(),
        bytes: b"inside the run folder".to_vec(),
    });

    write_attachments(&req, &run).await.unwrap();

    assert_eq!(
        fs::read(run.path().join("attachment-0-.._.._outside.txt")).unwrap(),
        b"inside the run folder"
    );
    assert!(!temp.path().join("outside.txt").exists());
}

// ---- the pure parts ----

#[test]
fn text_events_become_text_and_unknown_events_are_ignored() {
    let line = r#"{"type":"event","event":{"type":"text_delta","content":"Hel"},"session_id":null,"uuid":"u"}"#;
    assert_eq!(parse_line(line), Line::Text("Hel".into()));
    for other in [
        r#"{"type":"event","event":{"type":"credits","creditsUsed":1}}"#,
        r#"{"type":"event","event":{"type":"conversation_info","title":"x"}}"#,
        r#"{"type":"system","subtype":"skills_loaded"}"#,
        r#"{"type":"brand_new_message_type"}"#,
        "not json at all",
        "",
    ] {
        assert_eq!(parse_line(other), Line::Other, "{other}");
    }
    // A sub-task's text is not the answer.
    let nested = r#"{"type":"event","event":{"type":"text_delta","content":"x","subtaskId":"s1"}}"#;
    assert_eq!(parse_line(nested), Line::Other);
}

#[test]
fn the_init_message_reports_the_mode() {
    let line =
        r#"{"type":"system","subtype":"init","permission_mode":"DEFAULT","protocol_version":1}"#;
    assert_eq!(
        parse_line(line),
        Line::Init {
            permission_mode: Some("DEFAULT".into())
        }
    );
}

#[test]
fn a_tool_request_carries_its_id_name_and_input() {
    let line = r#"{"type":"control_request","request_id":"perm-1","request":{"subtype":"can_use_tool","tool_call_id":"t1","request":{"tool_name":"Bash","tool_input":{"command":"ls"}}}}"#;
    assert_eq!(
        parse_line(line),
        Line::Approval {
            request_id: "perm-1".into(),
            tool_name: "Bash".into(),
            input: json!({"command": "ls"}),
        }
    );
}

#[test]
fn a_tool_request_as_the_program_sends_it_names_the_tool_and_its_arguments() {
    // The shape measured against version 2.10.23 (research §12).
    let line = r#"{"type":"control_request","request_id":"perm-4","request":{"subtype":"can_use_tool","tool_call_id":"t4","always_allow_rules":[],"request":{"type":"generic","tool":{"id":"t4","name":"read","args":{"path":"/etc/hostname"},"status":"executing","endpoint":"file_read"},"displayName":"Read"}}}"#;
    assert_eq!(
        parse_line(line),
        Line::Approval {
            request_id: "perm-4".into(),
            tool_name: "read".into(),
            input: json!({"path": "/etc/hostname"}),
        }
    );
}

#[test]
fn a_tool_request_with_unknown_fields_still_asks_and_never_allows_on_its_own() {
    let line = r#"{"type":"control_request","request_id":"perm-2","request":{"subtype":"can_use_tool","tool_call_id":"t2","request":{"something":"else"}}}"#;
    match parse_line(line) {
        Line::Approval {
            request_id,
            tool_name,
            input,
        } => {
            assert_eq!(request_id, "perm-2");
            assert_eq!(tool_name, "Abacus tool");
            assert_eq!(input, json!({"something": "else"}));
        }
        other => panic!("{other:?}"),
    }
    // Not even an object: still a request, with the raw value.
    let bare = r#"{"type":"control_request","request_id":"perm-3","request":{"subtype":"can_use_tool","request":"text"}}"#;
    assert!(matches!(parse_line(bare), Line::Approval { .. }));
}

#[test]
fn other_control_requests_are_told_apart_and_one_without_an_id_is_ignored() {
    let line =
        r#"{"type":"control_request","request_id":"x","request":{"subtype":"something_new"}}"#;
    assert_eq!(
        parse_line(line),
        Line::UnknownRequest {
            request_id: "x".into()
        }
    );
    let no_id = r#"{"type":"control_request","request":{"subtype":"can_use_tool"}}"#;
    assert_eq!(parse_line(no_id), Line::Other);
}

#[test]
fn a_successful_result_has_its_text_and_token_counts() {
    let line = r#"{"type":"result","subtype":"success","is_error":false,"result":"Hello","token_usage":{"inputTokens":12,"outputTokens":34,"totalTokens":46},"errors":[]}"#;
    assert_eq!(
        parse_line(line),
        Line::Result {
            success: true,
            text: "Hello".into(),
            message: "Hello".into(),
            prompt_tokens: Some(12),
            completion_tokens: Some(34),
        }
    );
}

#[test]
fn a_failed_result_carries_the_first_error() {
    let line = r#"{"type":"result","subtype":"error_during_execution","is_error":true,"result":"x","errors":["the model refused","other"]}"#;
    match parse_line(line) {
        Line::Result {
            success, message, ..
        } => {
            assert!(!success);
            assert_eq!(message, "the model refused");
        }
        other => panic!("{other:?}"),
    }
    // An error subtype is never a success, whatever `is_error` says.
    let odd = r#"{"type":"result","subtype":"prompt_blocked","is_error":false,"result":"blocked"}"#;
    assert!(matches!(
        parse_line(odd),
        Line::Result { success: false, .. }
    ));
}

#[test]
fn the_answers_to_a_request_are_accept_or_reject_and_nothing_persistent() {
    let accepted = approval_answer("perm-1", true);
    assert_eq!(accepted["type"], "control_response");
    assert_eq!(accepted["response"]["subtype"], "success");
    assert_eq!(accepted["response"]["request_id"], "perm-1");
    assert_eq!(accepted["response"]["response"]["decision"], "accept");
    let rejected = approval_answer("perm-1", false);
    assert_eq!(rejected["response"]["response"]["decision"], "reject");
    for answer in [accepted, rejected] {
        let text = answer.to_string();
        for persistent in [
            "allowAlways",
            "allowYolo",
            "allow_always_with_rules",
            "question_answers",
        ] {
            assert!(!text.contains(persistent), "{text}");
        }
    }
}

#[test]
fn the_user_line_has_the_documented_shape() {
    assert_eq!(
        user_line("hi"),
        json!({"type": "user", "message": {"role": "user", "content": "hi"}})
    );
}

#[test]
fn a_fatal_reason_is_read_from_the_last_line_of_stderr() {
    let stderr = "some diagnostics\n{\"type\":\"fatal\",\"reason\":\"not_authenticated\"}\n";
    assert_eq!(fatal_reason(stderr).as_deref(), Some("not_authenticated"));
    assert_eq!(fatal_reason("nothing structured"), None);
    assert_eq!(
        fatal_reason("{\"type\":\"fatal\",\"reason\":\"user@example.com\"}"),
        None,
        "unknown vendor text is never returned as an error reason"
    );
}

#[test]
fn a_program_that_ends_at_once_is_classified_from_its_fatal_reason() {
    let fatal = |reason: &str| format!("{{\"type\":\"fatal\",\"reason\":\"{reason}\"}}\n");
    for reason in ["not_authenticated", "access_denied"] {
        match startup_error(fatal(reason).as_bytes()) {
            AdapterError::Unavailable { reason } => assert_eq!(reason, CREDENTIAL_REFUSED),
            other => panic!("{other:?}"),
        }
    }
    match startup_error(fatal("network_unreachable").as_bytes()) {
        AdapterError::Http { reason } => assert!(reason.contains("cannot be reached"), "{reason}"),
        other => panic!("{other:?}"),
    }
    match startup_error(fatal("no_models").as_bytes()) {
        AdapterError::Http { reason } => assert_eq!(reason, "abacusai: no_models"),
        other => panic!("{other:?}"),
    }
    // Text of the program that is not a plain reason is never repeated.
    match startup_error(b"{\"type\":\"fatal\",\"reason\":\"/home/someone/secret path\"}\n") {
        AdapterError::Http { reason } => assert!(!reason.contains("someone"), "{reason}"),
        other => panic!("{other:?}"),
    }
    match startup_error(b"{\"type\":\"fatal\",\"reason\":\"user@example.com\"}\n") {
        AdapterError::Http { reason } => assert_eq!(reason, "abacusai ended before it answered"),
        other => panic!("{other:?}"),
    }
    assert!(matches!(startup_error(b""), AdapterError::Http { .. }));
}

#[test]
fn the_vendor_id_and_an_empty_id_name_the_account_default() {
    assert_eq!(chosen_model("abacusai"), None);
    assert_eq!(chosen_model(""), None);
    assert_eq!(chosen_model("  "), None);
    assert_eq!(chosen_model("some-model"), Some("some-model"));
}

// ---- the program as a stub ----

struct Fixture {
    dir: TempDir,
    db: VaultDb,
    _vault_dir: TempDir,
}

impl Fixture {
    async fn new() -> Self {
        let (vault_dir, database) = open_test_vault();
        let db = VaultGate::new().vault_db(Arc::new(database)).unwrap();
        delegate_credential::save(&db, "abacusai", KIND_DEVICE_CODE, CODE, VAULT_SCOPE)
            .await
            .unwrap();
        Self {
            dir: TempDir::new().unwrap(),
            db,
            _vault_dir: vault_dir,
        }
    }

    fn path(&self, name: &str) -> PathBuf {
        self.dir.path().join(name)
    }

    /// A stub program with the body `body`; it writes its arguments to `args`, its environment to
    /// `env` and the first line of its input to `input` of this fixture's folder first.
    fn stub(&self, body: &str) -> PathBuf {
        let script = format!(
            "#!/bin/sh\necho \"$@\" > '{args}'\nfor arg; do last=$arg; done\ncp \"$last\" '{settings}' 2>/dev/null\nenv > '{env}'\nIFS= read -r first\nprintf '%s\\n' \"$first\" > '{input}'\n{body}\n",
            args = self.path("args").display(),
            settings = self.path("settings").display(),
            env = self.path("env").display(),
            input = self.path("input").display(),
        );
        let path = self.path("abacusai");
        fs::write(&path, script).unwrap();
        fs::set_permissions(&path, fs::Permissions::from_mode(0o755)).unwrap();
        path
    }

    fn adapter(&self, program: &Path, context: DelegateChatContext) -> CliDelegateAdapter {
        CliDelegateAdapter::new(
            DelegateVendor::Abacus,
            Vec::new(),
            DelegateRuntime::for_stub("abacusai", program, self.dir.path()),
            Some(context),
        )
    }

    fn context(&self) -> DelegateChatContext {
        DelegateChatContext {
            pending_tool_approvals: Default::default(),
            emit: Arc::new(|_event: &str, _payload: Value| {}),
            database: Some(self.db.clone()),
            children: Default::default(),
            haex: None,
            deny_rules: Vec::new(),
            tools: None,
        }
    }

    /// A context whose approval question is answered at once with `decision`; the requests that
    /// reached the bridge are recorded.
    fn answering(
        &self,
        decision: ApprovalDecision,
        seen: Arc<Mutex<Vec<Value>>>,
    ) -> DelegateChatContext {
        let context = self.context();
        let pending = context.pending_tool_approvals.clone();
        DelegateChatContext {
            emit: Arc::new(move |event: &str, payload: Value| {
                if event != "tool-permission-request" {
                    return;
                }
                let id = payload["requestId"].as_str().unwrap().parse().unwrap();
                seen.lock().unwrap().push(payload);
                let sender = pending
                    .lock()
                    .unwrap()
                    .remove(&id)
                    .expect("a pending request");
                let _ = sender.send(decision);
            }),
            ..context
        }
    }

    fn read(&self, name: &str) -> String {
        fs::read_to_string(self.path(name)).unwrap_or_default()
    }

    fn run_root(&self) -> PathBuf {
        VaultPaths::new(self.dir.path()).run_root("stub")
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
            content: "say hi".to_string(),
            attachments: Vec::new(),
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

/// The text of a stream and how it ended.
async fn collect(
    mut stream: impl std::borrow::BorrowMut<crate::adapters::AdapterStream>,
) -> (String, Option<StreamChunk>, Option<StreamError>) {
    let stream = stream.borrow_mut();
    let mut text = String::new();
    while let Some(next) = stream.next().await {
        match next {
            Ok(StreamChunk::Delta { content, .. }) => text.push_str(&content),
            Ok(done @ StreamChunk::Done { .. }) => return (text, Some(done), None),
            Ok(StreamChunk::ToolCalls(_)) => panic!("a delegate never emits ToolCalls"),
            Ok(StreamChunk::AgentActivity { .. }) => {}
            Err(error) => return (text, None, Some(error)),
        }
    }
    (text, None, None)
}

/// What 2.10.23 reports in `init` when started with `--permission-mode supervise` (research §15).
const INIT: &str =
    r#"{"type":"system","subtype":"init","permission_mode":"DEFAULT","protocol_version":1}"#;
/// What 2.10.23 reports in `init` when started with `--permission-mode unsupervised`.
const INIT_UNSUPERVISED: &str =
    r#"{"type":"system","subtype":"init","permission_mode":"YOLO","protocol_version":1}"#;
const HELLO: &str = r#"{"type":"event","event":{"type":"text_delta","content":"Hel"}}
{"type":"event","event":{"type":"text_delta","content":"lo"}}"#;
const OK_RESULT: &str = r#"{"type":"result","subtype":"success","is_error":false,"result":"Hello","token_usage":{"inputTokens":3,"outputTokens":2},"errors":[]}"#;

fn happy_body() -> String {
    format!("cat <<'EOF'\n{INIT}\n{HELLO}\n{OK_RESULT}\nEOF")
}

#[tokio::test]
async fn the_command_line_is_the_contract_and_the_child_has_the_device_code_and_no_key() {
    let fixture = Fixture::new().await;
    let program = fixture.stub(&happy_body());
    let adapter = fixture.adapter(&program, fixture.context());

    let stream = adapter.stream_chat(request("abacusai")).await.unwrap();
    collect(stream).await;

    let args = fixture.read("args");
    let (args, settings_path) = args.trim().rsplit_once(' ').unwrap();
    assert_eq!(
        args,
        "-p --input-format stream-json --output-format stream-json --include-partial-messages \
         --permission-mode supervise --no-agents-md --settings"
    );
    assert!(
        settings_path.ends_with(&format!("/{APPROVAL_SETTINGS_FILE}")),
        "{settings_path}"
    );
    // Every tool call asks, a read-only one too (research §12).
    let settings: Value = serde_json::from_str(&fixture.read("settings")).unwrap();
    assert_eq!(settings, json!({ "permissions": { "ask": ["*"] } }));
    let env = fixture.read("env");
    assert!(
        env.contains(&format!("CODELLM_DEVICE_CODE={CODE}")),
        "the device code is set"
    );
    assert!(!env.contains("ABACUS_API_KEY"), "an API key is never set");
    assert!(env.contains("ABACUS_AUTO_UPDATE=off"));
}

#[tokio::test]
async fn with_the_gate_closed_standard_requests_are_unavailable_and_nothing_starts() {
    let fixture = Fixture::new().await;
    let program = fixture.stub(&format!(
        "echo started > '{}'\n{happy}",
        fixture.path("started").display(),
        happy = happy_body(),
    ));

    let error = match spawn_abacus_invocation_with_coverage(
        DelegateRuntime::for_stub("abacusai", &program, fixture.dir.path()),
        request("abacusai"),
        fixture.context(),
        "abacusai".to_string(),
        false,
    )
    .await
    {
        Err(error) => error,
        Ok(_) => panic!("a closed gate must not start the program"),
    };

    assert!(
        matches!(&error, AdapterError::Unavailable { reason } if reason == APPROVALS_UNVERIFIED),
        "{error:?}"
    );
    assert!(!fixture.path("started").exists(), "nothing was spawned");
}

#[tokio::test]
async fn a_chosen_model_is_passed_on_and_the_account_default_is_not() {
    let fixture = Fixture::new().await;
    let program = fixture.stub(&happy_body());
    let adapter = fixture.adapter(&program, fixture.context());

    collect(adapter.stream_chat(request("some-model")).await.unwrap()).await;

    assert!(fixture.read("args").contains("--model some-model"));
}

#[tokio::test]
async fn the_user_line_is_written_and_text_and_the_result_become_the_chat_stream() {
    let fixture = Fixture::new().await;
    let program = fixture.stub(&happy_body());
    let runtime = DelegateRuntime::for_stub_version("abacusai", fixture.dir.path(), "1.0.0");
    let version = VaultPaths::new(fixture.dir.path())
        .cli_tool_dir("stub", "abacusai")
        .join("1.0.0");
    let confirmed = version.join(CONFIRMED_FILE);
    fs::create_dir_all(runtime.executable.parent().expect("program directory")).unwrap();
    fs::rename(program, &runtime.executable).unwrap();
    assert!(!confirmed.exists());
    let adapter = CliDelegateAdapter::new(
        DelegateVendor::Abacus,
        Vec::new(),
        runtime,
        Some(fixture.context()),
    );

    let mut stream = adapter.stream_chat(request("abacusai")).await.unwrap();
    let (text, done, error) = collect(&mut stream).await;
    tokio::time::timeout(std::time::Duration::from_secs(10), async {
        while stream.next().await.is_some() {}
    })
    .await
    .expect("the successful run finishes its cleanup");

    let sent: Value = serde_json::from_str(fixture.read("input").trim()).unwrap();
    assert_eq!(sent["type"], "user");
    assert!(sent["message"]["content"]
        .as_str()
        .unwrap()
        .contains("say hi"));
    assert_eq!(text, "Hello");
    assert!(
        confirmed.is_file(),
        "the first successful run confirms the version"
    );
    assert!(error.is_none(), "{error:?}");
    match done {
        Some(StreamChunk::Done {
            prompt_tokens,
            completion_tokens,
            finish_reason,
            ..
        }) => {
            assert_eq!(prompt_tokens, Some(3));
            assert_eq!(completion_tokens, Some(2));
            assert_eq!(finish_reason.as_deref(), Some("complete"));
        }
        other => panic!("{other:?}"),
    }
}

#[tokio::test]
async fn a_turn_answered_without_partial_events_still_has_its_text() {
    let fixture = Fixture::new().await;
    let program = fixture.stub(&format!("cat <<'EOF'\n{INIT}\n{OK_RESULT}\nEOF"));
    let adapter = fixture.adapter(&program, fixture.context());

    let (text, done, _) = collect(adapter.stream_chat(request("abacusai")).await.unwrap()).await;

    assert_eq!(text, "Hello");
    assert!(done.is_some());
}

#[tokio::test]
async fn unknown_message_types_and_garbage_are_ignored() {
    let fixture = Fixture::new().await;
    let program = fixture.stub(&format!(
        "cat <<'EOF'\n{INIT}\n{{\"type\":\"never_seen_before\"}}\nnot json\n{HELLO}\n{OK_RESULT}\nEOF"
    ));
    let adapter = fixture.adapter(&program, fixture.context());

    let (text, done, error) =
        collect(adapter.stream_chat(request("abacusai")).await.unwrap()).await;

    assert_eq!(text, "Hello");
    assert!(done.is_some() && error.is_none());
}

#[tokio::test]
async fn an_errored_result_ends_the_stream_with_the_model_error() {
    let fixture = Fixture::new().await;
    let program = fixture.stub(&format!(
        "cat <<'EOF'\n{INIT}\n{{\"type\":\"result\",\"subtype\":\"error_during_execution\",\"is_error\":true,\"result\":\"x\",\"errors\":[\"out of credits\"]}}\nEOF"
    ));
    let adapter = fixture.adapter(&program, fixture.context());

    let (_, done, error) = collect(adapter.stream_chat(request("abacusai")).await.unwrap()).await;

    assert!(done.is_none());
    assert!(matches!(error, Some(StreamError::Model(message)) if message == "out of credits"));
}

#[tokio::test]
async fn a_refused_credential_ends_as_invalid_credentials_and_nothing_is_retried() {
    let fixture = Fixture::new().await;
    let program =
        fixture.stub("echo '{\"type\":\"fatal\",\"reason\":\"not_authenticated\"}' >&2\nexit 1");
    let adapter = fixture.adapter(&program, fixture.context());

    let error = adapter
        .stream_chat(request("abacusai"))
        .await
        .err()
        .unwrap();

    assert!(
        matches!(&error, AdapterError::Unavailable { reason } if reason == CREDENTIAL_REFUSED),
        "{error:?}"
    );
    assert!(!error.is_transient());
}

#[tokio::test]
async fn an_unreachable_service_ends_as_a_network_failure() {
    let fixture = Fixture::new().await;
    let program =
        fixture.stub("echo '{\"type\":\"fatal\",\"reason\":\"network_unreachable\"}' >&2\nexit 1");
    let adapter = fixture.adapter(&program, fixture.context());

    let error = adapter
        .stream_chat(request("abacusai"))
        .await
        .err()
        .unwrap();

    assert!(
        matches!(&error, AdapterError::Http { reason } if reason.contains("cannot be reached")),
        "{error:?}"
    );
}

#[tokio::test]
async fn without_a_credential_the_request_ends_as_not_connected_and_nothing_starts() {
    let fixture = Fixture::new().await;
    delegate_credential::delete(&fixture.db, "abacusai")
        .await
        .unwrap();
    let program = fixture.stub(&happy_body());
    let adapter = fixture.adapter(&program, fixture.context());

    let error = adapter
        .stream_chat(request("abacusai"))
        .await
        .err()
        .unwrap();

    assert!(
        matches!(&error, AdapterError::Unavailable { reason } if reason == NOT_CONNECTED),
        "{error:?}"
    );
    assert!(
        !fixture.path("args").exists(),
        "the program was not started"
    );
    assert!(
        !fixture.run_root().exists() || fs::read_dir(fixture.run_root()).unwrap().next().is_none()
    );
}

#[tokio::test]
async fn a_context_without_the_vault_cannot_read_a_credential_and_starts_nothing() {
    let fixture = Fixture::new().await;
    let program = fixture.stub(&happy_body());
    let context = DelegateChatContext {
        database: None,
        ..fixture.context()
    };
    let adapter = fixture.adapter(&program, context);

    let error = adapter
        .stream_chat(request("abacusai"))
        .await
        .err()
        .unwrap();

    assert!(matches!(&error, AdapterError::Unavailable { reason } if reason == NOT_CONNECTED));
    assert!(!fixture.path("args").exists());
}

#[tokio::test]
async fn the_run_folder_is_removed_when_the_run_ends() {
    let fixture = Fixture::new().await;
    let program = fixture.stub(&happy_body());
    let adapter = fixture.adapter(&program, fixture.context());

    collect(adapter.stream_chat(request("abacusai")).await.unwrap()).await;

    // The folder goes with the spawned task, a moment after the last frame.
    for _ in 0..50 {
        let left = fs::read_dir(fixture.run_root()).map_or(0, Iterator::count);
        if left == 0 {
            return;
        }
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    }
    panic!("the run folder is still there");
}

#[tokio::test]
async fn gated_permissive_is_not_served_and_nothing_starts() {
    let fixture = Fixture::new().await;
    let program = fixture.stub(&happy_body());
    let adapter = fixture.adapter(&program, fixture.context());
    let mut asked = request("abacusai");
    asked.autonomy_mode = AutonomyMode::GatedPermissive;

    let error = adapter.stream_chat(asked).await.err().unwrap();

    match error {
        AdapterError::Unavailable { reason } => {
            assert_eq!(
                reason,
                "autonomy mode unavailable: vendor=abacusai; mode=gated_permissive"
            );
        }
        other => panic!("{other:?}"),
    }
    assert!(!fixture.path("args").exists());
}

fn ungated(model_id: &str) -> ChatRequest {
    let mut asked = request(model_id);
    asked.autonomy_mode = AutonomyMode::Ungated;
    asked
}

fn ungated_body() -> String {
    format!("cat <<'EOF'\n{INIT_UNSUPERVISED}\n{HELLO}\n{OK_RESULT}\nEOF")
}

#[tokio::test]
async fn ungated_runs_unsupervised_without_the_ask_rule() {
    let fixture = Fixture::new().await;
    let program = fixture.stub(&ungated_body());
    let adapter = fixture.adapter(&program, fixture.context());

    let (text, done, error) =
        collect(adapter.stream_chat(ungated("abacusai")).await.unwrap()).await;

    assert_eq!(text, "Hello");
    assert!(done.is_some() && error.is_none(), "{error:?}");
    assert_eq!(
        fixture.read("args").trim(),
        "-p --input-format stream-json --output-format stream-json --include-partial-messages \
         --permission-mode unsupervised --no-agents-md"
    );
    assert!(
        fixture.read("settings").is_empty(),
        "no settings file is handed over"
    );
    assert!(fixture
        .read("env")
        .contains(&format!("CODELLM_DEVICE_CODE={CODE}")));
}

#[tokio::test]
async fn ungated_rejects_what_the_program_still_asks_without_the_bridge() {
    let fixture = Fixture::new().await;
    let answer = fixture.path("answer");
    let program = fixture.stub(&approval_body(&answer, "ask_user_question").replacen(
        INIT,
        INIT_UNSUPERVISED,
        1,
    ));
    let seen = Arc::new(Mutex::new(Vec::new()));
    let adapter = fixture.adapter(
        &program,
        fixture.answering(ApprovalDecision::Allow, seen.clone()),
    );

    let (_, done, error) = collect(adapter.stream_chat(ungated("abacusai")).await.unwrap()).await;

    assert!(done.is_some() && error.is_none(), "{error:?}");
    let reply: Value = serde_json::from_str(fixture.read("answer").trim()).unwrap();
    assert_eq!(reply["response"]["response"]["decision"], "reject");
    assert!(seen.lock().unwrap().is_empty(), "the bridge was not asked");
}

#[tokio::test]
async fn ungated_is_not_held_by_the_approval_gate() {
    let fixture = Fixture::new().await;
    let program = fixture.stub(&ungated_body());

    let stream = spawn_abacus_invocation_with_coverage(
        DelegateRuntime::for_stub("abacusai", &program, fixture.dir.path()),
        ungated("abacusai"),
        fixture.context(),
        "abacusai".to_string(),
        false,
    )
    .await
    .unwrap();
    let (text, done, _) = collect(stream).await;

    assert_eq!(text, "Hello");
    assert!(done.is_some());
}

#[tokio::test]
async fn an_init_in_another_mode_ends_the_run() {
    // Each served mode refuses the other's report; the flag names are not what `init` reports.
    for (asked, reported) in [
        (request("abacusai"), "YOLO"),
        (request("abacusai"), "supervise"),
        (ungated("abacusai"), "DEFAULT"),
        (ungated("abacusai"), "unsupervised"),
    ] {
        let fixture = Fixture::new().await;
        let program = fixture.stub(&format!(
            "cat <<'EOF'\n{{\"type\":\"system\",\"subtype\":\"init\",\"permission_mode\":\"{reported}\"}}\nEOF\nsleep 5"
        ));
        let adapter = fixture.adapter(&program, fixture.context());

        let (text, done, error) = collect(adapter.stream_chat(asked).await.unwrap()).await;

        assert!(text.is_empty() && done.is_none(), "{reported}");
        assert!(
            matches!(error, Some(StreamError::Model(_))),
            "{reported}: {error:?}"
        );
    }
}

/// A stub that asks to run a tool, waits for the answer and writes the answer's line to `answer`.
fn approval_body(answer: &Path, tool: &str) -> String {
    format!(
        "cat <<'EOF'\n{INIT}\n{{\"type\":\"control_request\",\"request_id\":\"perm-1\",\"request\":{{\"subtype\":\"can_use_tool\",\"tool_call_id\":\"t1\",\"request\":{{\"tool_name\":\"{tool}\",\"tool_input\":{{\"command\":\"ls\"}}}}}}}}\nEOF\nIFS= read -r reply\nprintf '%s\\n' \"$reply\" > '{answer}'\ncat <<'EOF'\n{OK_RESULT}\nEOF",
        answer = answer.display(),
    )
}

#[tokio::test]
async fn a_tool_request_is_forwarded_to_the_approval_bridge_and_accepted_or_rejected() {
    for (decision, expected) in [
        (ApprovalDecision::Allow, "accept"),
        (ApprovalDecision::Deny, "reject"),
    ] {
        let fixture = Fixture::new().await;
        let answer = fixture.path("answer");
        let program = fixture.stub(&approval_body(&answer, "Bash"));
        let seen = Arc::new(Mutex::new(Vec::new()));
        let adapter = fixture.adapter(&program, fixture.answering(decision, seen.clone()));

        let (_, done, error) =
            collect(adapter.stream_chat(request("abacusai")).await.unwrap()).await;

        assert!(
            done.is_some() && error.is_none(),
            "the stream continues after the answer"
        );
        let reply: Value = serde_json::from_str(fixture.read("answer").trim()).unwrap();
        assert_eq!(reply["response"]["request_id"], "perm-1");
        assert_eq!(reply["response"]["response"]["decision"], expected);
        let seen = seen.lock().unwrap();
        assert_eq!(seen.len(), 1, "the bridge was asked once");
        assert_eq!(seen[0]["toolName"], "Bash");
    }
}

#[tokio::test]
async fn a_control_request_the_adapter_does_not_answer_gets_an_error_response() {
    let fixture = Fixture::new().await;
    let answer = fixture.path("answer");
    let program = fixture.stub(&format!(
        "cat <<'EOF'\n{INIT}\n{{\"type\":\"control_request\",\"request_id\":\"q-1\",\"request\":{{\"subtype\":\"something_new\"}}}}\nEOF\nIFS= read -r reply\nprintf '%s\\n' \"$reply\" > '{answer}'\ncat <<'EOF'\n{OK_RESULT}\nEOF",
        answer = answer.display(),
    ));
    let adapter = fixture.adapter(&program, fixture.context());

    let (_, done, _) = collect(adapter.stream_chat(request("abacusai")).await.unwrap()).await;

    assert!(done.is_some());
    let reply: Value = serde_json::from_str(fixture.read("answer").trim()).unwrap();
    assert_eq!(reply["response"]["subtype"], "error");
    assert_eq!(reply["response"]["request_id"], "q-1");
}

#[tokio::test]
async fn a_disconnect_during_a_response_does_not_disturb_it_and_the_next_request_is_refused() {
    let fixture = Fixture::new().await;
    // The program holds its own copy of the device code (in its environment), so deleting the entry
    // while it answers changes nothing for this response.
    let release = fixture.path("release-response");
    let program = fixture.stub(&format!(
        "cat <<'EOF'\n{INIT}\nEOF\nfor i in $(seq 1 500); do [ -e '{release}' ] && break; sleep 0.01; done\n[ -e '{release}' ] || exit 2\ncat <<'EOF'\n{HELLO}\n{OK_RESULT}\nEOF",
        release = release.display(),
    ));
    let adapter = fixture.adapter(&program, fixture.context());
    let stream = adapter.stream_chat(request("abacusai")).await.unwrap();

    delegate_credential::delete(&fixture.db, "abacusai")
        .await
        .unwrap();
    fs::write(&release, "deleted").unwrap();
    let (text, done, error) = collect(stream).await;

    assert_eq!(text, "Hello");
    assert!(done.is_some() && error.is_none(), "{error:?}");
    let next = adapter
        .stream_chat(request("abacusai"))
        .await
        .err()
        .unwrap();
    assert!(
        matches!(&next, AdapterError::Unavailable { reason } if reason == NOT_CONNECTED),
        "{next:?}"
    );
}

#[tokio::test]
async fn no_file_of_the_vault_folder_holds_the_device_code_after_a_run() {
    let fixture = Fixture::new().await;
    // A program that does not record its own environment, as the stubs above do.
    let path = fixture.path("abacusai");
    fs::write(
        &path,
        format!("#!/bin/sh\nIFS= read -r first\ncat <<'EOF'\n{INIT}\n{HELLO}\n{OK_RESULT}\nEOF\n"),
    )
    .unwrap();
    fs::set_permissions(&path, fs::Permissions::from_mode(0o755)).unwrap();
    let adapter = fixture.adapter(&path, fixture.context());

    collect(adapter.stream_chat(request("abacusai")).await.unwrap()).await;
    // The run folder goes with the task, a moment after the last frame.
    tokio::time::sleep(std::time::Duration::from_millis(500)).await;

    fn files(dir: &Path, found: &mut Vec<PathBuf>) {
        for entry in fs::read_dir(dir).into_iter().flatten().flatten() {
            let path = entry.path();
            if path.is_dir() {
                files(&path, found);
            } else {
                found.push(path);
            }
        }
    }
    let mut found = Vec::new();
    files(fixture.dir.path(), &mut found);
    for file in found {
        let bytes = fs::read(&file).unwrap_or_default();
        assert!(
            !bytes
                .windows(CODE.len())
                .any(|window| window == CODE.as_bytes()),
            "{} holds the device code",
            file.display()
        );
    }
}

#[test]
fn the_command_names_the_vaults_program_and_never_a_host_lookup() {
    let dir = TempDir::new().unwrap();
    let runtime =
        DelegateRuntime::for_stub("abacusai", dir.path().join("vault-abacusai"), dir.path());
    let run = runtime.run_folder().unwrap();

    let settings = run.path().join(APPROVAL_SETTINGS_FILE);
    let command = build_command(&runtime, "abacusai", None, Some(&settings), &run);

    assert_eq!(
        command.as_std().get_program(),
        dir.path().join("vault-abacusai").as_os_str()
    );
    let env: Vec<_> = command
        .as_std()
        .get_envs()
        .map(|(name, _)| name.to_string_lossy().into_owned())
        .collect();
    assert!(!env.iter().any(|name| name == "ABACUS_API_KEY"));
    assert!(
        !env.iter().any(|name| name == "CODELLM_DEVICE_CODE"),
        "set after containment, not here"
    );
}

// ---- US3: the approval posture (spec 096, T031, T032) ----

/// A stub that asks to run `tool` with the request line `request` (the JSON of the whole
/// `control_request`), waits for the answer and writes it to `answer`.
fn asking_body(answer: &Path, request: &str) -> String {
    format!(
        "cat <<'EOF'\n{INIT}\n{request}\nEOF\nIFS= read -r reply\nprintf '%s\\n' \"$reply\" > '{answer}'\ncat <<'EOF'\n{OK_RESULT}\nEOF",
        answer = answer.display(),
    )
}

fn tool_request(tool: &str) -> String {
    format!(
        "{{\"type\":\"control_request\",\"request_id\":\"perm-1\",\"request\":{{\"subtype\":\"can_use_tool\",\"tool_call_id\":\"t1\",\"request\":{{\"tool_name\":\"{tool}\",\"tool_input\":{{\"path\":\"notes.txt\"}}}}}}}}"
    )
}

/// The words of a decision that would outlive the one call; the adapter never sends any.
const PERSISTENT_WORDS: [&str; 4] = [
    "allowAlways",
    "allowYolo",
    "allow_always_with_rules",
    "question_answers",
];

#[tokio::test]
async fn a_read_only_and_a_changing_tool_are_each_asked_once_and_answered_without_a_persistent_variant(
) {
    for tool in ["Read", "Bash"] {
        for (decision, expected) in [
            (ApprovalDecision::Allow, "accept"),
            (ApprovalDecision::Deny, "reject"),
        ] {
            let fixture = Fixture::new().await;
            let answer = fixture.path("answer");
            let program = fixture.stub(&asking_body(&answer, &tool_request(tool)));
            let seen = Arc::new(Mutex::new(Vec::new()));
            let adapter = fixture.adapter(&program, fixture.answering(decision, seen.clone()));

            let (_, done, error) =
                collect(adapter.stream_chat(request("abacusai")).await.unwrap()).await;

            assert!(
                done.is_some() && error.is_none(),
                "{tool}: the stream continues after the answer ({error:?})"
            );
            let raw = fixture.read("answer");
            let reply: Value = serde_json::from_str(raw.trim()).unwrap();
            assert_eq!(
                reply["response"]["response"]["decision"], expected,
                "{tool}"
            );
            for word in PERSISTENT_WORDS {
                assert!(!raw.contains(word), "{tool}: the answer holds {word}");
            }
            assert_eq!(seen.lock().unwrap().len(), 1, "{tool}: asked once");
        }
    }
}

#[tokio::test]
async fn a_request_without_tool_fields_is_still_asked_for_and_never_allowed_on_its_own() {
    let fixture = Fixture::new().await;
    let answer = fixture.path("answer");
    let malformed = r#"{"type":"control_request","request_id":"perm-1","request":{"subtype":"can_use_tool","request":{}}}"#;
    let program = fixture.stub(&asking_body(&answer, malformed));
    let seen = Arc::new(Mutex::new(Vec::new()));
    let adapter = fixture.adapter(
        &program,
        fixture.answering(ApprovalDecision::Deny, seen.clone()),
    );

    let (_, done, error) = collect(adapter.stream_chat(request("abacusai")).await.unwrap()).await;

    assert!(done.is_some() && error.is_none(), "{error:?}");
    let reply: Value = serde_json::from_str(fixture.read("answer").trim()).unwrap();
    assert_eq!(reply["response"]["response"]["decision"], "reject");
    let seen = seen.lock().unwrap();
    assert_eq!(seen.len(), 1, "the person was asked");
    assert_eq!(seen[0]["toolName"], "Abacus tool");
}

#[tokio::test]
async fn a_program_that_closes_its_input_before_the_answer_ends_the_request_without_an_accept() {
    let fixture = Fixture::new().await;
    let seen = Arc::new(Mutex::new(Vec::new()));
    // The program asks and ends at once: the answer has nowhere to go.
    let program = fixture.stub(&format!(
        "cat <<'EOF'\n{INIT}\n{}\nEOF\nexit 0",
        tool_request("Bash")
    ));
    let adapter = fixture.adapter(
        &program,
        fixture.answering(ApprovalDecision::Allow, seen.clone()),
    );

    let (_, done, error) = collect(adapter.stream_chat(request("abacusai")).await.unwrap()).await;

    assert!(done.is_none(), "no result was reached");
    assert!(error.is_some(), "the request ends with an error");
}

/// Sets the vault's permission mode (`chat.permission_mode`).
async fn set_permission_mode(db: &VaultDb, mode: &str) {
    let mode = mode.to_owned();
    db.write(move |tx| {
        preferences::insert_or_update(tx, PrefScope::Vault, "chat.permission_mode", &mode)?;
        Ok(())
    })
    .await
    .unwrap();
}

#[tokio::test]
async fn under_the_posture_that_blocks_sensitive_actions_a_changing_tool_gets_no_accept() {
    let fixture = Fixture::new().await;
    set_permission_mode(&fixture.db, "plan").await;
    let answer = fixture.path("answer");
    let program = fixture.stub(&asking_body(&answer, &tool_request("Bash")));
    let requests = Arc::new(Mutex::new(Vec::new()));
    let seen = requests.clone();
    let context = DelegateChatContext {
        emit: Arc::new(move |event: &str, _payload: Value| {
            seen.lock().unwrap().push(event.to_owned());
        }),
        ..fixture.context()
    };
    let adapter = fixture.adapter(&program, context);

    let (_, done, error) = collect(adapter.stream_chat(request("abacusai")).await.unwrap()).await;

    assert!(done.is_some() && error.is_none(), "{error:?}");
    let reply: Value = serde_json::from_str(fixture.read("answer").trim()).unwrap();
    assert_eq!(reply["response"]["response"]["decision"], "reject");
    assert!(
        !requests
            .lock()
            .unwrap()
            .iter()
            .any(|event| event == "tool-permission-request"),
        "the posture refuses without asking"
    );
}

/// Whether the process `pid` is still alive.
fn alive(pid: &str) -> bool {
    std::process::Command::new("kill")
        .args(["-0", pid])
        .stderr(std::process::Stdio::null())
        .status()
        .is_ok_and(|status| status.success())
}

async fn wait_until_gone(pid: &str) -> bool {
    for _ in 0..100 {
        if !alive(pid) {
            return true;
        }
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    }
    false
}

#[tokio::test]
async fn a_stop_ends_the_running_program() {
    let fixture = Fixture::new().await;
    let pidfile = fixture.path("pid");
    let program = fixture.stub(&format!(
        "echo $$ > '{pid}'\ncat <<'EOF'\n{INIT}\nEOF\nsleep 30",
        pid = pidfile.display()
    ));
    let adapter = fixture.adapter(&program, fixture.context());
    let stream = adapter.stream_chat(request("abacusai")).await.unwrap();
    let pid = fixture.read("pid").trim().to_owned();
    assert!(alive(&pid), "the program runs");

    stream.abort_handle().abort();

    assert!(
        wait_until_gone(&pid).await,
        "the program was ended by the stop"
    );
}

#[tokio::test]
async fn a_closing_vault_ends_the_running_program_and_the_request() {
    let fixture = Fixture::new().await;
    let pidfile = fixture.path("pid");
    let program = fixture.stub(&format!(
        "echo $$ > '{pid}'\ncat <<'EOF'\n{INIT}\nEOF\nsleep 30",
        pid = pidfile.display()
    ));
    let context = fixture.context();
    let children = context.children.clone();
    let adapter = fixture.adapter(&program, context);
    let stream = adapter.stream_chat(request("abacusai")).await.unwrap();
    let pid = fixture.read("pid").trim().to_owned();
    assert!(alive(&pid), "the program runs");

    // What the close of the vault does to every child it started.
    children.kill_all();

    let (_, done, error) = collect(stream).await;
    assert!(
        done.is_none() && error.is_some(),
        "the request ends, not finished"
    );
    assert!(wait_until_gone(&pid).await, "the program is gone");
}

// ---- US4: no trace in the logs (spec 096, T037) ----

const TRACE_EMAIL: &str = "trace.person@example.invalid";
const TRACE_USER_CODE: &str = "TRACE-USER-CODE-5521";
const TRACE_DEVICE_CODE: &str = "trace-device-code-of-the-log-search-8830";
const TRACE_REQUEST: &str = "TRACE-REQUEST-TEXT-7741";

fn trace_request() -> ChatRequest {
    let mut asked = request("abacusai");
    asked.messages[0].content = TRACE_REQUEST.to_owned();
    asked
}

#[tokio::test]
async fn after_a_connect_a_request_and_a_stop_no_log_line_holds_the_email_the_codes_or_the_request()
{
    use crate::providers::abacus_signin::{Client, Secret};
    use crate::providers::connect_abacus::connect;
    use tokio_util::sync::CancellationToken;
    use wiremock::matchers::{method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    crate::test_log::install();
    let fixture = Fixture::new().await;

    // Connect against a stand-in of the sign-in service.
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/api/_authenticateDeviceForCodeLlm"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "success": true,
            "result": {
                "verificationUri": "https://example.invalid/device",
                "userCode": TRACE_USER_CODE,
                "pollingInterval": 1,
            },
        })))
        .mount(&server)
        .await;
    Mock::given(method("POST"))
        .and(path("/api/_pollDeviceCode"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "success": true,
            "result": { "deviceCode": TRACE_DEVICE_CODE, "chatWebUrl": "https://example.invalid/chat" },
        })))
        .mount(&server)
        .await;
    let client = Client::with_base(&server.uri()).unwrap();
    let record = fixture_record(&fixture);
    connect(
        &client,
        &fixture.db,
        &record,
        "Abacus AI".to_owned(),
        &Secret::new(TRACE_EMAIL.to_owned()),
        &CancellationToken::new(),
        |_| {},
    )
    .await
    .expect("the connect succeeds");

    // A request that runs to its end, and one that is stopped on the way.
    let program = fixture.stub(&happy_body());
    let adapter = fixture.adapter(&program, fixture.context());
    let (_, done, error) = collect(adapter.stream_chat(trace_request()).await.unwrap()).await;
    assert!(done.is_some() && error.is_none(), "{error:?}");

    let slow = fixture.stub(&format!(
        "echo $$ > '{pid}'\ncat <<'EOF'\n{INIT}\nEOF\nsleep 30",
        pid = fixture.path("pid").display()
    ));
    let stopped = fixture.adapter(&slow, fixture.context());
    let stream = stopped.stream_chat(trace_request()).await.unwrap();
    let pid = fixture.read("pid").trim().to_owned();
    stream.abort_handle().abort();
    assert!(wait_until_gone(&pid).await, "the stopped program exited");
    drop(stream);

    // What the sign-in service and the HTTP stack log of their own is not haex-vault's trace: the
    // service received the address by design.
    let logs = crate::test_log::all_but(&["wiremock", "hyper", "reqwest"]);
    for needle in [
        TRACE_EMAIL,
        TRACE_USER_CODE,
        TRACE_DEVICE_CODE,
        TRACE_REQUEST,
    ] {
        assert!(!logs.contains(needle), "a log line holds {needle}");
    }
    let paths = [fixture.dir.path(), fixture._vault_dir.path()];
    for path in paths {
        let path = path.display().to_string();
        assert!(
            !logs.contains(&path),
            "a log line names a path of the vault"
        );
    }

    // The record of the vault keeps fixed text only.
    let text = fs::read_to_string(record.path()).expect("read the vault record");
    assert!(text.contains("connect_finished"));
    assert_eq!(text.matches("run_started").count(), 2);
    for needle in [
        TRACE_EMAIL,
        TRACE_USER_CODE,
        TRACE_DEVICE_CODE,
        TRACE_REQUEST,
    ] {
        assert!(!text.contains(needle), "the record holds {needle}");
    }
    for path in paths {
        let path = path.display().to_string();
        assert!(
            !text.contains(&path),
            "the record names a path of the vault"
        );
    }
}

fn fixture_record(fixture: &Fixture) -> crate::cli_tools::record::Record {
    crate::cli_tools::record::Record::new(VaultPaths::new(fixture.dir.path()).records_dir("stub"))
}
