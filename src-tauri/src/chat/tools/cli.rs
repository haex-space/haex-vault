//! Host-CLI tool: runs an arbitrary shell command on this device.
//!
//! Always `Risky` (spec.md FR-015) — there is no working-directory or
//! command restriction in this feature's scope, so the permission gate is
//! the only thing standing between the model and an arbitrary local
//! command.

use async_trait::async_trait;
use serde_json::Value;
use std::process::Stdio;
use std::sync::{
    atomic::{AtomicUsize, Ordering},
    Arc,
};
use std::time::Duration;
use tokio::io::{AsyncRead, AsyncReadExt};
use tokio::process::ChildStderr;
use tokio::process::ChildStdout;
use tokio::process::Command;
use tokio_util::sync::CancellationToken;

use super::{RiskClass, Tool, ToolResult};
use crate::vault_gate::ChildRegistry;

const NAME: &str = "run_command";
/// Without the limits, a small model with no fitting tool guessed a command for haex-vault data, e.g.
/// `mail count unread` for "how many unread mails" (spec 046, R18).
const DESCRIPTION: &str = "Runs a shell command on the user's device and returns its output. \
Use it only when the user asks to run something on the device. It reaches programs and files of the device, \
not HaexVault's apps or extensions or their data. Never invent a command.";
const MAX_OUTPUT_BYTES: usize = 1024 * 1024;
const EXECUTION_TIMEOUT: Duration = Duration::from_secs(30);

async fn read_limited<R>(mut reader: R, total: Arc<AtomicUsize>) -> Result<Vec<u8>, &'static str>
where
    R: AsyncRead + Unpin,
{
    let mut output = Vec::new();
    let mut buffer = [0_u8; 8192];
    loop {
        let read = reader
            .read(&mut buffer)
            .await
            .map_err(|_| "failed to read command output")?;
        if read == 0 {
            return Ok(output);
        }
        let within_limit = total
            .try_update(Ordering::AcqRel, Ordering::Relaxed, |current| {
                current
                    .checked_add(read)
                    .filter(|next| *next <= MAX_OUTPUT_BYTES)
            })
            .is_ok();
        if !within_limit {
            return Err("command output exceeded the 1 MiB limit");
        }
        output.extend_from_slice(&buffer[..read]);
    }
}

/// Registered unconditionally in every `ChatState` (T019) — unlike MCP
/// tools, this one is never connection-dependent. Every shell it starts is registered with the
/// vault gate's child registry, so ending the vault ends the shell and what it started.
#[derive(Default)]
pub struct CliTool {
    children: ChildRegistry,
}

impl CliTool {
    pub fn new(children: ChildRegistry) -> Self {
        Self { children }
    }
}

fn input_schema() -> Value {
    serde_json::json!({
        "type": "object",
        "properties": {
            "command": {
                "type": "string",
                "description": "The shell command to execute, e.g. \"ls -la\"."
            }
        },
        "required": ["command"],
    })
}

#[async_trait]
impl Tool for CliTool {
    fn name(&self) -> &str {
        NAME
    }

    fn description(&self) -> &str {
        DESCRIPTION
    }

