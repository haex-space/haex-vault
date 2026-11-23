//! The Abacus AI delegate: the vault's copy of `abacusai` driven in its JSON-lines mode, one
//! process per request (spec 096 FR-007 to FR-010, `data-model.md` §4).
//!
//! The program runs as `-p --input-format stream-json --output-format stream-json
//! --permission-mode supervise`: the host writes one `user` line, reads `event` lines (the text
//! arrives as `text_delta` events), answers every `can_use_tool` control request through the
//! approval bridge with `accept` or `reject` and nothing else (never a persistent variant), and
//! sees the turn end with a `result` line. The account's device code is an entry of the password
//! manager; it reaches the process as `CODELLM_DEVICE_CODE` after containment and never as a
//! file or an argument. `ABACUS_API_KEY` is never set.
//!
//! Only the approving posture (`Standard`) is served for now: the two autonomy modes of spec 009
//! need their own decision on what Abacus' tool names and inputs mean (spec 096 US3), so they end
//! with "autonomy mode unavailable" before anything starts.

use std::path::Path;
use std::process::Stdio;
use std::time::{Duration, Instant};

use serde_json::{json, Value};
use tokio::io::{AsyncBufReadExt, AsyncRead, AsyncReadExt, AsyncWriteExt, BufReader, Lines};
use tokio::process::{ChildStdin, ChildStdout, Command};
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;

use crate::adapters::types::{StreamChunk, StreamError};
use crate::adapters::{AdapterError, AdapterStream, ChatRequest, ProviderModel};
use crate::cli_tools::runtime::{DelegateRuntime, RunFolder};
use crate::model_capabilities::ModelCapabilities;
use crate::passwords::binaries::sanitize_file_name;
use crate::providers::delegate_credential::{self, Read, KIND_DEVICE_CODE};

use super::autonomy::{ApprovalRequestPayload, AutonomyMode, AutonomyUnavailable};
use super::codex::MAX_STDERR_BYTES;
use super::process::{configure_process_group, map_spawn_error, ChildLifecycle};
use super::{
    approval_bridge, build_transcript_prompt, haex_mcp, DelegateChatContext, DelegateVendor,
};

/// The environment variable that carries the device code to the program.
const DEVICE_CODE_ENV: &str = "CODELLM_DEVICE_CODE";

/// The permission mode the program is started in and must report in its `init` message.
const PERMISSION_MODE: &str = "supervise";

/// The release gate of spec 096 T034. Flip only after T035 records that read-only and changing
/// tools both reach the approval bridge.
pub(super) const READ_ONLY_TOOLS_REACH_APPROVAL_BRIDGE: bool = false;

/// How long the program may take to write its first line (or to end) after it started.
const STARTUP_PROBE_TIMEOUT: Duration = Duration::from_secs(60);

/// The reasons an Abacus request ends with that the frontend words (`errors.delegate.*`).
pub(super) const NOT_CONNECTED: &str = "delegate.not_connected: abacusai";
pub(super) const CREDENTIAL_REFUSED: &str = "delegate.credential_refused: abacusai";
pub(super) const APPROVALS_UNVERIFIED: &str = "delegate.approvals_unverified: abacusai";

/// The `Unavailable` error of a request that has no credential in this scope.
pub(super) fn not_connected() -> AdapterError {
    AdapterError::Unavailable {
        reason: NOT_CONNECTED.into(),
    }
}

/// The `Unavailable` error of a request whose credential the service refused: the person connects
/// again, nothing is retried and no other backend is tried.
pub(super) fn credential_refused() -> AdapterError {
    AdapterError::Unavailable {
        reason: CREDENTIAL_REFUSED.into(),
    }
}

fn approvals_unverified() -> AdapterError {
    AdapterError::Unavailable {
        reason: APPROVALS_UNVERIFIED.into(),
    }
}

/// The single model of the account: its own default, chosen by the program (spec 096 plan D7).
pub(super) fn default_model() -> ProviderModel {
    ProviderModel {
        remote_id: DelegateVendor::Abacus.as_str().to_owned(),
        display_name: "Abacus AI".to_owned(),
        context_window: None,
        capabilities: ModelCapabilities::default(),
    }
}

