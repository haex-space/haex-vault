//! Unit tests for `codex.rs`'s JSON-RPC line classifier and request
//! builder (tasks.md T012). Event shapes are the real ones captured
//! live in research.md §2, not invented fixtures.

use serde_json::json;

use super::autonomy::ApprovalRequestPayload;
use super::codex::{
    Line, build_codex_payload, build_command, build_request, categorize_line,
    turn_completed_failure,
};
use crate::cli_tools::runtime::DelegateRuntime;

/// Spec 094 FR-003, FR-011: the vault's own program by its path, the vendor's switches before the
/// subcommand, and the child contained to the vault.
#[test]
fn the_command_runs_the_installed_program_with_the_vendors_switches_in_the_vault() {
    let dir = tempfile::TempDir::new().unwrap();
    let runtime = DelegateRuntime::for_stub("codex", dir.path().join("stub-codex"), dir.path());
    let run = runtime.run_folder().unwrap();
    let cmd = build_command(&runtime, &run);
    let std_cmd = cmd.as_std();

    assert_eq!(std_cmd.get_program(), dir.path().join("stub-codex"));
    let args: Vec<String> = std_cmd
        .get_args()
        .map(|a| a.to_string_lossy().into_owned())
        .collect();
    let subcommand = args.iter().position(|a| a == "app-server").unwrap();
    assert_eq!(args[subcommand + 1], "--stdio");
    // No update check and no plugin catalog clone (which would need the host's `git`).
    assert_eq!(&args[..2], ["-c", "check_for_update_on_startup=false"]);
    assert!(
        args[..subcommand]
            .windows(2)
            .any(|w| w == ["--disable", "plugins"])
    );
    assert_eq!(std_cmd.get_current_dir(), Some(run.path()));
    let codex_home = std_cmd
        .get_envs()
        .find(|(k, _)| *k == "CODEX_HOME")
        .and_then(|(_, v)| v);
    assert_eq!(codex_home, Some(run.path().as_os_str()));
}

/// Codex reads the answer to `mcpServer/elicitation/request` by its `action`; the `decision` of
/// the command and file approvals is no answer to it (schema of codex 0.169.0).
#[test]
fn a_question_of_an_mcp_server_is_declined_in_its_own_shape() {
    let answer = super::codex::elicitation_decline();
    assert_eq!(answer, json!({"action": "decline"}));
    assert!(answer.get("decision").is_none());
}

#[test]
fn build_request_matches_the_client_request_envelope() {
    let value = build_request(3, "turn/start", json!({"threadId": "abc"}));
    assert_eq!(value["jsonrpc"], "2.0");
    assert_eq!(value["id"], 3);
    assert_eq!(value["method"], "turn/start");
    assert_eq!(value["params"]["threadId"], "abc");
}

#[test]
fn a_response_to_our_own_request_is_recognized() {
    let line = r#"{"id": 2, "result": {"thread": {"id": "01a0"}}}"#;
    match categorize_line(Some(line)) {
        Line::Response { id, value } => {
            assert_eq!(id, 2);
            assert_eq!(value["result"]["thread"]["id"], "01a0");
        }
        other => panic!("expected Response, got {other:?}"),
    }
}

#[test]
fn a_live_approval_request_is_recognized_as_a_server_request() {
    // Verbatim shape from the research.md §2 live spike.
    let line = r#"{"method": "item/commandExecution/requestApproval", "id": 0, "params": {"threadId": "t", "turnId": "u", "itemId": "exec-1", "startedAtMs": 1, "command": "touch x"}}"#;
    match categorize_line(Some(line)) {
        Line::ServerRequest { id, method, .. } => {
            assert_eq!(id, json!(0));
            assert_eq!(method, "item/commandExecution/requestApproval");
        }
        other => panic!("expected ServerRequest, got {other:?}"),
    }
}

