//! Integration tests for the `connect_cli_delegate`/`submit_cli_delegate_code`
//! acquisition drivers (tasks.md T021/T021b) — each points at a stub script
//! standing in for the real `codex`/`claude` binary, matching the real wire
//! shapes captured live in research.md §5 (device-auth URL/code lines for
//! Codex, an OSC-8-wrapped URL plus a paste-back prompt for Claude).
//!
//! Unix-only: both stubs are `#!/bin/sh` scripts invoked directly, matching
//! how the real drivers spawn their binaries.

#![cfg(unix)]

use std::fs;
use std::os::unix::fs::PermissionsExt;

use haex_lib::adapters::cli_delegate::connect_claude::{start_claude_connect, submit_claude_code};
use haex_lib::adapters::cli_delegate::connect_codex::run_device_auth;
use haex_lib::cli_tools::runtime::DelegateRuntime;
use haex_lib::vault_gate::ChildRegistry;

fn write_stub(dir: &std::path::Path, name: &str, script: &str) -> std::path::PathBuf {
    let path = dir.join(name);
    fs::write(&path, script).expect("write stub script");
    let mut perms = fs::metadata(&path).unwrap().permissions();
    perms.set_mode(0o755);
    fs::set_permissions(&path, perms).expect("chmod stub script");
    path
}

#[tokio::test]
async fn codex_device_auth_reports_progress_and_returns_auth_json() {
    let dir = tempfile::tempdir().expect("tempdir");
    let stub = write_stub(
        dir.path(),
        "fake-codex",
        r#"#!/bin/sh
# The vendor's switches come before the subcommand (spec 094): look at the end.
case "$*" in *"login --device-auth")
  echo "1. Open this link in your browser and sign in to your account"
  echo "   https://auth.openai.com/codex/device"
  echo ""
  echo "2. Enter this one-time code (expires in 15 minutes)"
  echo "   AB12-CD34"
  echo ""
  printf '{"fake":"auth"}' > "$CODEX_HOME/auth.json"
  exit 0 ;;
esac
exit 1
"#,
    );

    let (tx, rx) = tokio::sync::oneshot::channel();
    let mut tx = Some(tx);
    let auth_bytes = run_device_auth(
        &DelegateRuntime::for_stub("codex", &stub, dir.path()),
        &ChildRegistry::default(),
        move |prompt| {
            if let Some(tx) = tx.take() {
                let _ = tx.send((prompt.url.clone(), prompt.code.clone()));
            }
        },
    )
    .await
    .expect("device-auth should succeed");

    let (url, code) = rx.await.expect("progress callback should have fired");
    assert_eq!(url, "https://auth.openai.com/codex/device");
    assert_eq!(code, "AB12-CD34");
    assert_eq!(auth_bytes, br#"{"fake":"auth"}"#);
}

#[tokio::test]
async fn codex_device_auth_non_zero_exit_is_invalid_credentials() {
    let dir = tempfile::tempdir().expect("tempdir");
    let stub = write_stub(
        dir.path(),
        "fake-codex-fail",
        "#!/bin/sh\necho \"denied\" 1>&2\nexit 1\n",
    );

    let error = run_device_auth(
        &DelegateRuntime::for_stub("codex", &stub, dir.path()),
        &ChildRegistry::default(),
        |_| {},
    )
    .await
    .expect_err("a non-zero exit should fail the connect flow");
    assert!(matches!(
        error,
        haex_lib::adapters::AdapterError::InvalidCredentials
    ));
}

#[tokio::test]
async fn claude_setup_token_pty_flow_yields_url_then_token_after_code_submission() {
    let dir = tempfile::tempdir().expect("tempdir");
    let stub = write_stub(
        dir.path(),
        "fake-claude",
        r#"#!/bin/sh
if [ "$1" = "setup-token" ]; then
  printf '\033]8;id=1;https://claude.com/fake/oauth?state=abc\007click here to sign in\033]8;;\007\n'
  printf 'Paste code here if prompted > '
  read -r code
  printf '\nYour long-lived token:\n\nsk-ant-oat01-FAKETOKEN123\n'
  exit 0
fi
exit 1
"#,
    );

    let (mut session, url) = start_claude_connect(
        &DelegateRuntime::for_stub("claude", &stub, dir.path()),
        ChildRegistry::default(),
    )
    .await
    .expect("claude setup-token should start and report a url");
    assert_eq!(url, "https://claude.com/fake/oauth?state=abc");

    let token = submit_claude_code(&mut session, "123456")
        .await
        .expect("submitting the code should yield a token");
    assert_eq!(token, b"sk-ant-oat01-FAKETOKEN123");
}