/// The model id to hand to the program, or none when the request names the account default (the
/// vendor id, or an empty id).
pub(super) fn chosen_model(model_id: &str) -> Option<&str> {
    let model_id = model_id.trim();
    (!model_id.is_empty() && model_id != DelegateVendor::Abacus.as_str()).then_some(model_id)
}

/// The vault's own installation started in its JSON-lines mode, contained to the vault. The device
/// code is not set here: it is set on the child after containment, in [`spawn_abacus_invocation`].
pub(super) fn build_command(
    runtime: &DelegateRuntime,
    model_id: &str,
    mcp_config: Option<&Path>,
    run: &RunFolder,
) -> Command {
    let mut cmd = Command::new(&runtime.executable);
    cmd.args(&runtime.vendor.config_args);
    cmd.arg("-p")
        .arg("--input-format")
        .arg("stream-json")
        .arg("--output-format")
        .arg("stream-json")
        .arg("--include-partial-messages")
        .arg("--permission-mode")
        .arg(PERMISSION_MODE)
        .arg("--no-agents-md");
    if let Some(model) = chosen_model(model_id) {
        cmd.arg("--model").arg(model);
    }
    if let Some(config) = mcp_config {
        cmd.arg("--mcp-config").arg(config);
    }
    cmd.current_dir(run.path())
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    runtime.contain(cmd.as_std_mut(), run);
    cmd
}

/// What one line of the program's output means to the adapter.
#[derive(Debug, Clone, PartialEq)]
pub(super) enum Line {
    /// `system` / `init`: the mode the program runs in.
    Init { permission_mode: Option<String> },
    /// A piece of the answer.
    Text(String),
    /// An `error` event; the turn's `result` follows.
    EventError(String),
    /// The program asks to run a tool. `input` is what it reports of the call.
    Approval {
        request_id: String,
        tool_name: String,
        input: Value,
    },
    /// A control request of a kind the adapter does not answer.
    UnknownRequest { request_id: String },
    /// The turn ended.
    Result {
        success: bool,
        text: String,
        message: String,
        prompt_tokens: Option<usize>,
        completion_tokens: Option<usize>,
    },
    /// Anything else: the vendor adds message types between versions.
    Other,
}

fn count(value: Option<&Value>) -> Option<usize> {
    value.and_then(Value::as_u64).map(|n| n as usize)
}

/// Reads one line of the program's stdout. A line that is not JSON, or has a type the adapter does
/// not know, is [`Line::Other`].
pub(super) fn parse_line(line: &str) -> Line {
    let Ok(message) = serde_json::from_str::<Value>(line) else {
        return Line::Other;
    };
    match message.get("type").and_then(Value::as_str) {
        Some("system") if message.get("subtype").and_then(Value::as_str) == Some("init") => {
            Line::Init {
                permission_mode: message
                    .get("permission_mode")
                    .and_then(Value::as_str)
                    .map(str::to_owned),
            }
        }
        Some("event") => {
            let event = message.get("event");
            match event.and_then(|e| e.get("type")).and_then(Value::as_str) {
                Some("text_delta") if event.and_then(|e| e.get("subtaskId")).is_none() => event
                    .and_then(|e| e.get("content"))
                    .and_then(Value::as_str)
                    .map_or(Line::Other, |text| Line::Text(text.to_owned())),
                Some("error") => Line::EventError(
                    event
                        .and_then(|e| e.get("error"))
                        .and_then(|e| e.get("message"))
                        .and_then(Value::as_str)
                        .unwrap_or("the tool reported an error")
                        .to_owned(),
                ),
                _ => Line::Other,
            }
        }
        Some("control_request") => {
            let Some(request_id) = message.get("request_id").and_then(Value::as_str) else {
                return Line::Other;
            };
            let request = message.get("request");
            if request
                .and_then(|r| r.get("subtype"))
                .and_then(Value::as_str)
                != Some("can_use_tool")
            {
                return Line::UnknownRequest {
                    request_id: request_id.to_owned(),
                };
            }
            // What the program reports of the call. The extraction is tolerant, because the
            // fields are not documented: whatever is missing leaves the name generic and the
            // input the raw request, so the person is still asked and nothing is allowed on its
            // own.
            let call = request.and_then(|r| r.get("request"));
            let tool_name = call
                .and_then(|c| c.get("tool_name").or_else(|| c.get("toolName")))
                .and_then(Value::as_str)
                .unwrap_or("Abacus tool")
                .to_owned();
            let input = call
                .and_then(|c| c.get("tool_input").or_else(|| c.get("toolInput")))
                .or(call)
                .cloned()
                .unwrap_or(Value::Null);
            Line::Approval {
                request_id: request_id.to_owned(),
                tool_name,
                input,
            }
        }
        Some("result") => {
            let success = message.get("is_error").and_then(Value::as_bool) == Some(false)
                && message.get("subtype").and_then(Value::as_str) == Some("success");
            let text = message
                .get("result")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_owned();
            let first_error = message
                .get("errors")
                .and_then(Value::as_array)
                .and_then(|errors| errors.first())
                .and_then(Value::as_str)
                .map(str::to_owned);
            let usage = message.get("token_usage");
            Line::Result {
                success,
                message: first_error.unwrap_or_else(|| text.clone()),
                text,
                prompt_tokens: count(usage.and_then(|u| u.get("inputTokens"))),
                completion_tokens: count(usage.and_then(|u| u.get("outputTokens"))),
            }
        }
        _ => Line::Other,
    }
}

