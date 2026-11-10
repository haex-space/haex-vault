//! Maintainability exception (spaex 500-LoC rule): these tests share fixtures
//! and helpers for the autonomy policy matrix, so splitting individual cases
//! would duplicate setup and hide the policy table. If a second autonomy
//! backend is added, extract the shared workspace/fixture helpers first and
//! split backend-specific cases into separate test modules.

use std::path::Path;
use std::sync::OnceLock;

use serde_json::json;
use tempfile::TempDir;

use super::*;

/// A real, existing directory backs every `workspace_root()` call so
/// `is_within_workspace`'s physical (symlink-resolving) containment check
/// has something genuine to canonicalize — a bare string literal like the
/// previous `"/tmp/holzi-invocation-abc123"` never existed on disk, which
/// the old lexical-only check never needed but this one does (code review:
/// symlink-traversal fix). Shared across this module's tests via a
/// `OnceLock`: none of them write real files under it except the dedicated
/// symlink-escape tests below (which use their own unique filenames), so
/// sharing carries no cross-test risk. Intentionally never cleaned up —
/// `TempDir`'s `Drop` never runs for a value parked in a `'static`, the
/// same tradeoff any `/tmp` leftover from a killed test run already has.
fn workspace_root() -> &'static Path {
    static WORKSPACE: OnceLock<TempDir> = OnceLock::new();
    WORKSPACE
        .get_or_init(|| TempDir::new().expect("create temp workspace root for tests"))
        .path()
}

/// Returns the shared test workspace as UTF-8 for JSON approval payloads.
fn workspace_str() -> &'static str {
    workspace_root()
        .to_str()
        .expect("workspace path is valid UTF-8")
}

/// Builds a path beneath the shared test workspace.
fn workspace_path(rel: &str) -> String {
    format!("{}/{rel}", workspace_str())
}

/// Builds a Claude approval payload for deny-rule unit tests.
fn claude_call(tool_name: &str, input: serde_json::Value) -> ApprovalRequestPayload {
    ApprovalRequestPayload::ClaudeToolCall {
        tool_name: tool_name.to_string(),
        input,
    }
}

/// Builds a Codex command approval payload for deny-rule unit tests.
fn codex_command(
    command: &str,
    cwd: Option<&str>,
    network: Option<NetworkContext>,
) -> ApprovalRequestPayload {
    ApprovalRequestPayload::CodexCommandExecution {
        command: command.to_string(),
        cwd: cwd.map(|s| s.to_string()),
        network,
    }
}

#[test]
/// Verifies that an empty deny-rule set permits every request shape.
fn empty_rule_set_always_allows() {
    let request = claude_call("Bash", json!({"command": "curl https://example.com"}));
    assert_eq!(
        evaluate_deny_rules(&[], &request, workspace_root()),
        ApprovalDecision::Allow
    );
    assert_eq!(
        evaluate_deny_rules(
            &[],
            &ApprovalRequestPayload::CodexFileChange,
            workspace_root()
        ),
        ApprovalDecision::Allow
    );
}

// --- WorkspaceEscape ---

#[test]
fn workspace_escape_claude_denies_path_outside_workspace() {
    let request = claude_call("Write", json!({"file_path": "/etc/passwd", "content": "x"}));
    assert_eq!(
        evaluate_deny_rules(&[DenyCategory::WorkspaceEscape], &request, workspace_root()),
        ApprovalDecision::Deny
    );
}

#[test]
fn workspace_escape_claude_allows_path_inside_workspace() {
    let request = claude_call(
        "Write",
        json!({"file_path": workspace_path("notes.txt"), "content": "x"}),
    );
    assert_eq!(
        evaluate_deny_rules(&[DenyCategory::WorkspaceEscape], &request, workspace_root()),
        ApprovalDecision::Allow
    );
}

#[test]
fn workspace_escape_claude_allows_relative_path_that_stays_inside() {
    let request = claude_call("Write", json!({"file_path": "notes.txt"}));
    assert_eq!(
        evaluate_deny_rules(&[DenyCategory::WorkspaceEscape], &request, workspace_root()),
        ApprovalDecision::Allow
    );
}

#[test]
fn workspace_escape_claude_denies_relative_parent_traversal() {
    let request = claude_call("Write", json!({"file_path": "../../etc/passwd"}));
    assert_eq!(
        evaluate_deny_rules(&[DenyCategory::WorkspaceEscape], &request, workspace_root()),
        ApprovalDecision::Deny
    );
}

