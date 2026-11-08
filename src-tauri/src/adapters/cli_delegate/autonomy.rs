//! Autonomy Mode (spec 009-autonomous-delegate-mode): `AutonomyMode`,
//! `DenyCategory`, the per-vendor `evaluate_deny_rules` matcher, the
//! `cli_delegate.deny_rules` vault preference (it was a device preference before
//! spec 023, FR-024), and the reactive
//! unsupported-mode error classifier. See
//! `specs/009-autonomous-delegate-mode/data-model.md` for the full design.

use std::path::{Component, Path, PathBuf};

use serde::{Deserialize, Serialize};
use serde_json::Value;
use ts_rs::TS;

use crate::chat::tools::ApprovalDecision;

use super::DelegateVendor;

/// Per-request posture for a `cli_delegate` invocation (spec FR-001/FR-002). Carried on
/// `ChatRequest`. As of spec 083 PR J3 it is a right of the agent profile of the model
/// (`Rights::delegate`), which the chat reads when it sends; `#[default]` is `Standard`, the
/// posture of a profile without the group (every call asks).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export, export_to = "../../src/types/bindings/")]
pub enum AutonomyMode {
    #[default]
    Standard,
    Ungated,
    GatedPermissive,
}

impl DenyCategory {
    /// The label stored in a profile's row, `None` for one this build does not know.
    pub fn parse(label: &str) -> Option<Self> {
        match label {
            "workspace_escape" => Some(Self::WorkspaceEscape),
            "network_access" => Some(Self::NetworkAccess),
            "credential_paths" => Some(Self::CredentialPaths),
            _ => None,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::WorkspaceEscape => "workspace_escape",
            Self::NetworkAccess => "network_access",
            Self::CredentialPaths => "credential_paths",
        }
    }
}

impl AutonomyMode {
    /// The posture stored in a profile's row, `None` for a label this build does not know.
    pub fn parse(label: &str) -> Option<Self> {
        match label {
            "standard" => Some(Self::Standard),
            "ungated" => Some(Self::Ungated),
            "gated_permissive" => Some(Self::GatedPermissive),
            _ => None,
        }
    }

    /// Returns the stable wire/storage label for this autonomy posture.
    pub fn as_str(self) -> &'static str {
        match self {
            AutonomyMode::Standard => "standard",
            AutonomyMode::Ungated => "ungated",
            AutonomyMode::GatedPermissive => "gated_permissive",
        }
    }
}

/// One operator-configured, persisted (spec FR-014) deny category enforced
/// under `GatedPermissive`. Fixed, small set (research.md §4) — no
/// general-purpose rule engine.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export, export_to = "../../src/types/bindings/")]
pub enum DenyCategory {
    WorkspaceEscape,
    NetworkAccess,
    CredentialPaths,
}

/// Host/protocol context Codex's `networkApprovalContext` carries on a
/// `CommandExecutionRequestApprovalParams` when the call is network-
/// triggered. Only its presence (`Some`/`None`) drives matching today
/// (data-model.md); the fields exist so the caller can log/display them.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NetworkContext {
    pub host: Option<String>,
    pub protocol: Option<String>,
}

/// What a vendor's approval callback actually provides — deliberately not a
/// lowest-common-denominator struct, so the real per-vendor asymmetry
/// (data-model.md) is visible in the type rather than hidden behind
/// optional fields. Identifies the vendor itself, so no separate
/// `DelegateVendor` parameter is needed alongside it.
#[derive(Debug, Clone, PartialEq)]
pub enum ApprovalRequestPayload {
    /// `input` carries `file_path` directly when the tool is a file-editing
    /// tool (`Write`/`Edit`/...).
    ClaudeToolCall { tool_name: String, input: Value },
    CodexCommandExecution {
        command: String,
        cwd: Option<String>,
        network: Option<NetworkContext>,
    },
    /// Deliberately field-less: `FileChangeRequestApprovalParams` exposes
    /// no path (research.md §3) — not a placeholder to fill in later.
    CodexFileChange,
    /// A Codex approval callback whose method holzi does not recognize, or
    /// whose `item/commandExecution/requestApproval` params are missing a
    /// field research.md §2 says the real schema always carries
    /// (`command`/`cwd`) — schema drift, not a legitimate empty command.
    /// Treated like `CodexFileChange`: no reliable signal for any deny
    /// category, so every category fails closed (FR-015) rather than
    /// evaluating against a guessed-empty payload that could bypass every
    /// enabled rule.
    CodexUnevaluable,
}

/// Fixed glob-like substrings for the `CredentialPaths` category
/// (research.md §4's named examples — not an exhaustive list by design).
const CREDENTIAL_PATH_PATTERNS: &[&str] = &[".ssh/", ".aws/", ".env", "id_rsa"];

