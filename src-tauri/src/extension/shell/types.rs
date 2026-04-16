use serde::{Deserialize, Serialize};
use ts_rs::TS;

/// Event name for shell output streaming
pub const SHELL_OUTPUT_EVENT: &str = "shell:output";

/// Shell session creation options
#[derive(Debug, Clone, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct ShellCreateOptions {
    /// Shell executable (e.g., "/bin/bash", "/bin/zsh"). If None, uses $SHELL or /bin/sh.
    pub shell: Option<String>,
    /// Working directory. If None, uses home directory.
    pub cwd: Option<String>,
    /// Initial terminal columns
    pub cols: Option<u16>,
    /// Initial terminal rows
    pub rows: Option<u16>,
    /// Environment variables to set
    pub env: Option<std::collections::HashMap<String, String>>,
}

/// Shell output event payload, emitted via Tauri events.
///
/// `extension_id` identifies the owning extension so the broadcast layer can
/// route the event only to iframes of that extension — stdout must not leak
/// to other (potentially untrusted) extensions sharing the same origin.
#[derive(Debug, Clone, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct ShellOutputEvent {
    pub session_id: String,
    pub extension_id: String,
    pub data: String,
}

/// Shell exit event payload. Includes `extension_id` for the same reason as
/// `ShellOutputEvent` — owner-scoped routing.
#[derive(Debug, Clone, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct ShellExitEvent {
    pub session_id: String,
    pub extension_id: String,
    pub exit_code: Option<i32>,
}

/// Available shell info returned by list_available
#[derive(Debug, Clone, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct ShellInfo {
    pub name: String,
    pub path: String,
}

/// Response from shell create command
#[derive(Debug, Clone, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct ShellCreateResponse {
    pub session_id: String,
    /// The resolved shell name (e.g., "bash", "zsh", "fish")
    pub shell_name: String,
}