#[test]
fn workspace_escape_claude_allows_when_no_file_path_present() {
    // Not a file-editing tool call — the category has nothing to evaluate,
    // so it does not apply (data-model.md).
    let request = claude_call("Read", json!({"query": "hi"}));
    assert_eq!(
        evaluate_deny_rules(&[DenyCategory::WorkspaceEscape], &request, workspace_root()),
        ApprovalDecision::Allow
    );
}

#[test]
fn workspace_escape_codex_command_denies_cwd_outside_workspace() {
    let request = codex_command("ls", Some("/etc"), None);
    assert_eq!(
        evaluate_deny_rules(&[DenyCategory::WorkspaceEscape], &request, workspace_root()),
        ApprovalDecision::Deny
    );
}

#[test]
fn workspace_escape_codex_command_allows_cwd_inside_workspace() {
    let request = codex_command("ls", Some(workspace_str()), None);
    assert_eq!(
        evaluate_deny_rules(&[DenyCategory::WorkspaceEscape], &request, workspace_root()),
        ApprovalDecision::Allow
    );
}

#[test]
fn workspace_escape_codex_command_allows_when_no_cwd_present() {
    // Not evaluable — mirrors Claude's "no file_path present" case: the
    // category has nothing to check, so it does not apply (data-model.md).
    // Distinct from `CodexFileChange`, which has no `cwd` field at all on
    // its real schema and fails closed for that reason (FR-015); this is
    // a defensive `Option` never expected to be absent in practice for a
    // `CommandExecutionRequestApprovalParams` payload.
    let request = codex_command("ls", None, None);
    assert_eq!(
        evaluate_deny_rules(&[DenyCategory::WorkspaceEscape], &request, workspace_root()),
        ApprovalDecision::Allow
    );
}

#[test]
fn workspace_escape_codex_file_change_fails_closed() {
    // FR-015: no path field exists on this payload at all.
    assert_eq!(
        evaluate_deny_rules(
            &[DenyCategory::WorkspaceEscape],
            &ApprovalRequestPayload::CodexFileChange,
            workspace_root()
        ),
        ApprovalDecision::Deny
    );
}

#[test]
fn workspace_escape_codex_unevaluable_fails_closed() {
    // Schema drift / an unrecognized approval kind — same reasoning as
    // `CodexFileChange` above.
    assert_eq!(
        evaluate_deny_rules(
            &[DenyCategory::WorkspaceEscape],
            &ApprovalRequestPayload::CodexUnevaluable,
            workspace_root()
        ),
        ApprovalDecision::Deny
    );
}

#[cfg(unix)]
#[test]
fn workspace_escape_claude_denies_symlink_traversal_out_of_workspace() {
    // A symlink placed inside the workspace pointing outside it: lexically
    // `<workspace>/escape-link/passwd` reads as "inside", but the OS
    // resolves it to `/etc/passwd` (code review: physical containment).
    let link_path = workspace_root().join("escape-link-claude");
    std::os::unix::fs::symlink("/etc", &link_path).expect("create test symlink");
    let request = claude_call(
        "Write",
        json!({"file_path": workspace_path("escape-link-claude/passwd"), "content": "x"}),
    );
    assert_eq!(
        evaluate_deny_rules(&[DenyCategory::WorkspaceEscape], &request, workspace_root()),
        ApprovalDecision::Deny
    );
}

#[cfg(unix)]
#[test]
fn workspace_escape_codex_denies_symlink_traversal_out_of_workspace() {
    let link_path = workspace_root().join("escape-link-codex");
    std::os::unix::fs::symlink("/etc", &link_path).expect("create test symlink");
    let request = codex_command("ls", Some(&workspace_path("escape-link-codex")), None);
    assert_eq!(
        evaluate_deny_rules(&[DenyCategory::WorkspaceEscape], &request, workspace_root()),
        ApprovalDecision::Deny
    );
}