/// The `control_response` that answers the `can_use_tool` request `request_id`.
pub(super) fn approval_answer(request_id: &str, allow: bool) -> Value {
    json!({
        "type": "control_response",
        "response": {
            "subtype": "success",
            "request_id": request_id,
            "response": { "decision": if allow { "accept" } else { "reject" } },
        },
    })
}

/// The `control_response` that declines a request the adapter does not answer.
fn unsupported_answer(request_id: &str) -> Value {
    json!({
        "type": "control_response",
        "response": {
            "subtype": "error",
            "request_id": request_id,
            "error": "not supported by this host",
        },
    })
}

/// The `user` line that starts the turn.
pub(super) fn user_line(prompt: &str) -> Value {
    json!({ "type": "user", "message": { "role": "user", "content": prompt } })
}

/// The reason the program gave for ending with exit 1: the `reason` of the last line of stderr
/// that is `{"type":"fatal","reason":...}`.
pub(super) fn fatal_reason(stderr: &str) -> Option<String> {
    let reason = stderr
        .lines()
        .rev()
        .filter_map(|line| serde_json::from_str::<Value>(line.trim()).ok())
        .find(|value| value.get("type").and_then(Value::as_str) == Some("fatal"))
        .and_then(|value| {
            value
                .get("reason")
                .and_then(Value::as_str)
                .map(str::to_owned)
        })?;
    matches!(
        reason.as_str(),
        "not_authenticated"
            | "access_denied"
            | "network_unreachable"
            | "no_models"
            | "invalid_config"
            | "startup_timeout"
            | "internal"
    )
    .then_some(reason)
}

/// Keeps the end of stderr, where the CLI writes its final fatal record, within the same bound as
/// other delegate stderr readers.
async fn read_stderr_tail<R: AsyncRead + Unpin>(mut reader: R) -> Vec<u8> {
    let mut output = Vec::new();
    let mut buffer = [0_u8; 4096];
    while let Ok(read) = reader.read(&mut buffer).await {
        if read == 0 {
            break;
        }
        if read >= MAX_STDERR_BYTES {
            output.clear();
            output.extend_from_slice(&buffer[read - MAX_STDERR_BYTES..read]);
            continue;
        }
        let overflow = output
            .len()
            .saturating_add(read)
            .saturating_sub(MAX_STDERR_BYTES);
        if overflow > 0 {
            output.drain(..overflow);
        }
        output.extend_from_slice(&buffer[..read]);
    }
    output
}

/// What a program that ended before its first line comes to for the person: a refused credential
/// leads back to connecting; the rest names the reason, never the program's own text.
pub(super) fn startup_error(stderr: &[u8]) -> AdapterError {
    let text = String::from_utf8_lossy(stderr);
    match fatal_reason(&text).as_deref() {
        Some("not_authenticated" | "access_denied") => credential_refused(),
        Some("network_unreachable") => AdapterError::Http {
            reason: "abacusai: the Abacus service cannot be reached".into(),
        },
        Some(reason @ ("no_models" | "invalid_config" | "startup_timeout" | "internal")) => {
            AdapterError::Http {
                reason: format!("abacusai: {reason}"),
            }
        }
        _ => AdapterError::Http {
            reason: "abacusai ended before it answered".into(),
        },
    }
}

