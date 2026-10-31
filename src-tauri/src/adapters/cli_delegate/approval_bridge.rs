use std::io;
use std::path::{Path, PathBuf};

#[cfg(unix)]
use serde::Deserialize;
use serde_json::{json, Value};
#[cfg(unix)]
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::sync::oneshot;
use uuid::Uuid;

use crate::chat::tools::permission::{self, PermissionMode};
use crate::chat::tools::ApprovalDecision;
use crate::storage::chat_messages::{self as msg_store, ChatMessage, MessageRole};
use crate::vault_gate::VaultDb;

use super::autonomy::{self, ApprovalRequestPayload, AutonomyMode};
use super::{DelegateChatContext, EventEmitter, PendingToolApprovals};

/// Fixed, non-localized markers for a `gated-permissive` tool-call audit
/// row's `tool_result` content — the frontend translates them, the same
/// i18n-boundary convention `chat/turn/tool_round.rs` already uses for
/// `blocked_by_plan_mode`/`tool_call_cancelled` (CONTEXT.md).
const MARKER_DENIED_BY_DENY_RULE: &str = "denied_by_deny_rule";
const MARKER_PERMITTED: &str = "gated_permissive_call_permitted";
/// Unlike the two markers above, this fills the call row's `tool_input` —
/// a slot the frontend renders verbatim as data, not through a translated
/// label, so it is plain, already-legible text rather than an i18n key.
/// `ChatMessage`'s own validation requires a `ToolCall` row to carry *some*
/// `tool_input` (`storage::chat_messages::validate`), so this stands in for
/// the raw call input the audit trail deliberately no longer persists
/// (code review: it can carry command arguments, file contents, or
/// credentials).
const REDACTED_TOOL_INPUT: &str = "[redacted]";

#[cfg(unix)]
#[derive(Debug, Deserialize)]
struct ApprovalRequest {
    tool_name: String,
    input: Value,
}

/// Applies holzi's existing permission posture (`Standard`) or, under
/// `GatedPermissive`, the operator's persisted deny rules
/// (spec 009-autonomous-delegate-mode) instead. `Ungated` never reaches
/// this function in practice — Claude spawns no bridge for it and Codex's
/// `approvalPolicy: "never"` means Codex itself never emits the callback.
#[allow(clippy::too_many_arguments)]
pub async fn request_approval(
    pending: &PendingToolApprovals,
    emit: &EventEmitter,
    database: Option<&VaultDb>,
    thread_id: Option<Uuid>,
    autonomy_mode: AutonomyMode,
    workspace_root: &Path,
    tool_name: String,
    input: Value,
    payload: ApprovalRequestPayload,
) -> ApprovalDecision {
    if autonomy_mode == AutonomyMode::GatedPermissive {
        return gated_permissive_decision(
            database,
            workspace_root,
            thread_id,
            &tool_name,
            &payload,
        )
        .await;
    }

    let mode = match database {
        Some(database) => permission::read_from_vault(database).await,
        None => PermissionMode::Manual,
    };
    let risk = if is_risky_tool(&tool_name) {
        crate::chat::tools::RiskClass::Risky
    } else {
        crate::chat::tools::RiskClass::Safe
    };
    match permission::decide(mode, risk) {
        permission::Decision::Allow => ApprovalDecision::Allow,
        permission::Decision::Deny => ApprovalDecision::Deny,
        permission::Decision::Ask => {
            let request_id = Uuid::new_v4();
            let (sender, receiver) = oneshot::channel();
            pending
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner())
                .insert(request_id, sender);
            emit(
                "tool-permission-request",
                json!({
                    "requestId": request_id,
                    "threadId": thread_id.unwrap_or_else(Uuid::nil),
                    "toolName": tool_name,
                    "toolInput": input,
                    "riskClass": crate::chat::events::risk_class_str(risk),
                    "toolSource": "cli",
                }),
            );
            receiver.await.unwrap_or(ApprovalDecision::Deny)
        }
    }
}

