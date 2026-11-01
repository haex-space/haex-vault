//! `claude -p` process driver (tasks.md T013, T015). Drives one
//! `stream_chat` call to completion as a subprocess and translates its
//! `--output-format stream-json` events into [`StreamChunk`]s.
//!
//! The command uses an isolated configuration directory and routes Claude's
//! live permission-prompt tool through the shared holzi approval bridge.
//!
//! Deliberately over the repo's 500-LoC soft threshold: parsing the
//! stream-json line protocol, building the `claude` invocation, and
//! classifying its exit failures are one cohesive "drive this subprocess
//! call" responsibility with a single change reason (the `claude -p`
//! contract), and splitting them across files would scatter one thing
//! readers need to see together. When the parser gains another event family,
//! extract the stream-json parser into `claude_protocol.rs` and leave process
//! setup/shutdown in this module.

use std::path::Path;
use std::process::Stdio;
use std::time::{Duration, Instant};

use tempfile::TempDir;
use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader};
use tokio::process::Command;
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;

use crate::adapters::types::{StreamChunk, StreamError};
use crate::adapters::{AdapterError, AdapterStream, ChatRequest};

use super::approval_bridge;
use super::autonomy::{self, AutonomyMode};
use super::holzi_mcp;
use super::process::{configure_process_group, map_spawn_error, ChildLifecycle};
use super::subagents;
use super::{build_transcript_prompt, DelegateChatContext, DelegateVendor};

/// Caps how much of a `claude` stderr we keep around for an error
/// message — mirrors `chat/tools/cli.rs`'s own output cap, just applied
/// to diagnostics rather than command output.
const MAX_STDERR_BYTES: usize = 64 * 1024;

/// Bounds the `Ungated` "first line vs. early exit" startup probe (see its
/// call site below) — mirrors `codex.rs`'s `HANDSHAKE_TIMEOUT` for the
/// analogous wait on Codex's own startup handshake.
const STARTUP_PROBE_TIMEOUT: Duration = Duration::from_secs(30);

/// What one `stream-json` line means for the stream. `pub(super)` so
/// `claude_tests.rs` can exercise it directly without spawning a real
/// process (tasks.md T011).
pub(super) enum LineOutcome {
    /// Nothing worth surfacing (a system/status event, a tool-related
    /// event this feature doesn't model, an empty line).
    Ignore,
    Chunk(StreamChunk),
    /// Either a malformed/unparseable line (a well-formed `claude` never
    /// emits one, so seeing one means something genuinely went wrong —
    /// a truncated write, a version mismatch) or a `result` line
    /// reporting `is_error: true`.
    Error(StreamError),
}

/// Every `tool_use`-typed content block's `id` in a stream-json
/// `message.content` array (research.md §2 shape).
fn tool_use_ids(content: &serde_json::Value) -> Vec<String> {
    content
        .as_array()
        .into_iter()
        .flatten()
        .filter(|block| block.get("type").and_then(|t| t.as_str()) == Some("tool_use"))
        .filter_map(|block| block.get("id").and_then(|id| id.as_str()))
        .map(str::to_string)
        .collect()
}

/// Every `tool_result`-typed content block's `tool_use_id` in a
/// stream-json `message.content` array (research.md §2 shape).
fn tool_result_ids(content: &serde_json::Value) -> Vec<String> {
    content
        .as_array()
        .into_iter()
        .flatten()
        .filter(|block| block.get("type").and_then(|t| t.as_str()) == Some("tool_result"))
        .filter_map(|block| block.get("tool_use_id").and_then(|id| id.as_str()))
        .map(str::to_string)
        .collect()
}