/// Reports whether a supplied path or command mentions a protected credential path.
fn matches_credential_pattern(value: Option<&str>) -> bool {
    value
        .map(|v| {
            // Patterns are lowercase, forward-slash Unix paths; normalize
            // the candidate the same way so a Windows-style path
            // (`C:\Users\me\.aws\credentials`) or a differently-cased
            // component doesn't bypass the check (code review).
            let normalized = v.replace('\\', "/").to_ascii_lowercase();
            CREDENTIAL_PATH_PATTERNS
                .iter()
                .any(|pattern| normalized.contains(pattern))
        })
        .unwrap_or(false)
}

/// Any command-execution-shaped or explicitly network-named Claude tool
/// call is denied outright while `NetworkAccess` is enabled: Claude's
/// approval payload carries no structured network signal, so even a
/// command that doesn't obviously touch the network cannot be positively
/// ruled out — data-model.md's documented fail-closed default for this
/// category (recognized cases like `curl`/`wget` are denied for the same
/// reason, not a different one).
fn claude_network_denied(tool_name: &str) -> bool {
    matches!(
        tool_name.to_ascii_lowercase().as_str(),
        "webfetch" | "websearch" | "bash"
    )
}

/// Resolves a lexically (no filesystem access — the path may not exist
/// yet) normalized form of `path`, joined onto `workspace_root` first when
/// relative.
fn resolve_lexically(path: &str, workspace_root: &Path) -> PathBuf {
    let path = Path::new(path);
    let joined = if path.is_absolute() {
        path.to_path_buf()
    } else {
        workspace_root.join(path)
    };
    let mut normalized = PathBuf::new();
    for component in joined.components() {
        match component {
            Component::ParentDir => {
                normalized.pop();
            }
            Component::CurDir => {}
            other => normalized.push(other.as_os_str()),
        }
    }
    normalized
}

/// Resolves `path` (via [`resolve_lexically`]) and then collapses any
/// symlink in its *existing* prefix by walking up to the nearest ancestor
/// that is actually on disk, canonicalizing that, and re-appending the
/// remaining not-yet-created components lexically — a `Write`/`Edit`
/// target frequently does not exist yet, so a plain `canonicalize()` on
/// the full path would just fail. Falls back to the lexical form only if
/// no ancestor at all can be canonicalized (e.g. a bogus root); the
/// caller's containment check still runs against that fallback, it just
/// can't benefit from symlink resolution in that degenerate case.
///
/// This closes the gap a lexical-only check has: a symlink placed inside
/// the workspace pointing outside it (`workspace_root/link -> /etc`) would
/// otherwise let `workspace_root/link/passwd` pass as "inside" even though
/// the real, OS-resolved target is `/etc/passwd` (code review).
fn resolve_physically(path: &str, workspace_root: &Path) -> PathBuf {
    let lexical = resolve_lexically(path, workspace_root);
    let mut existing: &Path = &lexical;
    let mut trailing: Vec<&std::ffi::OsStr> = Vec::new();
    loop {
        if let Ok(mut resolved) = existing.canonicalize() {
            trailing.reverse();
            resolved.extend(trailing);
            return resolved;
        }
        match (existing.file_name(), existing.parent()) {
            (Some(name), Some(parent)) => {
                trailing.push(name);
                existing = parent;
            }
            _ => return lexical,
        }
    }
}

/// Checks physical containment within an existing, canonical workspace root.
fn is_within_workspace(candidate: &str, workspace_root: &Path) -> bool {
    let Ok(canonical_root) = workspace_root.canonicalize() else {
        return false;
    };
    resolve_physically(candidate, workspace_root).starts_with(canonical_root)
}