    fn source(&self) -> &'static str {
        "cli"
    }

    fn input_schema(&self) -> Value {
        input_schema()
    }

    fn risk_class(&self) -> RiskClass {
        RiskClass::Risky
    }

    async fn execute(&self, input: Value, cancel: CancellationToken) -> ToolResult {
        let Some(command) = input.get("command").and_then(Value::as_str) else {
            return ToolResult::error("missing required \"command\" string input");
        };

        // A shell (`sh -c` / `cmd /C`) is used deliberately so the model can
        // rely on shell features (pipes, globs, redirection) — spec.md
        // FR-015 already accepts no command/working-directory restriction,
        // so there is no additional risk introduced by going through a
        // shell versus exec'ing a single argv. Desktop-portable per plan.md
        // Target Platform: every OS this feature targets has one of the two.
        #[cfg(not(windows))]
        let mut cmd = {
            let mut cmd = Command::new("sh");
            cmd.arg("-c").arg(command);
            cmd
        };
        #[cfg(windows)]
        let mut cmd = {
            let mut cmd = Command::new("cmd");
            cmd.arg("/C").arg(command);
            cmd
        };
        cmd.stdout(Stdio::piped()).stderr(Stdio::piped());
        // Own process group so a kill can reach a grandchild the shell
        // forks (e.g. for a pipeline) — `Child::kill` below only ever
        // signals this one PID, never anything it spawned in turn (T032).
        // Windows has no equivalent at spawn time; `taskkill /T` below
        // walks the OS's own parent-child process tree instead.
        #[cfg(unix)]
        cmd.process_group(0);
        if cancel.is_cancelled() {
            return ToolResult::error("tool_call_cancelled");
        }
        let mut child = match cmd.spawn() {
            Ok(child) => child,
            Err(e) => return ToolResult::error(format!("failed to spawn command: {e}")),
        };

        // Registered from the first moment the shell exists; the entry goes as soon as it is reaped.
        let child_registration = child.id().map(|pid| self.children.register(pid));

        let Some(stdout): Option<ChildStdout> = child.stdout.take() else {
            return ToolResult::error("failed to capture command stdout");
        };
        let Some(stderr): Option<ChildStderr> = child.stderr.take() else {
            return ToolResult::error("failed to capture command stderr");
        };
        let total = Arc::new(AtomicUsize::new(0));
        let mut stdout_task = Some(tokio::spawn(read_limited(stdout, total.clone())));
        let mut stderr_task = Some(tokio::spawn(read_limited(stderr, total)));
        let mut stdout_bytes = None;
        let mut stderr_bytes = None;

        enum Outcome {
            Finished(Result<(std::process::ExitStatus, Vec<u8>, Vec<u8>), &'static str>),
            TimedOut,
            Cancelled,
        }

        let outcome = tokio::select! {
            result = tokio::time::timeout(EXECUTION_TIMEOUT, async {
                while stdout_bytes.is_none() || stderr_bytes.is_none() {
                    match (stdout_bytes.is_none(), stderr_bytes.is_none()) {
                        (true, true) => tokio::select! {
                            result = stdout_task.as_mut().unwrap() => {
                                stdout_task = None;
                                stdout_bytes = Some(result.map_err(|_| "output reader task failed")??);
                            }
                            result = stderr_task.as_mut().unwrap() => {
                                stderr_task = None;
                                stderr_bytes = Some(result.map_err(|_| "output reader task failed")??);
                            }
                        },
                        (true, false) => {
                            let result = stdout_task.take().unwrap().await;
                            stdout_bytes = Some(result.map_err(|_| "output reader task failed")??);
                        }
                        (false, true) => {
                            let result = stderr_task.take().unwrap().await;
                            stderr_bytes = Some(result.map_err(|_| "output reader task failed")??);
                        }
                        (false, false) => unreachable!(),
                    }
                }
                let status = child
                    .wait()
                    .await
                    .map_err(|_| "failed to wait for command")?;
                Ok::<_, &'static str>((status, stdout_bytes.unwrap(), stderr_bytes.unwrap()))
            }) => {
                match result {
                    Ok(inner) => Outcome::Finished(inner),
                    Err(_) => Outcome::TimedOut,
                }
            }
            _ = cancel.cancelled() => Outcome::Cancelled,
        };

        let failed = !matches!(&outcome, Outcome::Finished(Ok(_)));
        if failed {
            // `Child::kill` only signals this one PID — a grandchild the
            // shell forked (e.g. one side of a pipeline) would otherwise
            // survive, orphaned, running to completion on its own. Signal
            // the whole group instead (`process_group(0)` at spawn made
            // this PID its own group leader too, so this also covers the
            // shell itself).
            #[cfg(unix)]
            if let Some(pid) = child.id() {
                // SAFETY: FFI call with a plain integer PID and signal
                // constant, no pointers involved.
                unsafe {
                    libc::kill(-(pid as libc::pid_t), libc::SIGKILL);
                }
            }
            // Windows has no process-group signal; `taskkill /T` walks the
            // OS's own parent-child tree from this PID instead, which
            // covers `cmd /C`'s own children the same way the group kill
            // does on Unix.
            #[cfg(windows)]
            if let Some(pid) = child.id() {
                let _ = tokio::process::Command::new("taskkill")
                    .args(["/T", "/F", "/PID", &pid.to_string()])
                    .output()
                    .await;
            }
            // Reap explicitly rather than relying on `Child`'s Drop — that
            // fallback only sends the kill signal without waiting, which
            // would leave a zombie until something else reaps it (T032).
            let _ = child.kill().await;
            let _ = child.wait().await;
            if let Some(task) = stdout_task {
                task.abort();
                let _ = task.await;
            }
            if let Some(task) = stderr_task {
                task.abort();
                let _ = task.await;
            }
        }

        drop(child_registration);

        match outcome {
            Outcome::Finished(Ok((status, stdout, stderr))) => {
                let mut content = String::from_utf8_lossy(&stdout).into_owned();
                if !stderr.is_empty() {
                    if !content.is_empty() {
                        content.push('\n');
                    }
                    content.push_str(&String::from_utf8_lossy(&stderr));
                }
                if status.success() {
                    ToolResult::ok(content)
                } else {
                    let code = status
                        .code()
                        .map(|c| c.to_string())
                        .unwrap_or_else(|| "signal".to_string());
                    if content.is_empty() {
                        content = format!("command exited with status {code}");
                    }
                    ToolResult::error(content)
                }
            }
            Outcome::Finished(Err(reason)) => ToolResult::error(reason),
            Outcome::TimedOut => ToolResult::error("command timed out after 30 seconds"),
            Outcome::Cancelled => ToolResult::error("tool_call_cancelled"),
        }
    }
}
