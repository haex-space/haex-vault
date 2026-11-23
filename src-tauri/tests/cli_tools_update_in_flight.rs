//! An update does not disturb a run that is going, and closing the vault ends every run (spec 094
//! US3, T049, FR-010, SC-008).
//!
//! The tool is installed twice through the real install pipeline from a local server, as a shell
//! script per version that waits for a signal file in the vault's home before it answers. Unix
//! only, because the tool is a script.

#![cfg(unix)]

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use haex_lib::adapters::cli_delegate::{CliDelegateAdapter, DelegateChatContext, DelegateVendor};
use haex_lib::adapters::{
    AdapterStream, ChatMessage, ChatRequest, ChatRole, ProviderAdapter, StreamChunk,
};
use haex_lib::cli_tools::catalog::{Artifact, ArtifactKind, Catalog};
use haex_lib::cli_tools::containment::EnvContainment;
use haex_lib::cli_tools::install::{install, InstallContext};
use haex_lib::cli_tools::platform::PlatformKey;
use haex_lib::cli_tools::record::Record;
use haex_lib::cli_tools::runtime::{DelegateTools, RuntimeSource};
use haex_lib::cli_tools::uses::vendor_in_use;
use haex_lib::instances::vault_paths::VaultPaths;
use haex_lib::vault_gate::ChildRegistry;
use sha2::{Digest, Sha256};
use tokio_util::sync::CancellationToken;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

const OLD: &str = "1.0.0";
const NEW: &str = "1.1.0";

/// The tool of `version`: `--version` names it; a run says it started, waits for the `go` file in
/// the vault's home, and then answers like a Claude stream.
fn tool(version: &str) -> String {
    format!(
        r#"#!/bin/sh
case " $* " in
  *" --version "*) echo "claude {version} (stub)"; exit 0 ;;
esac
cat >/dev/null
echo $$ > "$HOME/pid-{version}"
echo "$CLAUDE_CONFIG_DIR" > "$HOME/run-folder-{version}"
echo started > "$HOME/started-{version}"
while [ ! -f "$HOME/go" ]; do sleep 0.05; done
cat <<'EOF'
{{"type":"stream_event","event":{{"type":"content_block_delta","delta":{{"type":"text_delta","text":"from {version}"}}}}}}
{{"type":"result","is_error":false,"subtype":"success","result":"ok","usage":{{"input_tokens":1,"output_tokens":1}}}}
EOF
"#
    )
}

struct Vault {
    paths: VaultPaths,
    children: ChildRegistry,
    _dir: tempfile::TempDir,
}

const NAME: &str = "anna";

impl Vault {
    fn new() -> Self {
        let dir = tempfile::tempdir().expect("tempdir");
        Self {
            paths: VaultPaths::new(dir.path()),
            children: ChildRegistry::default(),
            _dir: dir,
        }
    }

    fn home(&self) -> PathBuf {
        self.paths.home(NAME)
    }

    fn vendor_dir(&self) -> PathBuf {
        self.paths.cli_tool_dir(NAME, "claude")
    }

    fn folders(&self) -> Vec<String> {
        let mut names: Vec<String> = fs::read_dir(self.vendor_dir())
            .map(|entries| {
                entries
                    .flatten()
                    .map(|entry| entry.file_name().to_string_lossy().into_owned())
                    .filter(|name| !name.starts_with('.'))
                    .collect()
            })
            .unwrap_or_default();
        names.sort();
        names
    }

    /// Installs `version` from a local server, as the Settings page does.
    async fn install_tool(&self, version: &str) {
        let body = tool(version).into_bytes();
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/tool"))
            .respond_with(ResponseTemplate::new(200).set_body_bytes(body.clone()))
            .mount(&server)
            .await;
        let context = InstallContext {
            vendor: Arc::new(Catalog::shipped().vendor("claude").unwrap().clone()),
            version: version.to_string(),
            artifact: Artifact {
                url: format!("{}/tool", server.uri()),
                sha256: Sha256::digest(&body)
                    .iter()
                    .map(|byte| format!("{byte:02x}"))
                    .collect(),
                size: body.len() as u64,
                kind: ArtifactKind::File,
                unpacked_size: None,
                requires_avx2: false,
            },
            platform: PlatformKey::host().expect("a platform with a catalog entry"),
            vendor_dir: self.vendor_dir(),
            home: self.home(),
            run_root: self.paths.run_root(NAME),
            containment: Arc::new(EnvContainment),
            record: Record::new(self.paths.records_dir(NAME)),
        };
        install(&context, &CancellationToken::new(), |_| {})
            .await
            .expect("the install succeeds");
    }

    fn adapter(&self) -> CliDelegateAdapter {
        CliDelegateAdapter::new(
            DelegateVendor::Claude,
            b"fake-oauth-token".to_vec(),
            RuntimeSource::Vault {
                tools: DelegateTools::new(self.paths.clone(), NAME),
                vendor: "claude",
            },
            Some(DelegateChatContext {
                children: self.children.clone(),
                pending_tool_approvals: Default::default(),
                emit: Arc::new(|_event: &str, _payload: serde_json::Value| {}),
                database: None,
                haex: None,
                deny_rules: Vec::new(),
                tools: None,
            }),
        )
    }

    fn signal_go(&self) {
        fs::write(self.home().join("go"), "").unwrap();
    }