#[cfg(unix)]
#[test]
fn workspace_escape_allows_a_symlink_that_stays_inside_the_workspace() {
    // A symlink is not inherently suspicious — only escaping the workspace
    // through one is denied.
    let target = workspace_root().join("real-target-dir");
    std::fs::create_dir(&target).expect("create symlink target dir");
    let link_path = workspace_root().join("inside-link");
    std::os::unix::fs::symlink(&target, &link_path).expect("create test symlink");
    let request = claude_call(
        "Write",
        json!({"file_path": workspace_path("inside-link/notes.txt"), "content": "x"}),
    );
    assert_eq!(
        evaluate_deny_rules(&[DenyCategory::WorkspaceEscape], &request, workspace_root()),
        ApprovalDecision::Allow
    );
}

// --- NetworkAccess ---

#[test]
fn network_access_claude_denies_recognized_webfetch() {
    let request = claude_call("WebFetch", json!({"url": "https://example.com"}));
    assert_eq!(
        evaluate_deny_rules(&[DenyCategory::NetworkAccess], &request, workspace_root()),
        ApprovalDecision::Deny
    );
}

#[test]
fn network_access_claude_denies_unclassifiable_bash_command() {
    // A Bash call whose command text matches no recognized network
    // pattern still fails closed: Claude's approval payload has no
    // structured network field, so an unclassifiable command cannot be
    // positively ruled out (data-model.md). Distinct from T019's
    // evaluator-error mapping — this is the evaluator's own documented
    // behavior for a well-formed but ambiguous request.
    let request = claude_call("Bash", json!({"command": "ls -la /tmp"}));
    assert_eq!(
        evaluate_deny_rules(&[DenyCategory::NetworkAccess], &request, workspace_root()),
        ApprovalDecision::Deny
    );
}

#[test]
fn network_access_claude_allows_non_command_non_network_tool() {
    let request = claude_call("Read", json!({"file_path": workspace_path("a.txt")}));
    assert_eq!(
        evaluate_deny_rules(&[DenyCategory::NetworkAccess], &request, workspace_root()),
        ApprovalDecision::Allow
    );
}

#[test]
fn network_access_codex_command_denies_structured_network_context() {
    let request = codex_command(
        "curl https://example.com",
        Some(workspace_str()),
        Some(NetworkContext {
            host: Some("example.com".to_string()),
            protocol: Some("https".to_string()),
        }),
    );
    assert_eq!(
        evaluate_deny_rules(&[DenyCategory::NetworkAccess], &request, workspace_root()),
        ApprovalDecision::Deny
    );
}

#[test]
fn network_access_codex_command_allows_without_network_context() {
    let request = codex_command("ls", Some(workspace_str()), None);
    assert_eq!(
        evaluate_deny_rules(&[DenyCategory::NetworkAccess], &request, workspace_root()),
        ApprovalDecision::Allow
    );
}

#[test]
fn network_access_codex_file_change_does_not_apply() {
    // Not a network-triggered approval kind — unlike WorkspaceEscape/
    // CredentialPaths, this category simply does not apply to a file
    // change at all (data-model.md), so it allows rather than fails closed.
    assert_eq!(
        evaluate_deny_rules(
            &[DenyCategory::NetworkAccess],
            &ApprovalRequestPayload::CodexFileChange,
            workspace_root()
        ),
        ApprovalDecision::Allow
    );
}

#[test]
fn network_access_codex_unevaluable_fails_closed() {
    // Unlike `CodexFileChange`, this shape carries no guarantee the
    // underlying action isn't network-triggered.
    assert_eq!(
        evaluate_deny_rules(
            &[DenyCategory::NetworkAccess],
            &ApprovalRequestPayload::CodexUnevaluable,
            workspace_root()
        ),
        ApprovalDecision::Deny
    );
}

// --- CredentialPaths ---

#[test]
fn credential_paths_claude_denies_ssh_file_path() {
    let request = claude_call("Read", json!({"file_path": "/home/user/.ssh/id_rsa"}));
    assert_eq!(
        evaluate_deny_rules(&[DenyCategory::CredentialPaths], &request, workspace_root()),
        ApprovalDecision::Deny
    );
}

#[test]
fn credential_paths_denies_a_windows_style_backslash_and_uppercase_path() {
    // Patterns are lowercase, forward-slash Unix paths (code review): a
    // differently-cased, backslash-separated path must still match.
    let request = claude_call(
        "Read",
        json!({"file_path": r"C:\Users\me\.AWS\Credentials"}),
    );
    assert_eq!(
        evaluate_deny_rules(&[DenyCategory::CredentialPaths], &request, workspace_root()),
        ApprovalDecision::Deny
    );
}