/// Pure NDJSON-line parser — no I/O, so it's directly unit-testable
/// (tasks.md T011) against the real event shapes captured in
/// research.md §1. `ttft_ms` is `Some` only the first time a `Delta` is
/// produced (time-to-first-token), computed by the caller from `start`.
/// `tracker` accumulates sub-agent activity across the whole invocation
/// (research.md §2, spec 011-composer-toolbar-parity) — owned by the
/// caller, threaded through one line at a time so this function stays a
/// pure, directly unit-testable parser (tasks.md T011's own rationale).
pub(super) fn parse_line(
    line: &str,
    ttft_ms: Option<u64>,
    total_ms: u64,
    tracker: &mut subagents::Tracker,
) -> LineOutcome {
    if line.trim().is_empty() {
        return LineOutcome::Ignore;
    }
    let value: serde_json::Value = match serde_json::from_str(line) {
        Ok(v) => v,
        Err(e) => {
            return LineOutcome::Error(StreamError::Internal(format!("invalid JSON line: {e}")))
        }
    };
    match value.get("type").and_then(|v| v.as_str()) {
        Some("assistant") | Some("user") => {
            let message_type = value.get("type").and_then(|v| v.as_str());
            let parent_tool_use_id = value.get("parent_tool_use_id").and_then(|v| v.as_str());
            let content = value
                .pointer("/message/content")
                .cloned()
                .unwrap_or_default();
            let event = if let Some(parent_id) = parent_tool_use_id {
                // Any message belonging to a sub-agent — including its very
                // first ("prompt") message — confirms that its parent
                // tool_use id spawned one (research.md §2).
                let event = tracker.observe_parent_reference(parent_id);
                if message_type == Some("assistant") {
                    // Sub-agents can dispatch further agents. Register those
                    // ids before their first child message references them,
                    // while preserving the promotion event for this line.
                    tracker.observe_top_level_tool_use(&tool_use_ids(&content));
                }
                event
            } else if message_type == Some("assistant") {
                tracker.observe_top_level_tool_use(&tool_use_ids(&content));
                subagents::TrackerEvent::None
            } else {
                // Rare (multiple sub-agents in the same batch finishing on
                // the same stream-json line): keep the last real update —
                // still the correct, current active count either way.
                let mut event = subagents::TrackerEvent::None;
                for id in tool_result_ids(&content) {
                    let this_event = tracker.observe_tool_result(&id);
                    if !matches!(this_event, subagents::TrackerEvent::None) {
                        event = this_event;
                    }
                }
                event
            };
            match event {
                subagents::TrackerEvent::Update {
                    active_count,
                    batch_size,
                } => LineOutcome::Chunk(StreamChunk::AgentActivity {
                    active_count,
                    batch_size,
                }),
                subagents::TrackerEvent::None => LineOutcome::Ignore,
            }
        }
        Some("stream_event") => {
            let Some(delta) = value
                .get("event")
                .filter(|e| e.get("type").and_then(|t| t.as_str()) == Some("content_block_delta"))
                .and_then(|e| e.get("delta"))
            else {
                return LineOutcome::Ignore;
            };
            let delta_type = delta.get("type").and_then(|t| t.as_str());
            let text = delta.get("text").and_then(|t| t.as_str());
            match (delta_type, text) {
                (Some("text_delta"), Some(text)) => LineOutcome::Chunk(StreamChunk::Delta {
                    content: text.to_string(),
                    reasoning: None,
                }),
                (Some("thinking_delta"), Some(text)) => LineOutcome::Chunk(StreamChunk::Delta {
                    content: String::new(),
                    reasoning: Some(text.to_string()),
                }),
                _ => LineOutcome::Ignore,
            }
        }
        Some("result") => {
            let is_error = value
                .get("is_error")
                .and_then(|v| v.as_bool())
                .unwrap_or(false);
            if is_error {
                let message = value
                    .get("result")
                    .and_then(|v| v.as_str())
                    .unwrap_or("claude reported an error")
                    .to_string();
                return LineOutcome::Error(StreamError::Model(message));
            }
            let usage = value.get("usage");
            let prompt_tokens = usage
                .and_then(|u| u.get("input_tokens"))
                .and_then(|v| v.as_u64())
                .map(|v| v as usize);
            let completion_tokens = usage
                .and_then(|u| u.get("output_tokens"))
                .and_then(|v| v.as_u64())
                .map(|v| v as usize);
            LineOutcome::Chunk(StreamChunk::Done {
                finish_reason: Some("complete".to_string()),
                prompt_tokens,
                completion_tokens,
                ttft_ms,
                total_ms,
            })
        }
        _ => LineOutcome::Ignore,
    }
}

