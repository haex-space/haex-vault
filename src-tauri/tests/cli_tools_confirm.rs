//! A version counts as proven only when a run with it ended the regular way (spec 094 US3,
//! FR-022, T050): Claude's `result`, Codex's completed turn, the credential of a connect flow.
//! A stop by the user, a missing login and a network error never confirm it.
//!
//! The tools are shell scripts that lie where an installed version does. Unix only.

#![cfg(unix)]

use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::PathBuf;

use haex_lib::adapters::cli_delegate::connect_codex::run_device_auth;
use haex_lib::adapters::cli_delegate::{CliDelegateAdapter, DelegateChatContext, DelegateVendor};
use haex_lib::adapters::{ChatMessage, ChatRequest, ChatRole, ProviderAdapter, StreamChunk};
use haex_lib::cli_tools::runtime::DelegateRuntime;
use haex_lib::instances::vault_paths::VaultPaths;
use haex_lib::vault_gate::ChildRegistry;

const VERSION: &str = "1.0.0";

const CLAUDE_OK: &str = r#"#!/bin/sh
cat >/dev/null
cat <<'EOF'
{"type":"system","subtype":"init","tools":["Task","Bash","Read"],"mcp_servers":[]}
{"type":"stream_event","event":{"type":"content_block_delta","delta":{"type":"text_delta","text":"ok"}}}
{"type":"result","is_error":false,"subtype":"success","result":"ok","usage":{"input_tokens":1,"output_tokens":1}}
EOF
"#;

const CLAUDE_ERROR_RESULT: &str = r#"#!/bin/sh
cat >/dev/null
echo '{"type":"system","subtype":"init","tools":["Task","Bash","Read"],"mcp_servers":[]}'
echo '{"type":"result","is_error":true,"subtype":"error","result":"Invalid API key"}'
"#;

/// A network error or a missing login: the tool says nothing useful and ends.
const CLAUDE_NO_RESULT: &str = "#!/bin/sh\ncat >/dev/null\necho 'could not connect' >&2\nexit 1\n";

/// A run that is still going when the user stops it.
const CLAUDE_SLOW: &str = r#"#!/bin/sh
cat >/dev/null
echo '{"type":"system","subtype":"init","tools":["Task","Bash","Read"],"mcp_servers":[]}'
echo '{"type":"stream_event","event":{"type":"content_block_delta","delta":{"type":"text_delta","text":"so far"}}}'
sleep 30
"#;

fn codex(turn: &str, after: &str) -> String {
    format!(
        r#"#!/bin/sh
read a; echo '{{"id":1,"result":{{}}}}'
read b; echo '{{"id":2,"result":{{"config":{{"web_search":"disabled"}}}}}}'
read c; echo '{{"id":3,"result":{{"data":[{{"name":"browser_use","enabled":false}},{{"name":"browser_use_external","enabled":false}},{{"name":"browser_use_full_cdp_access","enabled":false}},{{"name":"browser_annotation_api","enabled":false}},{{"name":"in_app_browser","enabled":false}}],"nextCursor":null}}}}'
read d; echo '{{"id":4,"result":{{"thread":{{"id":"t1"}}}}}}'
read e; echo '{{"id":5,"result":{{}}}}'
echo '{{"method":"item/agentMessage/delta","params":{{"delta":"hi"}}}}'
{turn}
{after}
"#
    )
}

const COMPLETED: &str =
    r#"echo '{"method":"turn/completed","params":{"turn":{"status":"completed"}}}'"#;
const FAILED: &str = r#"echo '{"method":"turn/completed","params":{"turn":{"status":"failed","error":{"message":"not logged in"}}}}'"#;

struct Setup {
    dir: tempfile::TempDir,
    runtime: DelegateRuntime,
}

impl Setup {
    /// A stub `vendor` program with `script` as its body, in the version folder `VERSION`.
    fn new(vendor: &str, script: &str) -> Self {
        let dir = tempfile::tempdir().unwrap();
        let runtime = DelegateRuntime::for_stub_version(vendor, dir.path(), VERSION);
        fs::create_dir_all(runtime.executable.parent().unwrap()).unwrap();
        fs::write(&runtime.executable, script).unwrap();
        fs::set_permissions(&runtime.executable, fs::Permissions::from_mode(0o755)).unwrap();
        Self { dir, runtime }
    }

    fn vendor_dir(&self, vendor: &str) -> PathBuf {
        VaultPaths::new(self.dir.path()).cli_tool_dir("stub", vendor)
    }

    fn confirmed(&self, vendor: &str) -> bool {
        self.vendor_dir(vendor)
            .join(VERSION)
            .join(".confirmed")
            .is_file()
    }

    fn adapter(&self, vendor: DelegateVendor, credentials: &[u8]) -> CliDelegateAdapter {
        CliDelegateAdapter::new(
            vendor,
            credentials.to_vec(),
            self.runtime.clone(),
            Some(DelegateChatContext {
                pending_tool_approvals: Default::default(),
                emit: std::sync::Arc::new(|_: &str, _: serde_json::Value| {}),
                database: None,
                children: Default::default(),
                haex: None,
                deny_rules: Vec::new(),
                tools: None,
            }),
        )
    }
}