async fn write_line(stdin: &mut ChildStdin, value: &Value) -> std::io::Result<()> {
    let mut bytes = serde_json::to_vec(value).map_err(std::io::Error::other)?;
    bytes.push(b'\n');
    stdin.write_all(&bytes).await?;
    stdin.flush().await
}

/// Writes the attachments of the request into the run folder, where the program's own tools find
/// them by the names the prompt mentions.
async fn write_attachments(req: &ChatRequest, run: &RunFolder) -> Result<(), AdapterError> {
    let mut index = 0;
    for message in &req.messages {
        for attachment in &message.attachments {
            let name = format!(
                "attachment-{index}-{}",
                sanitize_file_name(&attachment.name)
            );
            index += 1;
            tokio::fs::write(run.path().join(&name), &attachment.bytes)
                .await
                .map_err(|error| AdapterError::Http {
                    reason: format!(
                        "failed to write attachment {} for abacusai: {error}",
                        attachment.name
                    ),
                })?;
        }
    }
    Ok(())
}

/// The credential of the account for this request, or why there is none.
async fn read_credential(
    context: &DelegateChatContext,
) -> Result<delegate_credential::Credential, AdapterError> {
    let Some(db) = context.database.as_ref() else {
        return Err(not_connected());
    };
    match delegate_credential::read(
        db,
        DelegateVendor::Abacus.as_str(),
        delegate_credential::scope_of(DelegateVendor::Abacus.as_str()),
    )
    .await
    {
        Ok(Read::Ready(credential)) if credential.kind() == KIND_DEVICE_CODE => Ok(credential),
        Ok(Read::Ready(_)) => Err(credential_refused()),
        Ok(Read::Missing) => Err(not_connected()),
        Err(error) => Err(AdapterError::Http {
            reason: format!("the Abacus credential could not be read: {error}"),
        }),
    }
}

/// Starts one isolated Abacus invocation and exposes its output as an adapter stream.
pub(super) async fn spawn_abacus_invocation(
    runtime: DelegateRuntime,
    req: ChatRequest,
    context: DelegateChatContext,
    // The model as a subject of the agent profiles (`<provider id>:<remote id>`).
    model: String,
) -> Result<AdapterStream, AdapterError> {
    spawn_abacus_invocation_with_coverage(
        runtime,
        req,
        context,
        model,
        READ_ONLY_TOOLS_REACH_APPROVAL_BRIDGE,
    )
    .await
}

#[cfg(test)]
pub(super) async fn spawn_abacus_invocation_for_test(
    runtime: DelegateRuntime,
    req: ChatRequest,
    context: DelegateChatContext,
    model: String,
) -> Result<AdapterStream, AdapterError> {
    spawn_abacus_invocation_with_coverage(runtime, req, context, model, true).await
}

