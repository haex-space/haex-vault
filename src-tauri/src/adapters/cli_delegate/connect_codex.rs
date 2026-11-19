//! `codex login --device-auth` connect-flow driver (spec 007-cli-delegate US2).
//!
//! Plain piped stdio, no PTY needed — confirmed live (research.md §5): unlike
//! `claude setup-token` (`connect_claude.rs`), this prints a URL and a
//! one-time code as ordinary lines and then polls in the background until the
//! user enters the code on the website, so one command covers the whole flow.

use std::process::Stdio;
use std::time::Duration;

use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::Command;

use crate::adapters::AdapterError;
use crate::cli_tools::runtime::DelegateRuntime;

use super::DelegateVendor;
use super::codex::read_limited;
use super::process::{ChildLifecycle, configure_process_group, map_spawn_error};
use crate::vault_gate::ChildRegistry;

/// Matches the CLI's own stated code lifetime ("expires in 15 minutes",
/// research.md §5) plus headroom for the user to actually complete the
/// browser step.
const DEVICE_AUTH_TIMEOUT: Duration = Duration::from_secs(20 * 60);

/// The URL/code pair `codex login --device-auth` prints for the user to act
/// on (research.md §5).
#[derive(Debug, PartialEq, Eq)]
pub struct DeviceAuthPrompt {
    pub url: String,
    pub code: String,
}

/// Scans accumulated stdout (plain text — `codex login --device-auth` uses
/// color codes only, no cursor positioning, so no ANSI stripping is needed
/// beyond what the terminal itself would already ignore as SGR sequences
/// embedded in otherwise-plain lines) for the device-auth URL and one-time
/// code. Pure and unit-testable.
pub(super) fn parse_device_auth_prompt(text: &str) -> Option<DeviceAuthPrompt> {
    let strip_sgr = |line: &str| -> String {
        let mut out = String::with_capacity(line.len());
        let mut chars = line.chars().peekable();
        while let Some(c) = chars.next() {
            if c == '\u{1b}' && chars.peek() == Some(&'[') {
                chars.next();
                for next in chars.by_ref() {
                    if next.is_ascii_alphabetic() {
                        break;
                    }
                }
            } else {
                out.push(c);
            }
        }
        out
    };
    let cleaned: Vec<String> = text.lines().map(|l| strip_sgr(l.trim())).collect();
    let url = cleaned
        .iter()
        .find(|line| line.starts_with("https://"))?
        .clone();
    let code = cleaned.iter().find(|line| is_device_code(line))?.clone();
    Some(DeviceAuthPrompt { url, code })
}

/// A device code looks like `GL00-DVEC2`: short, uppercase alphanumeric,
/// exactly one hyphen — distinct enough from surrounding prose lines not to
/// need a regex dependency for one call site.
fn is_device_code(line: &str) -> bool {
    (6..=16).contains(&line.len())
        && line.contains('-')
        && line
            .chars()
            .all(|c| c.is_ascii_uppercase() || c.is_ascii_digit() || c == '-')
}

/// Runs `codex login --device-auth` to completion in an isolated `CODEX_HOME`
/// (spec.md FR-003), calling `on_prompt` once with the extracted URL/code as
/// soon as they appear, then waiting for the process to exit successfully and
/// returning the resulting `auth.json` bytes (data-model.md `credentials`
/// shape — unchanged from the chat-invocation path's expectation, `codex.rs`).
pub async fn run_device_auth(
    runtime: &DelegateRuntime,
    children: &ChildRegistry,
    mut on_prompt: impl FnMut(&DeviceAuthPrompt) + Send,
) -> Result<Vec<u8>, AdapterError> {
    super::process::ensure_available(crate::platform::capabilities().cli_delegates)?;
    // The run folder lies in the vault's home and is removed when this flow ends (spec 094).
    let tmp = runtime.run_folder().map_err(|error| AdapterError::Http {
        reason: format!("failed to create the run folder for codex login: {error}"),
    })?;

    let mut cmd = Command::new(&runtime.executable);
    cmd.args(&runtime.vendor.config_args);
    cmd.arg("login").arg("--device-auth");
    cmd.current_dir(tmp.path())
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    runtime.contain(cmd.as_std_mut(), &tmp);
    configure_process_group(&mut cmd);

    let mut child = ChildLifecycle::spawn(&mut cmd, children)
        .map_err(|error| map_spawn_error(DelegateVendor::Codex, error))?;

    let stdout = child
        .child_mut()
        .stdout
        .take()
        .expect("stdout piped at spawn");
    let stderr = child
        .child_mut()
        .stderr
        .take()
        .expect("stderr piped at spawn");
    let stderr_task = tokio::spawn(read_limited(stderr));

    let run = async {
        let mut lines = BufReader::new(stdout).lines();
        let mut buf = String::new();
        let mut prompted = false;
        loop {
            match lines.next_line().await {
                Ok(Some(line)) => {
                    buf.push_str(&line);
                    buf.push('\n');
                    if !prompted {
                        if let Some(prompt) = parse_device_auth_prompt(&buf) {
                            on_prompt(&prompt);
                            prompted = true;
                        }
                    }
                }
                Ok(None) => return Ok(()),
                Err(error) => {
                    return Err(AdapterError::Http {
                        reason: format!("codex login stdout read failed: {error}"),
                    });
                }
            }
        }
    };

    let run_result = tokio::time::timeout(DEVICE_AUTH_TIMEOUT, run).await;
    let status = match run_result {
        Ok(Ok(())) => child.child_mut().wait().await,
        Ok(Err(error)) => {
            child.terminate_and_reap().await;
            stderr_task.abort();
            return Err(error);
        }
        Err(_) => {
            child.terminate_and_reap().await;
            stderr_task.abort();
            return Err(AdapterError::Http {
                reason: format!(
                    "codex device-auth login did not complete within {DEVICE_AUTH_TIMEOUT:?}"
                ),
            });
        }
    };

    // Its output is drained so the child never blocks on a full pipe; none of it is logged.
    let _ = stderr_task.await;
    let status = status.map_err(|error| AdapterError::Http {
        reason: format!("codex login wait failed: {error}"),
    })?;

    if !status.success() {
        // Fixed text only: the tool's own output can name the account or a path, and a
        // device-wide log must say nothing about a vault (spec 094 FR-019).
        log::warn!("codex device login ended without success");
        return Err(AdapterError::InvalidCredentials);
    }

    let credentials = tokio::fs::read(tmp.path().join("auth.json"))
        .await
        .map_err(|error| AdapterError::Http {
            reason: format!("codex login succeeded but auth.json is missing: {error}"),
        })?;
    if credentials.is_empty() {
        return Err(AdapterError::InvalidCredentials);
    }
    // The credential is obtained: the version has proven itself (spec 094 FR-022).
    runtime.confirm_run().await;
    Ok(credentials)
}