/// The `--mcp-config` file and whether it carries holzi's own server.
pub(super) struct McpArgs<'a> {
    pub config: &'a Path,
    pub holzi: bool,
}

pub(super) fn build_command(
    binary: &str,
    model: &str,
    mcp: Option<McpArgs<'_>>,
    system_prompt_path: Option<&Path>,
    tmp: &TempDir,
    autonomy_mode: AutonomyMode,
    reasoning_option: Option<&str>,
) -> Command {
    let mut cmd = Command::new(binary);
    cmd.arg("-p")
        .arg("--output-format")
        .arg("stream-json")
        .arg("--verbose")
        .arg("--include-partial-messages");
    // `req.model_id` is a real Anthropic model id from `fetch_claude_models`
    // (mod.rs) — `claude --help` documents `--model` as accepting either a
    // short alias or "a model's full name", and the id the models API
    // returns satisfies the latter. An empty id (e.g. a session loaded
    // before this feature existed) falls back to the CLI's own default
    // rather than passing `--model ""`. The vendor discriminator
    // (`DelegateVendor::Claude.as_str()`) is the same fallback: it's the
    // synthetic single-model id `list_models` returned for Claude before
    // this change, still persisted as a thread's `last_model_id` for
    // anyone who connected before the real model list existed, and isn't
    // a real alias `claude` itself accepts.
    if !model.is_empty() && model != DelegateVendor::Claude.as_str() {
        cmd.arg("--model").arg(model);
    }
    // Real Claude Code CLI flag, passed through as the option id
    // `send_message` already validated against this model's cached options.
    if let Some(level) = reasoning_option {
        cmd.arg("--effort").arg(level);
    }
    if autonomy_mode == AutonomyMode::Ungated {
        // Native full-autonomy mechanism (research.md §2): no bridge is
        // spawned for this mode at all, so there is no approval tool to
        // wire up either. `Standard`/`GatedPermissive` keep today's exact
        // command — only the approval *decision function* changes for
        // `GatedPermissive` (approval_bridge.rs), not the invocation.
        cmd.arg("--permission-mode").arg("bypassPermissions");
    } else {
        cmd.arg("--permission-mode")
            .arg("default")
            .arg("--permission-prompt-tool")
            .arg("mcp__holzi-approve__approve");
    }
    // The approval bridge outside `Ungated`, and holzi's own server for a registered delegate in
    // every mode (spec 021 R7).
    if let Some(mcp) = mcp {
        cmd.arg("--mcp-config").arg(mcp.config);
        // holzi asks for its own tools by the delegate's approval mode; Claude must not ask again.
        if mcp.holzi {
            cmd.arg("--allowedTools").arg(holzi_mcp::CLAUDE_HOLZI_TOOLS);
        }
    }
    if let Some(system_prompt_path) = system_prompt_path {
        cmd.arg("--append-system-prompt-file")
            .arg(system_prompt_path);
    }
    cmd.env("CLAUDE_CONFIG_DIR", tmp.path())
        .current_dir(tmp.path())
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    cmd
}

/// Terminates `child`, drains `stderr_task`, aborts `listener_task` (if
/// any), and classifies the collected stderr against `autonomy_mode`
/// (tasks.md T009/T022): a recognized unsupported-mode rejection becomes
/// `AdapterError::Unavailable`; otherwise `fallback` (with the stderr text
/// appended, if any) becomes the reason of a generic `AdapterError::Http`.
/// Called on every startup failure path below regardless of mode — a
/// spawned child and its background tasks must not be left running just
/// because the failure happened before the `Ungated`-only peek-ahead
/// (code review). The unsupported-mode classification itself still only
/// ever matches in practice for `Ungated`: `Standard`/`GatedPermissive`
/// send `--permission-mode default`, a value every `claude` accepts, so
/// their failures always fall through to the generic `Http` case.
async fn classify_exit_error(
    child: &mut ChildLifecycle,
    stderr_task: tokio::task::JoinHandle<Vec<u8>>,
    listener_task: Option<tokio::task::JoinHandle<()>>,
    autonomy_mode: AutonomyMode,
    fallback: &str,
) -> AdapterError {
    child.terminate_and_reap().await;
    let stderr_bytes = stderr_task.await.unwrap_or_default();
    if let Some(task) = listener_task {
        task.abort();
        let _ = task.await;
    }
    let stderr_text = String::from_utf8_lossy(&stderr_bytes).trim().to_string();
    if let Some(unavailable) =
        autonomy::classify_autonomy_spawn_error(DelegateVendor::Claude, autonomy_mode, &stderr_text)
    {
        return AdapterError::Unavailable {
            reason: unavailable.to_string(),
        };
    }
    let reason = if stderr_text.is_empty() {
        fallback.to_string()
    } else {
        format!("{fallback}: {stderr_text}")
    };
    AdapterError::Http { reason }
}