async fn spawn_abacus_invocation_with_coverage(
    runtime: DelegateRuntime,
    req: ChatRequest,
    context: DelegateChatContext,
    model: String,
    approval_coverage_verified: bool,
) -> Result<AdapterStream, AdapterError> {
    // Only the approving posture is served (see the module documentation); nothing is started for
    // another one.
    if req.autonomy_mode != AutonomyMode::Standard {
        return Err(AdapterError::Unavailable {
            reason: AutonomyUnavailable {
                vendor: DelegateVendor::Abacus,
                mode: req.autonomy_mode,
            }
            .to_string(),
        });
    }
    if !approval_coverage_verified {
        return Err(approvals_unverified());
    }
    let credential = read_credential(&context).await?;
    let run = runtime.run_folder().map_err(|error| AdapterError::Http {
        reason: format!("failed to create the run folder for abacusai: {error}"),
    })?;
    write_attachments(&req, &run).await?;

    let haex = haex_mcp::server_for(&context, DelegateVendor::Abacus, req.thread_id, &model).await;
    let mcp_config = haex_mcp::write_claude_config(run.path(), None, haex.as_ref()).await?;
    let transcript = build_transcript_prompt(&req);
    let prompt = match haex_mcp::with_instruction(req.system_prompt.as_deref(), haex.is_some()) {
        Some(system) => format!("{system}\n\n{transcript}"),
        None => transcript,
    };

    let mut command = build_command(&runtime, &req.model_id, mcp_config.as_deref(), &run);
    // After containment, on this child alone: the base layer removed every host variable of the
    // vendor first. An API key is never set.
    command.env(DEVICE_CODE_ENV, credential.secret());
    drop(credential);
    configure_process_group(&mut command);
    let mut child = ChildLifecycle::spawn(&mut command, &context.children)
        .map_err(|error| map_spawn_error(DelegateVendor::Abacus, error))?;
    let mut stdin = child
        .child_mut()
        .stdin
        .take()
        .ok_or_else(|| AdapterError::Http {
            reason: "failed to capture abacusai stdin".into(),
        })?;
    let stdout = child
        .child_mut()
        .stdout
        .take()
        .ok_or_else(|| AdapterError::Http {
            reason: "failed to capture abacusai stdout".into(),
        })?;
    let stderr = child
        .child_mut()
        .stderr
        .take()
        .ok_or_else(|| AdapterError::Http {
            reason: "failed to capture abacusai stderr".into(),
        })?;
    let stderr_task = tokio::spawn(read_stderr_tail(stderr));
    let mut lines = BufReader::new(stdout).lines();

    // A program that ends at once (a refused credential, a bad installation) is classified from
    // what it said on stderr, before any stream is returned.
    let first = tokio::time::timeout(STARTUP_PROBE_TIMEOUT, async {
        write_line(&mut stdin, &user_line(&prompt)).await?;
        lines.next_line().await
    })
    .await;
    let first = match first {
        Ok(Ok(Some(line))) => Some(line),
        Ok(Ok(None) | Err(_)) => {
            child.terminate_and_reap().await;
            let stderr = stderr_task.await.unwrap_or_default();
            return Err(startup_error(&stderr));
        }
        Err(_) => {
            child.terminate_and_reap().await;
            stderr_task.abort();
            let _ = stderr_task.await;
            return Err(AdapterError::Http {
                reason: "abacusai did not start in time".into(),
            });
        }
    };

    let (tx, rx) = mpsc::unbounded_channel();
    let cancellation = CancellationToken::new();
    let task_cancellation = cancellation.clone();
    let thread_id = req.thread_id;
    let workspace_root = run.path().to_path_buf();
    let join = tokio::spawn(async move {
        // The run folder and haex-vault's pass end with this start.
        let _run = run;
        let _haex = haex;
        let outcome = drive(
            &mut stdin,
            &mut lines,
            first,
            &tx,
            &task_cancellation,
            &context,
            thread_id,
            &workspace_root,
        )
        .await;
        // The turn is over: closing stdin lets the program end by itself; the process group is
        // ended in any case.
        drop(stdin);
        if matches!(outcome, Outcome::Finished) {
            runtime.confirm_run().await;
        }
        child.terminate_and_reap().await;
        if matches!(outcome, Outcome::Ended) {
            // The program ended without a result: its last words explain why.
            let stderr = stderr_task.await.unwrap_or_default();
            let reason = fatal_reason(&String::from_utf8_lossy(&stderr));
            let _ = tx.send(Err(match reason.as_deref() {
                Some("not_authenticated" | "access_denied") => {
                    StreamError::Model(CREDENTIAL_REFUSED.to_owned())
                }
                Some(reason) => StreamError::Internal(format!("abacusai: {reason}")),
                None => StreamError::UnexpectedEnd,
            }));
        } else {
            stderr_task.abort();
            let _ = stderr_task.await;
        }
    });

    Ok(AdapterStream::new_with_cancellation(
        rx,
        join.abort_handle(),
        cancellation,
    ))
}

/// How the reading of the program's output ended.
#[derive(Debug, PartialEq, Eq)]
enum Outcome {
    /// A `result` line ended the turn normally.
    Finished,
    /// A `result` line, a refusal or a cancel ended the turn and the stream has its last frame.
    Done,
    /// The output ended without a `result`.
    Ended,
}