    async fn wait_for(&self, file: &str) -> String {
        let path = self.home().join(file);
        for _ in 0..200 {
            if let Ok(text) = fs::read_to_string(&path) {
                if !text.is_empty() && text.ends_with('\n') {
                    return text.trim().to_string();
                }
            }
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
        panic!("{file} never appeared");
    }
}

fn request() -> ChatRequest {
    ChatRequest {
        sampling: Default::default(),
        model_id: "claude-delegate".to_string(),
        thread_id: None,
        system_prompt: None,
        messages: vec![ChatMessage {
            role: ChatRole::User,
            attachments: Vec::new(),
            content: "hello".to_string(),
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

/// What a run produced: its text, and whether it ended with `Done`.
async fn finish(stream: &mut AdapterStream) -> (String, bool) {
    let mut text = String::new();
    let mut done = false;
    while let Some(chunk) = stream.next().await {
        match chunk {
            Ok(StreamChunk::Delta { content, .. }) => text.push_str(&content),
            Ok(StreamChunk::Done { .. }) => done = true,
            Ok(_) => {}
            Err(_) => return (text, false),
        }
    }
    (text, done)
}

fn alive(pid: i32) -> bool {
    // SAFETY: signal 0 only asks whether the process exists; it is delivered to nobody.
    unsafe { libc::kill(pid, 0) == 0 }
}

async fn wait_until_gone(pid: i32) {
    for _ in 0..200 {
        if !alive(pid) {
            return;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    panic!("process {pid} is still alive");
}

async fn wait_until_unused(dir: &Path) {
    for _ in 0..200 {
        if !vendor_in_use(dir) {
            return;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    panic!("a run still holds its version");
}

#[tokio::test]
async fn an_update_during_a_run_lets_it_finish_and_the_next_run_uses_the_new_version() {
    let vault = Vault::new();
    vault.install_tool(OLD).await;
    let adapter = vault.adapter();
    let mut running = adapter.stream_chat(request()).await.expect("starts");
    vault.wait_for(&format!("started-{OLD}")).await;

    // The update installs next to the version in use while the run goes on.
    vault.install_tool(NEW).await;
    assert_eq!(vault.folders(), [OLD, NEW]);
    vault.signal_go();

    let (text, done) = finish(&mut running).await;
    assert!(done, "the run that was in flight finishes normally");
    assert_eq!(text, format!("from {OLD}"));

    let mut next = adapter.stream_chat(request()).await.expect("starts");
    let (text, done) = finish(&mut next).await;
    assert!(done);
    assert_eq!(
        text,
        format!("from {NEW}"),
        "what starts later uses the new one"
    );

    // The run on the new version ended normally, so it is proven and the old one goes.
    wait_until_unused(&vault.vendor_dir()).await;
    assert!(vault.vendor_dir().join(NEW).join(".confirmed").is_file());
    assert_eq!(vault.folders(), [NEW]);
}

#[tokio::test]
async fn closing_the_vault_ends_a_run_that_is_going_and_leaves_nothing_of_it() {
    let vault = Vault::new();
    vault.install_tool(OLD).await;
    let adapter = vault.adapter();
    let mut running = adapter.stream_chat(request()).await.expect("starts");
    let pid: i32 = vault.wait_for(&format!("pid-{OLD}")).await.parse().unwrap();
    let run_folder = PathBuf::from(vault.wait_for(&format!("run-folder-{OLD}")).await);
    assert!(run_folder.is_dir());
    assert_eq!(vault.children.live(), 1, "the run's process is registered");

    // What the close does: the reply is cancelled, then every registered child is ended.
    running.abort_handle().abort();
    vault.children.kill_all();

    let (_, done) = finish(&mut running).await;
    assert!(!done, "a closed run does not end with a result");
    wait_until_gone(pid).await;
    wait_until_unused(&vault.vendor_dir()).await;
    for _ in 0..100 {
        if !run_folder.exists() {
            break;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    assert!(!run_folder.exists(), "the run folder is gone");
    assert!(
        !vault.vendor_dir().join(OLD).join(".confirmed").exists(),
        "a closed run confirms nothing"
    );
    assert_eq!(vault.children.live(), 0);
    assert!(
        fs::read_dir(vault.paths.run_root(NAME))
            .unwrap()
            .next()
            .is_none(),
        "no run folder is left"
    );
}

#[tokio::test]
async fn a_close_that_only_ends_the_children_still_leaves_no_process_and_no_confirmation() {
    let vault = Vault::new();
    vault.install_tool(OLD).await;
    let adapter = vault.adapter();
    let mut running = adapter.stream_chat(request()).await.expect("starts");
    let pid: i32 = vault.wait_for(&format!("pid-{OLD}")).await.parse().unwrap();

    vault.children.kill_all();

    let (_, done) = finish(&mut running).await;
    assert!(!done);
    wait_until_gone(pid).await;
    wait_until_unused(&vault.vendor_dir()).await;
    assert!(!vault.vendor_dir().join(OLD).join(".confirmed").exists());
    assert!(fs::read_dir(vault.paths.run_root(NAME))
        .unwrap()
        .next()
        .is_none());
}

#[tokio::test]
async fn a_run_started_after_the_close_began_is_ended_at_once() {
    let vault = Vault::new();
    vault.install_tool(OLD).await;
    vault.children.kill_all();
    let adapter = vault.adapter();

    let started = adapter.stream_chat(request()).await;

    if let Ok(mut stream) = started {
        let (_, done) = finish(&mut stream).await;
        assert!(
            !done,
            "a child registered after the close is killed at once"
        );
    }
    wait_until_unused(&vault.vendor_dir()).await;
    assert!(!vault.vendor_dir().join(OLD).join(".confirmed").exists());
}