#[test]
fn credential_paths_claude_denies_command_touching_env_file() {
    let request = claude_call("Bash", json!({"command": "cat .env"}));
    assert_eq!(
        evaluate_deny_rules(&[DenyCategory::CredentialPaths], &request, workspace_root()),
        ApprovalDecision::Deny
    );
}

#[test]
fn credential_paths_claude_allows_unrelated_file() {
    let request = claude_call("Write", json!({"file_path": workspace_path("report.md")}));
    assert_eq!(
        evaluate_deny_rules(&[DenyCategory::CredentialPaths], &request, workspace_root()),
        ApprovalDecision::Allow
    );
}

#[test]
fn credential_paths_codex_command_denies_aws_pattern() {
    let request = codex_command("cat ~/.aws/credentials", Some(workspace_str()), None);
    assert_eq!(
        evaluate_deny_rules(&[DenyCategory::CredentialPaths], &request, workspace_root()),
        ApprovalDecision::Deny
    );
}

#[test]
fn credential_paths_codex_command_allows_unrelated_command() {
    let request = codex_command("ls", Some(workspace_str()), None);
    assert_eq!(
        evaluate_deny_rules(&[DenyCategory::CredentialPaths], &request, workspace_root()),
        ApprovalDecision::Allow
    );
}

#[test]
fn credential_paths_codex_file_change_fails_closed() {
    assert_eq!(
        evaluate_deny_rules(
            &[DenyCategory::CredentialPaths],
            &ApprovalRequestPayload::CodexFileChange,
            workspace_root()
        ),
        ApprovalDecision::Deny
    );
}

#[test]
fn credential_paths_codex_unevaluable_fails_closed() {
    assert_eq!(
        evaluate_deny_rules(
            &[DenyCategory::CredentialPaths],
            &ApprovalRequestPayload::CodexUnevaluable,
            workspace_root()
        ),
        ApprovalDecision::Deny
    );
}

#[test]
fn multiple_enabled_categories_deny_if_any_matches() {
    let request = claude_call("Read", json!({"file_path": "/home/user/.ssh/id_rsa"}));
    assert_eq!(
        evaluate_deny_rules(
            &[DenyCategory::WorkspaceEscape, DenyCategory::CredentialPaths],
            &request,
            workspace_root()
        ),
        ApprovalDecision::Deny
    );
}

// --- classify_autonomy_spawn_error ---

#[test]
fn classifies_claude_unsupported_permission_mode() {
    let raw = "error: option '--permission-mode <mode>' argument 'bypassPermissions' is invalid.";
    let result = classify_autonomy_spawn_error(DelegateVendor::Claude, AutonomyMode::Ungated, raw);
    assert_eq!(
        result,
        Some(AutonomyUnavailable {
            vendor: DelegateVendor::Claude,
            mode: AutonomyMode::Ungated,
        })
    );
}

#[test]
fn does_not_classify_unrelated_claude_error() {
    let raw = "claude exited without producing a result: network error";
    assert_eq!(
        classify_autonomy_spawn_error(DelegateVendor::Claude, AutonomyMode::Ungated, raw),
        None
    );
}

#[test]
fn classifies_codex_unsupported_approval_policy() {
    let raw =
        r#"codex thread/start failed: {"code":-32602,"message":"unknown field `approvalPolicy`"}"#;
    let result =
        classify_autonomy_spawn_error(DelegateVendor::Codex, AutonomyMode::GatedPermissive, raw);
    assert_eq!(
        result,
        Some(AutonomyUnavailable {
            vendor: DelegateVendor::Codex,
            mode: AutonomyMode::GatedPermissive,
        })
    );
}

#[test]
fn does_not_classify_unrelated_codex_error() {
    let raw = "codex app-server exited before responding to thread/start";
    assert_eq!(
        classify_autonomy_spawn_error(DelegateVendor::Codex, AutonomyMode::Ungated, raw),
        None
    );
}

#[test]
fn does_not_classify_a_sandbox_init_failure_as_unavailable() {
    // Mentions "sandbox" but is not a rejection of the field itself (code
    // review) — a transient/environmental failure must not be misreported
    // as "this autonomy mode is unsupported".
    let raw = "codex thread/start failed: sandbox initialization failed: permission denied";
    assert_eq!(
        classify_autonomy_spawn_error(DelegateVendor::Codex, AutonomyMode::GatedPermissive, raw),
        None
    );
}