/// Evaluates one enabled deny category against a normalized vendor request.
fn category_denies(
    category: DenyCategory,
    request: &ApprovalRequestPayload,
    workspace_root: &Path,
) -> bool {
    match (category, request) {
        (DenyCategory::WorkspaceEscape, ApprovalRequestPayload::ClaudeToolCall { input, .. }) => {
            input
                .get("file_path")
                .and_then(Value::as_str)
                .map(|path| !is_within_workspace(path, workspace_root))
                .unwrap_or(false)
        }
        (
            DenyCategory::WorkspaceEscape,
            ApprovalRequestPayload::CodexCommandExecution { cwd, .. },
        ) => cwd
            .as_deref()
            .map(|cwd| !is_within_workspace(cwd, workspace_root))
            .unwrap_or(false),
        // FR-015: no path field on this payload at all — cannot evaluate,
        // fail closed rather than allow an unverifiable call through.
        (DenyCategory::WorkspaceEscape, ApprovalRequestPayload::CodexFileChange) => true,
        // Same fail-closed reasoning as `CodexFileChange`: schema drift or
        // an unrecognized approval kind leaves nothing reliable to check.
        (DenyCategory::WorkspaceEscape, ApprovalRequestPayload::CodexUnevaluable) => true,

        (DenyCategory::CredentialPaths, ApprovalRequestPayload::ClaudeToolCall { input, .. }) => {
            matches_credential_pattern(input.get("file_path").and_then(Value::as_str))
                || matches_credential_pattern(input.get("command").and_then(Value::as_str))
        }
        (
            DenyCategory::CredentialPaths,
            ApprovalRequestPayload::CodexCommandExecution { command, .. },
        ) => matches_credential_pattern(Some(command.as_str())),
        // FR-015: same unevaluable-payload gap as WorkspaceEscape.
        (DenyCategory::CredentialPaths, ApprovalRequestPayload::CodexFileChange) => true,
        (DenyCategory::CredentialPaths, ApprovalRequestPayload::CodexUnevaluable) => true,

        (DenyCategory::NetworkAccess, ApprovalRequestPayload::ClaudeToolCall { tool_name, .. }) => {
            claude_network_denied(tool_name)
        }
        (
            DenyCategory::NetworkAccess,
            ApprovalRequestPayload::CodexCommandExecution { network, .. },
        ) => network.is_some(),
        // Not a network-triggered approval kind; category does not apply.
        (DenyCategory::NetworkAccess, ApprovalRequestPayload::CodexFileChange) => false,
        // Unlike `CodexFileChange`, this payload shape carries no guarantee
        // the underlying action isn't network-triggered — FR-015 fail
        // closed applies here too.
        (DenyCategory::NetworkAccess, ApprovalRequestPayload::CodexUnevaluable) => true,
    }
}

/// The `GatedPermissive` approval-callback evaluator (data-model.md).
/// Returns `Deny` if any enabled category matches (including the FR-015
/// unevaluable-but-enabled case), `Allow` otherwise — an empty `enabled`
/// slice always allows (spec's documented edge case: no deny rules
/// configured means fully permissive).
///
/// This evaluates only approval callbacks the vendor actually emits; a
/// tool call for which no callback fires never reaches this function
/// (data-model.md).
pub fn evaluate_deny_rules(
    enabled: &[DenyCategory],
    request: &ApprovalRequestPayload,
    workspace_root: &Path,
) -> ApprovalDecision {
    if enabled
        .iter()
        .any(|category| category_denies(*category, request, workspace_root))
    {
        ApprovalDecision::Deny
    } else {
        ApprovalDecision::Allow
    }
}

/// A requested `Ungated`/`GatedPermissive` invocation could not be started
/// because the installed CLI does not support the native mechanism this
/// mode needs (spec FR-013/SC-006) — not a generic spawn failure.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AutonomyUnavailable {
    pub vendor: DelegateVendor,
    pub mode: AutonomyMode,
}

impl std::fmt::Display for AutonomyUnavailable {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "autonomy mode unavailable: vendor={}; mode={}",
            self.vendor.as_str(),
            self.mode.as_str()
        )
    }
}

impl std::error::Error for AutonomyUnavailable {}

/// Recognizes a vendor's error text as "this CLI version does not support
/// the requested autonomy mode" rather than a generic failure. Reactive
/// (classify an actual failure) rather than a proactive CLI-version probe
/// (research.md §2/§ — matches how 007 already surfaces "backend
/// unavailable" for its own other failure classes, spec FR-008
/// precedent).
pub fn classify_autonomy_spawn_error(
    vendor: DelegateVendor,
    mode: AutonomyMode,
    raw_error: &str,
) -> Option<AutonomyUnavailable> {
    let recognized = match vendor {
        // An old `claude` rejects an unknown `--permission-mode` enum
        // value on its own argument parser, e.g.:
        // "error: option '--permission-mode <mode>' argument
        // 'bypassPermissions' is invalid."
        DelegateVendor::Claude => {
            raw_error.contains("--permission-mode") && raw_error.contains("invalid")
        }
        // An old `codex app-server` rejects an unrecognized `thread/start`
        // field with a JSON-RPC "invalid params" error mentioning the field
        // name, e.g. `{"code":-32602,"message":"unknown field
        // \"approvalPolicy\""}`. Requiring both the field name and an
        // invalid/unknown-field indication (rather than the field name
        // alone) keeps an unrelated failure — e.g. "sandbox initialization
        // failed: permission denied" — classified as the transient error it
        // actually is, instead of a false "mode unavailable" (code review).
        DelegateVendor::Codex => {
            let names_field = raw_error.contains("approvalPolicy") || raw_error.contains("sandbox");
            let rejected_as_unknown = raw_error.contains("unknown field")
                || raw_error.contains("invalid params")
                || raw_error.contains("-32602");
            names_field && rejected_as_unknown
        }
    };
    recognized.then_some(AutonomyUnavailable { vendor, mode })
}

#[cfg(test)]
#[path = "autonomy_tests.rs"]
mod tests;