/// The `GatedPermissive` decision path (spec FR-004/FR-007): reads the
/// persisted deny rules and evaluates them against `payload`, never
/// populating `pending_tool_approvals` or emitting `tool-permission-request`
/// — no human turn in this loop by design. Fails closed to `Deny` (FR-010)
/// on any read/parse problem, independently of the `Ask`-branch's own
/// `receiver.await.unwrap_or(Deny)` safety net above, which this
/// synchronous path does not go through. Also persists an audit record of
/// the call and its decision (spec FR-005/US2) — `gated-permissive` is the
/// only mode with a per-tool-call record at all (`Ungated` has none,
/// FR-006; `Standard`'s own tool activity, when it has any, is recorded by
/// the built-in tool loop instead).
async fn gated_permissive_decision(
    database: Option<&VaultDb>,
    workspace_root: &Path,
    thread_id: Option<Uuid>,
    tool_name: &str,
    payload: &ApprovalRequestPayload,
) -> ApprovalDecision {
    let Some(database) = database else {
        return ApprovalDecision::Deny;
    };
    let rules = database
        .read(|r| Ok(autonomy::get_deny_rules(r)))
        .await
        .ok()
        .and_then(|result| result.ok());

    let decision = match rules {
        Some(rules) => autonomy::evaluate_deny_rules(&rules, payload, workspace_root),
        None => ApprovalDecision::Deny,
    };

    let persisted =
        persist_gated_permissive_record(database, thread_id, tool_name, payload, decision).await;

    // The audit record is the only proof this decision was ever made (spec
    // FR-005/US2). If it could not be committed, treat the call as denied
    // rather than let an unaudited `Allow` through.
    if persisted {
        decision
    } else {
        ApprovalDecision::Deny
    }
}

/// Persists one `tool_call`/`tool_result` row pair recording a
/// `gated-permissive` approval callback and holzi's decision on it — the
/// only outcome holzi itself observes at this boundary; the delegate
/// executes the tool in its own process, so no actual tool output is
/// available to record (unlike the built-in tool loop's own rows). Both
/// rows commit in one transaction — a partial pair (a call row with no
/// matching result) would misrepresent the audit trail just as badly as
/// losing it entirely. Returns whether the transaction committed; the
/// caller denies the call outright when it did not (spec FR-005/US2: an
/// unaudited `Allow` is not an acceptable outcome).
async fn persist_gated_permissive_record(
    database: &VaultDb,
    thread_id: Option<Uuid>,
    tool_name: &str,
    payload: &ApprovalRequestPayload,
    decision: ApprovalDecision,
) -> bool {
    let Some(thread_id) = thread_id else {
        return false;
    };
    let tool_source = match payload {
        ApprovalRequestPayload::ClaudeToolCall { .. } => "cli_delegate:claude",
        ApprovalRequestPayload::CodexCommandExecution { .. }
        | ApprovalRequestPayload::CodexFileChange
        | ApprovalRequestPayload::CodexUnevaluable => "cli_delegate:codex",
    };
    let call_id = Uuid::new_v4().to_string();
    // Distinct, increasing timestamps so the call row reliably sorts before
    // its result under `list_messages`'s `created_at ASC, id ASC` ordering
    // (the same reason `tool_round.rs::persist_round` bumps its own clock
    // between the two).
    let now = now_ms();
    let call_row = ChatMessage {
        id: Uuid::new_v4(),
        thread_id,
        // No parent row is available at this call boundary (unlike the
        // built-in tool loop's `TurnRunner`, which walks its own chain) —
        // display order is driven entirely by `created_at`/`id`
        // (`list_messages`), so this is a bookkeeping gap only.
        parent_id: None,
        role: MessageRole::ToolCall,
        content: String::new(),
        provider_id: None,
        model_id: None,
        prompt_tokens: None,
        completion_tokens: None,
        finish_reason: None,
        created_at: now,
        idempotency_key: None,
        tool_name: Some(tool_name.to_string()),
        tool_call_id: Some(call_id.clone()),
        // Never the raw call input: it can carry command arguments, file
        // contents, or credential-bearing text (code review). The decision
        // was made against the typed, already-vetted `ApprovalRequestPayload`,
        // not this row, so the audit trail needs the outcome, not the input.
        tool_input: Some(REDACTED_TOOL_INPUT.to_string()),
        tool_is_error: None,
        tool_source: Some(tool_source.to_string()),
        autonomy_mode: Some(AutonomyMode::GatedPermissive.as_str().to_string()),
        tool_origin: None,
    };
    let result_row = ChatMessage {
        id: Uuid::new_v4(),
        thread_id,
        parent_id: None,
        role: MessageRole::ToolResult,
        content: match decision {
            ApprovalDecision::Allow => MARKER_PERMITTED.to_string(),
            ApprovalDecision::Deny => MARKER_DENIED_BY_DENY_RULE.to_string(),
        },
        provider_id: None,
        model_id: None,
        prompt_tokens: None,
        completion_tokens: None,
        finish_reason: None,
        created_at: now + 1,
        idempotency_key: None,
        tool_name: None,
        tool_call_id: Some(call_id),
        tool_input: None,
        tool_is_error: Some(matches!(decision, ApprovalDecision::Deny)),
        tool_source: None,
        autonomy_mode: Some(AutonomyMode::GatedPermissive.as_str().to_string()),
        tool_origin: None,
    };

    database
        .write(move |tx| {
            msg_store::insert_message(tx, &call_row)?;
            msg_store::insert_message(tx, &result_row)
        })
        .await
        .is_ok()
}

