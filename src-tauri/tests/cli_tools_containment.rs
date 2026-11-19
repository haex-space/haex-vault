//! A delegate runs from the vault's own installation and leaves nothing on the host (spec 094
//! US2, T037 to T039): the host's `PATH` and folders play no part (SC-002, SC-003), and no log
//! record names the vault, a path or request text (SC-009).
//!
//! The tool is installed through the real install pipeline from a local server: a small shell
//! script that answers `--version` and otherwise acts as a Claude stream. Unix only, because the
//! tool is a script.

#![cfg(unix)]

use std::collections::BTreeSet;
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, OnceLock};

use holzi_lib::adapters::cli_delegate::{CliDelegateAdapter, DelegateChatContext, DelegateVendor};
use holzi_lib::adapters::{
    AdapterError, ChatMessage, ChatRequest, ChatRole, ProviderAdapter, StreamChunk,
};
use holzi_lib::cli_tools::catalog::{Artifact, ArtifactKind, Catalog};
use holzi_lib::cli_tools::containment::EnvContainment;
use holzi_lib::cli_tools::install::{install, InstallContext};
use holzi_lib::cli_tools::platform::PlatformKey;
use holzi_lib::cli_tools::record::{Record, RECORD_FILE};
use holzi_lib::cli_tools::runtime::{DelegateTools, RuntimeSource};
use holzi_lib::instances::vault_paths::VaultPaths;
use sha2::{Digest, Sha256};
use tokio_util::sync::CancellationToken;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

const VERSION: &str = "1.0.0";
const REQUEST_TEXT: &str = "Secret-request-text-9137";

/// The tool: `--version` names the installed version; a run acts as a Claude stream and leaves
/// what it saw in the vault's home (the environment, which program it was, one file at each of
/// the places a real tool writes to).
const TOOL: &str = r#"#!/bin/sh
case " $* " in
  *" --version "*) echo "claude 1.0.0 (stub)"; exit 0 ;;
esac
cat >/dev/null
env > "$HOME/seen-env.txt"
echo "$0" > "$HOME/seen-program.txt"
echo x > "$HOME/wrote-home"
echo x > "$TMPDIR/wrote-tmp"
echo x > "$CLAUDE_CONFIG_DIR/wrote-config"
cat <<'EOF'
{"type":"system","subtype":"init"}
{"type":"stream_event","event":{"type":"content_block_delta","delta":{"type":"text_delta","text":"ok"}}}
{"type":"result","is_error":false,"subtype":"success","result":"ok","usage":{"input_tokens":1,"output_tokens":1}}
EOF
"#;

/// Tests here change process-wide environment variables and read the logger, so they run one
/// after the other.
static SERIAL: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

struct Capture(Mutex<Vec<String>>);

impl log::Log for Capture {
    fn enabled(&self, _: &log::Metadata) -> bool {
        true
    }
    fn log(&self, record: &log::Record) {
        self.0
            .lock()
            .unwrap()
            .push(format!("{} {}", record.target(), record.args()));
    }
    fn flush(&self) {}
}

fn captured() -> &'static Capture {
    static CAPTURE: OnceLock<&'static Capture> = OnceLock::new();
    CAPTURE.get_or_init(|| {
        let capture: &'static Capture = Box::leak(Box::new(Capture(Mutex::new(Vec::new()))));
        log::set_logger(capture).expect("no other logger in this test binary");
        log::set_max_level(log::LevelFilter::Trace);
        capture
    })
}