fn request() -> ChatRequest {
    ChatRequest {
        sampling: Default::default(),
        model_id: "model".to_string(),
        thread_id: None,
        system_prompt: None,
        messages: vec![ChatMessage {
            role: ChatRole::User,
            content: "hello".to_string(),
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

/// Runs `adapter` to its end and says whether it ended with `Done`.
async fn run_to_the_end(adapter: &CliDelegateAdapter) -> bool {
    let mut stream = adapter.stream_chat(request()).await.expect("starts");
    let mut done = false;
    while let Some(chunk) = stream.next().await {
        match chunk {
            Ok(StreamChunk::Done { .. }) => {
                done = true;
                break;
            }
            Ok(_) => {}
            Err(_) => break,
        }
    }
    done
}

/// Starts `adapter`, waits for the first piece of text, and stops the run the way a user does.
async fn start_and_stop(adapter: &CliDelegateAdapter) {
    let mut stream = adapter.stream_chat(request()).await.expect("starts");
    loop {
        match stream.next().await {
            Some(Ok(StreamChunk::Delta { .. })) => break,
            Some(Ok(_)) => {}
            other => panic!("the run should still be going: {:?}", other.is_some()),
        }
    }
    stream.abort_handle().abort();
    while stream.next().await.is_some() {}
}

#[tokio::test]
async fn a_claude_run_that_ends_with_its_result_confirms_its_version() {
    let setup = Setup::new("claude", CLAUDE_OK);
    let adapter = setup.adapter(DelegateVendor::Claude, b"token");
    assert!(!setup.confirmed("claude"));

    assert!(run_to_the_end(&adapter).await);

    assert!(setup.confirmed("claude"));
}

#[tokio::test]
async fn a_claude_run_with_an_error_result_a_missing_login_or_a_network_error_confirms_nothing() {
    for script in [CLAUDE_ERROR_RESULT, CLAUDE_NO_RESULT] {
        let setup = Setup::new("claude", script);
        let adapter = setup.adapter(DelegateVendor::Claude, b"token");

        assert!(!run_to_the_end(&adapter).await);

        assert!(!setup.confirmed("claude"), "{script}");
    }
}

#[tokio::test]
async fn a_claude_run_the_user_stops_confirms_nothing() {
    let setup = Setup::new("claude", CLAUDE_SLOW);
    let adapter = setup.adapter(DelegateVendor::Claude, b"token");

    start_and_stop(&adapter).await;

    assert!(!setup.confirmed("claude"));
}

#[tokio::test]
async fn a_codex_run_that_completes_its_turn_confirms_its_version() {
    let setup = Setup::new("codex", &codex(COMPLETED, "sleep 30"));
    let adapter = setup.adapter(DelegateVendor::Codex, b"{}");

    assert!(run_to_the_end(&adapter).await);

    assert!(setup.confirmed("codex"));
}

#[tokio::test]
async fn a_codex_run_with_a_failed_turn_or_a_cut_connection_confirms_nothing() {
    for script in [codex(FAILED, "sleep 30"), codex("true", "exit 0")] {
        let setup = Setup::new("codex", &script);
        let adapter = setup.adapter(DelegateVendor::Codex, b"{}");

        assert!(!run_to_the_end(&adapter).await);

        assert!(!setup.confirmed("codex"), "{script}");
    }
}

#[tokio::test]
async fn a_codex_run_the_user_stops_confirms_nothing() {
    let setup = Setup::new("codex", &codex("true", "sleep 30"));
    let adapter = setup.adapter(DelegateVendor::Codex, b"{}");

    start_and_stop(&adapter).await;

    assert!(!setup.confirmed("codex"));
}

#[tokio::test]
async fn a_codex_login_that_obtains_the_credential_confirms_its_version() {
    let setup = Setup::new(
        "codex",
        "#!/bin/sh\necho 'logged in'\nprintf '{\"auth\":1}' > \"$CODEX_HOME/auth.json\"\n",
    );

    let credentials = run_device_auth(&setup.runtime, &ChildRegistry::default(), |_| {})
        .await
        .expect("the login succeeds");

    assert_eq!(credentials, b"{\"auth\":1}");
    assert!(setup.confirmed("codex"));
}

#[tokio::test]
async fn a_codex_login_that_fails_or_leaves_no_credential_confirms_nothing() {
    for script in [
        "#!/bin/sh\necho 'denied' >&2\nexit 1\n",
        "#!/bin/sh\necho 'logged in'\n",
    ] {
        let setup = Setup::new("codex", script);

        let result = run_device_auth(&setup.runtime, &ChildRegistry::default(), |_| {}).await;

        assert!(result.is_err(), "{script}");
        assert!(!setup.confirmed("codex"), "{script}");
    }
}