/// Reads the program's lines, turns them into stream frames and answers its approval requests,
/// until the turn ends.
#[allow(clippy::too_many_arguments)]
async fn drive(
    stdin: &mut ChildStdin,
    lines: &mut Lines<BufReader<ChildStdout>>,
    first: Option<String>,
    tx: &mpsc::UnboundedSender<Result<StreamChunk, StreamError>>,
    cancel: &CancellationToken,
    context: &DelegateChatContext,
    thread_id: Option<uuid::Uuid>,
    workspace_root: &Path,
) -> Outcome {
    let start = Instant::now();
    let mut ttft_ms = None;
    let mut streamed = false;
    let mut supervise_confirmed = false;
    let mut pending = first;
    loop {
        if cancel.is_cancelled() {
            return Outcome::Done;
        }
        let line = match pending.take() {
            Some(line) => line,
            None => {
                let next = tokio::select! {
                    () = cancel.cancelled() => return Outcome::Done,
                    next = lines.next_line() => next,
                };
                match next {
                    Ok(Some(line)) => line,
                    Ok(None) => return Outcome::Ended,
                    Err(error) => {
                        let _ = tx.send(Err(StreamError::Internal(format!(
                            "failed to read abacusai output: {error}"
                        ))));
                        return Outcome::Done;
                    }
                }
            }
        };
        match parse_line(&line) {
            Line::Init { permission_mode } => {
                // The posture rests on this mode; a program that reports another one is not used.
                if permission_mode.as_deref() != Some(PERMISSION_MODE) {
                    let _ = tx.send(Err(StreamError::Model(
                        "abacusai did not start in its approving mode".into(),
                    )));
                    return Outcome::Done;
                }
                supervise_confirmed = true;
            }
            Line::Text(content) => {
                if !content.is_empty() {
                    ttft_ms.get_or_insert_with(|| start.elapsed().as_millis() as u64);
                    streamed = true;
                }
                if tx
                    .send(Ok(StreamChunk::Delta {
                        content,
                        reasoning: None,
                    }))
                    .is_err()
                {
                    return Outcome::Done;
                }
            }
            Line::Approval {
                request_id,
                tool_name,
                input,
            } => {
                // Nothing is allowed on its own: the bridge asks the person or applies the
                // posture, and a closed or failed answer is a rejection. The payload only matters
                // for the autonomy modes that are not served here.
                let payload = ApprovalRequestPayload::ClaudeToolCall {
                    tool_name: tool_name.clone(),
                    input: input.clone(),
                };
                let decision = tokio::select! {
                    () = cancel.cancelled() => return Outcome::Done,
                    decision = approval_bridge::request_approval(
                        &context.pending_tool_approvals,
                        &context.emit,
                        context.database.as_ref(),
                        &context.deny_rules,
                        thread_id,
                        AutonomyMode::Standard,
                        workspace_root,
                        tool_name,
                        input,
                        payload,
                    ) => decision,
                };
                let allow = matches!(decision, crate::chat::tools::ApprovalDecision::Allow);
                let answer = approval_answer(&request_id, allow);
                let write = tokio::select! {
                    () = cancel.cancelled() => return Outcome::Done,
                    write = write_line(stdin, &answer) => write,
                };
                if write.is_err() {
                    return Outcome::Ended;
                }
            }
            Line::UnknownRequest { request_id } => {
                let answer = unsupported_answer(&request_id);
                let write = tokio::select! {
                    () = cancel.cancelled() => return Outcome::Done,
                    write = write_line(stdin, &answer) => write,
                };
                if write.is_err() {
                    return Outcome::Ended;
                }
            }
            Line::Result {
                success,
                text,
                message,
                prompt_tokens,
                completion_tokens,
            } => {
                if !supervise_confirmed {
                    let _ = tx.send(Err(StreamError::Model(
                        "abacusai did not confirm its approving mode".into(),
                    )));
                    return Outcome::Done;
                }
                if !success {
                    let _ = tx.send(Err(StreamError::Model(message)));
                    return Outcome::Done;
                }
                // A turn the program answered without partial events still has its text.
                if !streamed && !text.is_empty() {
                    ttft_ms.get_or_insert_with(|| start.elapsed().as_millis() as u64);
                    let _ = tx.send(Ok(StreamChunk::Delta {
                        content: text,
                        reasoning: None,
                    }));
                }
                let _ = tx.send(Ok(StreamChunk::Done {
                    finish_reason: Some("complete".to_owned()),
                    prompt_tokens,
                    completion_tokens,
                    ttft_ms,
                    total_ms: start.elapsed().as_millis() as u64,
                }));
                return Outcome::Finished;
            }
            Line::EventError(_) | Line::Other => {}
        }
    }
}

#[cfg(test)]
#[path = "abacus_tests.rs"]
mod tests;