#[test]
fn a_notification_has_no_id() {
    let line = r#"{"method": "turn/completed", "params": {"threadId": "t"}}"#;
    match categorize_line(Some(line)) {
        Line::Notification { method, .. } => assert_eq!(method, "turn/completed"),
        other => panic!("expected Notification, got {other:?}"),
    }
}

#[test]
fn turn_completed_with_status_failed_reports_the_error_message() {
    // Verbatim shape observed live from a genuine auth failure (garbage
    // credential in an isolated CODEX_HOME, research.md §6) -- codex has
    // no separate `turn/failed` notification for this case, only
    // `turn.status` inside `turn/completed` distinguishes it.
    let params = json!({
        "threadId": "t",
        "turn": {
            "id": "u",
            "status": "failed",
            "error": {"message": "unexpected status 401 Unauthorized: Missing bearer or basic authentication in header"}
        }
    });
    assert_eq!(
        turn_completed_failure(&params).as_deref(),
        Some(
            "unexpected status 401 Unauthorized: Missing bearer or basic authentication in header"
        )
    );
}

#[test]
fn turn_completed_with_status_completed_reports_no_failure() {
    let params = json!({
        "threadId": "t",
        "turn": {"id": "u", "status": "completed", "error": null, "items": []}
    });
    assert_eq!(turn_completed_failure(&params), None);
}

#[test]
fn turn_completed_failed_without_an_error_message_still_reports_a_failure() {
    let params = json!({"turn": {"id": "u", "status": "failed"}});
    assert_eq!(
        turn_completed_failure(&params).as_deref(),
        Some("codex turn failed")
    );
}

#[test]
fn eof_and_blank_and_malformed_lines_do_not_panic() {
    assert!(matches!(categorize_line(None), Line::Eof));
    assert!(matches!(categorize_line(Some("")), Line::Unrecognized));
    assert!(matches!(
        categorize_line(Some("not json")),
        Line::Unrecognized
    ));
}

// --- build_codex_payload fail-closed cases (code review) ---

#[test]
fn command_execution_payload_carries_command_and_cwd() {
    let params = json!({"command": "ls -la", "cwd": "/workspace"});
    match build_codex_payload("item/commandExecution/requestApproval", &params) {
        ApprovalRequestPayload::CodexCommandExecution { command, cwd, .. } => {
            assert_eq!(command, "ls -la");
            assert_eq!(cwd.as_deref(), Some("/workspace"));
        }
        other => panic!("expected CodexCommandExecution, got {other:?}"),
    }
}

#[test]
fn file_change_payload_has_no_fields() {
    assert_eq!(
        build_codex_payload("item/fileChange/requestApproval", &json!({})),
        ApprovalRequestPayload::CodexFileChange
    );
}

#[test]
fn command_execution_missing_command_is_unevaluable_not_empty() {
    let params = json!({"cwd": "/workspace"});
    assert_eq!(
        build_codex_payload("item/commandExecution/requestApproval", &params),
        ApprovalRequestPayload::CodexUnevaluable
    );
}

#[test]
fn command_execution_missing_cwd_is_unevaluable_not_empty() {
    let params = json!({"command": "ls -la"});
    assert_eq!(
        build_codex_payload("item/commandExecution/requestApproval", &params),
        ApprovalRequestPayload::CodexUnevaluable
    );
}

#[test]
fn command_execution_null_cwd_is_unevaluable_not_none() {
    // A `cwd` key that is present but not a string (`null`, a number, an
    // object) must not fall through to `cwd: None` — `WorkspaceEscape`
    // treats `None` as "nothing to check" (allow) rather than "unevaluable"
    // (deny), so this would otherwise bypass the category (code review).
    let params = json!({"command": "ls -la", "cwd": null});
    assert_eq!(
        build_codex_payload("item/commandExecution/requestApproval", &params),
        ApprovalRequestPayload::CodexUnevaluable
    );
}

#[test]
fn unrecognized_method_is_unevaluable() {
    assert_eq!(
        build_codex_payload("item/permissions/requestApproval", &json!({})),
        ApprovalRequestPayload::CodexUnevaluable
    );
}