/// Returns the current Unix timestamp in milliseconds for ordered audit rows.
fn now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

fn is_risky_tool(name: &str) -> bool {
    let name = name.to_ascii_lowercase();
    !matches!(
        name.as_str(),
        "read" | "glob" | "grep" | "ls" | "list" | "search" | "cat"
    )
}

#[cfg(unix)]
pub fn bind_socket(path: &std::path::Path) -> io::Result<tokio::net::UnixListener> {
    std::fs::remove_file(path).ok();
    tokio::net::UnixListener::bind(path)
}

#[cfg(unix)]
pub fn start_listener(
    listener: tokio::net::UnixListener,
    context: DelegateChatContext,
    thread_id: Option<Uuid>,
    autonomy_mode: AutonomyMode,
    workspace_root: PathBuf,
) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        loop {
            let Ok((stream, _)) = listener.accept().await else {
                break;
            };
            let context = context.clone();
            let workspace_root = workspace_root.clone();
            tokio::spawn(async move {
                let (read_half, mut write_half) = stream.into_split();
                let mut lines = BufReader::new(read_half).lines();
                while let Ok(Some(line)) = lines.next_line().await {
                    let Ok(request) = serde_json::from_str::<ApprovalRequest>(&line) else {
                        break;
                    };
                    let payload = ApprovalRequestPayload::ClaudeToolCall {
                        tool_name: request.tool_name.clone(),
                        input: request.input.clone(),
                    };
                    let decision = request_approval(
                        &context.pending_tool_approvals,
                        &context.emit,
                        context.database.as_ref(),
                        thread_id,
                        autonomy_mode,
                        &workspace_root,
                        request.tool_name,
                        request.input,
                        payload,
                    )
                    .await;
                    let value = json!({
                        "decision": match decision {
                            ApprovalDecision::Allow => "allow",
                            ApprovalDecision::Deny => "deny",
                        }
                    });
                    let Ok(mut bytes) = serde_json::to_vec(&value) else {
                        break;
                    };
                    bytes.push(b'\n');
                    if write_half.write_all(&bytes).await.is_err() {
                        break;
                    }
                }
            });
        }
    })
}

#[cfg(not(unix))]
pub fn bind_socket(_path: &std::path::Path) -> io::Result<()> {
    Err(io::Error::new(
        io::ErrorKind::Unsupported,
        "CLI delegate approval sockets are not implemented on this target",
    ))
}

#[cfg(not(unix))]
pub fn start_listener(
    _listener: (),
    _context: DelegateChatContext,
    _thread_id: Option<Uuid>,
    _autonomy_mode: AutonomyMode,
    _workspace_root: PathBuf,
) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async {})
}