/// Starts one isolated Claude Code invocation and exposes its output as an adapter stream.
pub(super) async fn spawn_claude_invocation(
    binary: String,
    credentials: Vec<u8>,
    req: ChatRequest,
    context: DelegateChatContext,
) -> Result<AdapterStream, AdapterError> {
    let token = super::decode_claude_oauth_token(&credentials)?;

    // Disposable per-invocation CLAUDE_CONFIG_DIR + cwd (spec 007-cli-delegate
    // FR-002/FR-003) — removed when `tmp` drops at the end of the spawned
    // task below, success or failure alike.
    let tmp = TempDir::new().map_err(|e| AdapterError::Http {
        reason: format!("failed to create temp dir for claude invocation: {e}"),
    })?;

    let prompt = build_transcript_prompt(&req);
    // Attachment bytes go into the same disposable working directory the
    // process already runs in — Claude Code's own Read tool can then find
    // them by the filename `build_transcript_prompt` just mentioned
    // (research.md §3), with no dedicated CLI attachment flag needed.
    let mut attachment_index = 0;
    for message in &req.messages {
        for attachment in &message.attachments {
            let sandbox_name = format!("attachment-{attachment_index}-{}", attachment.name);
            attachment_index += 1;
            tokio::fs::write(tmp.path().join(&sandbox_name), &attachment.bytes)
                .await
                .map_err(|error| AdapterError::Http {
                    reason: format!(
                        "failed to write attachment {} for claude invocation: {error}",
                        attachment.name
                    ),
                })?;
        }
    }
    let system_prompt_path = req.system_prompt.as_deref().map(|system_prompt| {
        let path = tmp.path().join("system-prompt.txt");
        (path, system_prompt)
    });
    if let Some((path, system_prompt)) = &system_prompt_path {
        tokio::fs::write(path, system_prompt)
            .await
            .map_err(|error| AdapterError::Http {
                reason: format!("failed to write Claude system prompt: {error}"),
            })?;
    }
    let autonomy_mode = req.autonomy_mode;
    // `Ungated` never needs a bridge process at all — Claude is told to
    // bypass permissions entirely, so there is no approval tool call to
    // relay (tasks.md T021). holzi's own server comes in every mode.
    let holzi = holzi_mcp::server_for(&context, DelegateVendor::Claude, req.thread_id).await;
    let (approve_socket, listener_task) = if autonomy_mode == AutonomyMode::Ungated {
        (None, None)
    } else {
        let socket_path = tmp.path().join("approval.sock");
        // Without Unix sockets the bind fails, and its success type is `()`.
        #[cfg_attr(not(unix), allow(clippy::let_unit_value))]
        let listener =
            approval_bridge::bind_socket(&socket_path).map_err(|error| AdapterError::Http {
                reason: format!("failed to create Claude approval socket: {error}"),
            })?;
        let listener_task = approval_bridge::start_listener(
            listener,
            context.clone(),
            req.thread_id,
            autonomy_mode,
            tmp.path().to_path_buf(),
        );
        (Some(socket_path), Some(listener_task))
    };
    let mcp_config_path =
        holzi_mcp::write_claude_config(tmp.path(), approve_socket.as_deref(), holzi.as_ref())
            .await?;
    let mut cmd = build_command(
        &binary,
        &req.model_id,
        mcp_config_path.as_deref().map(|config| McpArgs {
            config,
            holzi: holzi.is_some(),
        }),
        system_prompt_path.as_ref().map(|(path, _)| path.as_path()),
        &tmp,
        autonomy_mode,
        req.reasoning_option.as_deref(),
    );
    configure_process_group(&mut cmd);
    cmd.env("CLAUDE_CODE_OAUTH_TOKEN", &token);

    let mut child = ChildLifecycle::spawn(&mut cmd, &context.children)
        .map_err(|e| map_spawn_error(&binary, DelegateVendor::Claude, e))?;

    let mut stdin = child
        .child_mut()
        .stdin
        .take()
        .ok_or_else(|| AdapterError::Http {
            reason: "failed to capture claude stdin".into(),
        })?;
    let stdout = child
        .child_mut()
        .stdout
        .take()
        .ok_or_else(|| AdapterError::Http {
            reason: "failed to capture claude stdout".into(),
        })?;
    let mut stderr = child
        .child_mut()
        .stderr
        .take()
        .ok_or_else(|| AdapterError::Http {
            reason: "failed to capture claude stderr".into(),
        })?;

    // Spawned before writing the prompt: an installed `claude` too old to
    // accept `bypassPermissions` can reject it and exit immediately, on its
    // own argument parser, before ever reading stdin — which surfaces as a
    // broken-pipe *write* failure below, not a read/EOF one. Both early-exit
    // shapes route through `classify_exit_error` so either is classified
    // the same way (tasks.md T009/T022, spec FR-013/SC-006).
    let stderr_task = tokio::spawn(async move {
        let mut buf = Vec::new();
        let mut chunk = [0_u8; 4096];
        loop {
            match stderr.read(&mut chunk).await {
                Ok(0) | Err(_) => break,
                Ok(n) => {
                    let remaining = MAX_STDERR_BYTES.saturating_sub(buf.len());
                    if remaining > 0 {
                        buf.extend_from_slice(&chunk[..n.min(remaining)]);
                    }
                }
            }
        }
        buf
    });

    if let Err(error) = stdin.write_all(prompt.as_bytes()).await {
        return Err(classify_exit_error(
            &mut child,
            stderr_task,
            listener_task,
            autonomy_mode,
            &format!("failed to write Claude prompt: {error}"),
        )
        .await);
    }
    if let Err(error) = stdin.shutdown().await {
        return Err(classify_exit_error(
            &mut child,
            stderr_task,
            listener_task,
            autonomy_mode,
            &format!("failed to close Claude prompt input: {error}"),
        )
        .await);
    }
    // `shutdown()` alone does not release the underlying pipe fd — only
    // `Drop` does. The success path used to return (and thus drop `stdin`)
    // almost immediately after this point, so the child always saw EOF
    // quickly; the `Ungated` peek-ahead below now keeps this function
    // running longer, so `stdin` must be dropped explicitly or a child that
    // actually reads its prompt from stdin (any real `claude`, and any
    // stub written to behave like one) deadlocks waiting for EOF.
    drop(stdin);

    let mut reader = BufReader::new(stdout).lines();

    // Reactive classification (spec FR-013/SC-006, tasks.md T009/T022): a
    // slower-to-reject `claude` may accept the prompt write but still exit
    // before emitting any `stream-json` line. Unlike Codex's synchronous
    // `thread/start`, Claude has no separate handshake call to fail — the
    // only way to detect this before committing to the stream is to race
    // "first line" against "process already exited" once, here, before
    // returning. `GatedPermissive` sends the exact same flags as `Standard`
    // (only the approval bridge's decision function differs), so it can
    // never hit this path and is not worth the same peek — scoped to
    // `Ungated` only.
    let mut peeked_line: Option<String> = None;
    if autonomy_mode == AutonomyMode::Ungated {
        match tokio::time::timeout(STARTUP_PROBE_TIMEOUT, reader.next_line()).await {
            Ok(Ok(Some(line))) => peeked_line = Some(line),
            Ok(Ok(None)) => {
                return Err(classify_exit_error(
                    &mut child,
                    stderr_task,
                    listener_task,
                    autonomy_mode,
                    "claude exited without producing a result",
                )
                .await);
            }
            Ok(Err(e)) => {
                return Err(AdapterError::Http {
                    reason: format!("failed to read claude stdout: {e}"),
                })
            }
            // A silent `claude` that neither emits output nor exits would
            // otherwise hang this probe indefinitely — cancellation aside
            // (the caller can still drop this future), nothing else here
            // bounds the wait (code review).
            Err(_) => {
                return Err(classify_exit_error(
                    &mut child,
                    stderr_task,
                    listener_task,
                    autonomy_mode,
                    &format!(
                        "claude produced no output within {STARTUP_PROBE_TIMEOUT:?} of starting"
                    ),
                )
                .await);
            }
        }
    }

    let (tx, rx) = mpsc::unbounded_channel();
    let cancellation = CancellationToken::new();
    let task_cancellation = cancellation.clone();
    let join = tokio::spawn(async move {
        // Keep `tmp` alive for the whole process lifetime; it is removed
        // when this task ends, on every path below.
        let _tmp = tmp;
        // holzi's pass for this start ends with it.
        let _holzi = holzi;

        let start = Instant::now();
        let mut ttft_ms = None;
        let mut saw_result = false;
        let mut peeked_line = peeked_line;
        let mut agent_tracker = subagents::Tracker::new();

        loop {
            // The line already consumed by the peek-ahead above (if any)
            // must be processed first, before reading any further.
            let line = if let Some(line) = peeked_line.take() {
                Ok(Some(line))
            } else {
                tokio::select! {
                    _ = task_cancellation.cancelled() => {
                        child.terminate_and_reap().await;
                        stderr_task.abort();
                        let _ = stderr_task.await;
                        if let Some(task) = listener_task {
                            task.abort();
                            let _ = task.await;
                        }
                        return;
                    }
                    line = reader.next_line() => line,
                }
            };
            match line {
                Ok(Some(line)) => {
                    match parse_line(
                        &line,
                        ttft_ms,
                        start.elapsed().as_millis() as u64,
                        &mut agent_tracker,
                    ) {
                        LineOutcome::Ignore => {}
                        LineOutcome::Chunk(chunk) => {
                            if matches!(chunk, StreamChunk::Delta { .. }) && ttft_ms.is_none() {
                                ttft_ms = Some(start.elapsed().as_millis() as u64);
                            }
                            let is_done = matches!(chunk, StreamChunk::Done { .. });
                            if tx.send(Ok(chunk)).is_err() {
                                // Receiver dropped (AdapterStream discarded) —
                                // stop, the caller no longer wants output.
                                child.terminate_and_reap().await;
                                stderr_task.abort();
                                let _ = stderr_task.await;
                                if let Some(task) = listener_task {
                                    task.abort();
                                    let _ = task.await;
                                }
                                return;
                            }
                            if is_done {
                                saw_result = true;
                                break;
                            }
                        }
                        LineOutcome::Error(err) => {
                            saw_result = true;
                            let _ = tx.send(Err(err));
                            break;
                        }
                    }
                }
                Ok(None) => break,
                Err(e) => {
                    let _ = tx.send(Err(StreamError::Internal(format!(
                        "failed to read claude stdout: {e}"
                    ))));
                    break;
                }
            }
        }

        // A successful result is the end of this one-shot invocation, even
        // if a CLI keeps its server loop alive after emitting it.
        child.terminate_and_reap().await;
        let stderr_bytes = stderr_task.await.unwrap_or_default();
        if let Some(task) = listener_task {
            task.abort();
            let _ = task.await;
        }

        if !saw_result {
            let stderr_text = String::from_utf8_lossy(&stderr_bytes).trim().to_string();
            let reason = if stderr_text.is_empty() {
                "claude exited without producing a result".to_string()
            } else {
                format!("claude exited without producing a result: {stderr_text}")
            };
            let _ = tx.send(Err(StreamError::Internal(reason)));
        }
    });

    Ok(AdapterStream::new_with_cancellation(
        rx,
        join.abort_handle(),
        cancellation,
    ))
}