/// Sets environment variables for a test and puts the previous values back afterwards.
struct HostEnv(Vec<(&'static str, Option<std::ffi::OsString>)>);

impl HostEnv {
    fn set(vars: &[(&'static str, Option<&str>)]) -> Self {
        let before = vars
            .iter()
            .map(|(name, _)| (*name, std::env::var_os(name)))
            .collect();
        for (name, value) in vars {
            match value {
                Some(value) => std::env::set_var(name, value),
                None => std::env::remove_var(name),
            }
        }
        Self(before)
    }
}

impl Drop for HostEnv {
    fn drop(&mut self) {
        for (name, value) in self.0.drain(..) {
            match value {
                Some(value) => std::env::set_var(name, value),
                None => std::env::remove_var(name),
            }
        }
    }
}

struct Vault {
    paths: VaultPaths,
    name: String,
    _dir: tempfile::TempDir,
}

impl Vault {
    fn new(name: &str) -> Self {
        let dir = tempfile::tempdir().expect("tempdir");
        Self {
            paths: VaultPaths::new(dir.path()),
            name: name.to_string(),
            _dir: dir,
        }
    }

    fn tools(&self) -> DelegateTools {
        DelegateTools::new(self.paths.clone(), self.name.clone())
    }

    fn home(&self) -> PathBuf {
        self.paths.home(&self.name)
    }

    fn folder(&self) -> PathBuf {
        self.paths.folder(&self.name)
    }

    /// Installs the tool from a local server, as the Settings page does.
    async fn install_tool(&self) {
        let body = TOOL.as_bytes().to_vec();
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/tool"))
            .respond_with(ResponseTemplate::new(200).set_body_bytes(body.clone()))
            .mount(&server)
            .await;
        let context = InstallContext {
            vendor: Arc::new(Catalog::shipped().vendor("claude").unwrap().clone()),
            version: VERSION.to_string(),
            artifact: Artifact {
                url: format!("{}/tool", server.uri()),
                sha256: Sha256::digest(&body)
                    .iter()
                    .map(|byte| format!("{byte:02x}"))
                    .collect(),
                size: body.len() as u64,
                kind: ArtifactKind::File,
                unpacked_size: None,
            },
            platform: PlatformKey::host().expect("a platform with a catalog entry"),
            vendor_dir: self.paths.cli_tool_dir(&self.name, "claude"),
            home: self.home(),
            run_root: self.paths.run_root(&self.name),
            containment: Arc::new(EnvContainment),
            record: Record::new(self.paths.records_dir(&self.name)),
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
                tools: self.tools(),
                vendor: "claude",
            },
            Some(DelegateChatContext {
                children: Default::default(),
                pending_tool_approvals: Arc::new(Mutex::new(Default::default())),
                emit: Arc::new(|_event: &str, _payload: serde_json::Value| {}),
                database: None,
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
        model_id: "claude-delegate".to_string(),
        thread_id: None,
        system_prompt: None,
        messages: vec![ChatMessage {
            role: ChatRole::User,
            attachments: Vec::new(),
            content: REQUEST_TEXT.to_string(),
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

/// Runs one request to its end.
async fn run_request(adapter: &CliDelegateAdapter) {
    let mut stream = adapter
        .stream_chat(request())
        .await
        .map_err(|error| error.to_string())
        .expect("the run starts");
    let mut saw_done = false;
    while let Some(chunk) = stream.next().await {
        if matches!(chunk.expect("no stream error"), StreamChunk::Done { .. }) {
            saw_done = true;
        }
    }
    assert!(saw_done, "the stream ended without Done");
}

/// Every path below `root`, relative to it.
fn tree(root: &Path) -> BTreeSet<String> {
    fn walk(base: &Path, dir: &Path, out: &mut BTreeSet<String>) {
        let Ok(entries) = fs::read_dir(dir) else {
            return;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            out.insert(path.strip_prefix(base).unwrap().display().to_string());
            if entry.file_type().is_ok_and(|kind| kind.is_dir()) {
                walk(base, &path, out);
            }
        }
    }
    let mut out = BTreeSet::new();
    walk(root, root, &mut out);
    out
}

fn env_value(dump: &str, name: &str) -> Option<String> {
    dump.lines()
        .find_map(|line| line.strip_prefix(&format!("{name}=")))
        .map(str::to_string)
}

fn executable_script(path: &Path, body: &str) {
    fs::write(path, body).unwrap();
    fs::set_permissions(path, fs::Permissions::from_mode(0o755)).unwrap();
}

#[tokio::test]
async fn a_run_leaves_the_host_alone_and_its_environment_is_the_contract() {
    let _serial = SERIAL.lock().await;
    let vault = Vault::new("Containment");
    vault.install_tool().await;

    // The host: its own home, temp folder and XDG folder, an account key for each vendor, a
    // proxy, a locale. Nothing of it may change or be written to.
    let host = tempfile::tempdir().expect("host dir");
    let host_home = host.path().join("home");
    let host_tmp = host.path().join("tmp");
    fs::create_dir_all(&host_home).unwrap();
    fs::create_dir_all(&host_tmp).unwrap();
    let host_path = std::env::var("PATH").expect("PATH");
    let before = tree(host.path());
    let _env = HostEnv::set(&[
        ("HOME", Some(host_home.to_str().unwrap())),
        ("TMPDIR", Some(host_tmp.to_str().unwrap())),
        (
            "XDG_CONFIG_HOME",
            Some(host_home.join(".config").to_str().unwrap()),
        ),
        ("XDG_CACHE_HOME", None),
        ("ANTHROPIC_API_KEY", Some("host-anthropic-key")),
        ("OPENAI_API_KEY", Some("host-openai-key")),
        ("HTTPS_PROXY", Some("http://proxy.example:3128")),
        ("LANG", Some("de_DE.UTF-8")),
    ]);

    run_request(&vault.adapter()).await;

    let home = vault.home();
    let seen =
        fs::read_to_string(home.join("seen-env.txt")).expect("the tool wrote its environment");
    let run_tmp = PathBuf::from(env_value(&seen, "TMPDIR").expect("run TMPDIR"));
    assert_eq!(env_value(&seen, "HOME"), Some(home.display().to_string()));
    assert!(
        env_value(&seen, "TMPDIR")
            .unwrap()
            .starts_with(&home.display().to_string()),
        "TMPDIR is a folder of the vault"
    );
    assert!(env_value(&seen, "CLAUDE_CONFIG_DIR")
        .unwrap()
        .starts_with(&home.display().to_string()));
    assert_eq!(
        env_value(&seen, "PATH"),
        Some(host_path),
        "PATH reaches the tool unchanged"
    );
    assert_eq!(
        env_value(&seen, "HTTPS_PROXY").as_deref(),
        Some("http://proxy.example:3128")
    );
    assert_eq!(env_value(&seen, "LANG").as_deref(), Some("de_DE.UTF-8"));
    assert_eq!(
        env_value(&seen, "ANTHROPIC_API_KEY"),
        None,
        "the host's account key does not reach the tool"
    );
    // Only the vendor's own account variables go: another vendor's key is not Claude's business,
    // and holzi leaves what it has no reason to touch (the Codex run removes `OPENAI_` in turn).
    assert_eq!(
        env_value(&seen, "OPENAI_API_KEY").as_deref(),
        Some("host-openai-key")
    );
    assert_eq!(
        env_value(&seen, "XDG_CONFIG_HOME"),
        Some(format!("{}/.config", home.display())),
        "an XDG folder the host defines is redirected into the vault"
    );
    assert_eq!(
        env_value(&seen, "XDG_CACHE_HOME"),
        None,
        "an XDG folder the host leaves unset stays unset"
    );

    assert_eq!(
        tree(host.path()),
        before,
        "nothing on the host changed (SC-003)"
    );
    assert!(home.join("wrote-home").exists());
    assert!(
        !run_tmp.exists(),
        "the observed temp folder of a run goes with the run"
    );
}

#[tokio::test]
async fn a_host_copy_on_the_path_is_never_started() {
    let _serial = SERIAL.lock().await;
    let vault = Vault::new("PathNeverUsed");

    let bin = tempfile::tempdir().expect("host bin");
    let marker = bin.path().join("host-copy-ran");
    executable_script(
        &bin.path().join("claude"),
        &format!("#!/bin/sh\necho ran > '{}'\n", marker.display()),
    );
    let host_path = format!(
        "{}:{}",
        bin.path().display(),
        std::env::var("PATH").expect("PATH")
    );
    let _env = HostEnv::set(&[("PATH", Some(&host_path))]);

    // Without an installation in the vault the run ends, and the host copy is not the fallback.
    let adapter = vault.adapter();
    let error = match adapter.stream_chat(request()).await {
        Ok(_) => panic!("a run without an installation must not start"),
        Err(error) => error,
    };
    match error {
        AdapterError::Unavailable { reason } => {
            assert_eq!(reason, "not installed in this vault: claude");
        }
        other => panic!("unexpected error: {other}"),
    }
    assert!(!marker.exists(), "the host copy was started");

    // With one, the vault's copy is the one that runs.
    vault.install_tool().await;
    run_request(&adapter).await;
    assert!(!marker.exists(), "the host copy was started");
    let program = fs::read_to_string(vault.home().join("seen-program.txt")).unwrap();
    assert!(
        program.trim().starts_with(
            &vault
                .paths
                .cli_tool_dir(&vault.name, "claude")
                .display()
                .to_string()
        ),
        "the program was {program}"
    );
}

#[tokio::test]
async fn no_log_record_names_the_vault_a_path_or_a_request_and_the_vault_keeps_its_own_record() {
    let _serial = SERIAL.lock().await;
    let capture = captured();
    let vault = Vault::new("Geheimtresor-4711");

    vault.install_tool().await;
    run_request(&vault.adapter()).await;
    // A run that cannot start is logged too, so it is part of what is checked.
    let missing = Vault::new("Geheimtresor-0815");
    let _ = missing.adapter().stream_chat(request()).await;

    let lines = capture.0.lock().unwrap().clone();
    let vault_folder = vault.folder().display().to_string();
    let missing_folder = missing.folder().display().to_string();
    let device_wide_names = [
        "Geheimtresor",
        REQUEST_TEXT,
        "cli-tools",
        vault_folder.as_str(),
        missing_folder.as_str(),
    ];
    for line in &lines {
        for needle in device_wide_names {
            assert!(
                !line.contains(needle),
                "a log record names {needle:?}: {line}"
            );
        }
    }

    let record = vault.paths.records_dir(&vault.name).join(RECORD_FILE);
    let text = fs::read_to_string(&record).expect("the vault keeps its own record");
    assert!(text.contains("install_finished"), "record: {text}");
    assert!(text.contains("run_started"), "record: {text}");
    assert!(
        !text.contains(REQUEST_TEXT),
        "the record holds no request text"
    );
    assert!(
        record.starts_with(vault.folder()),
        "the record lies in the vault folder"
    );
}
