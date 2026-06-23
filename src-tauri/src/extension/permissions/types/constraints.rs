use serde::{Deserialize, Serialize};
use ts_rs::TS;

use super::permission::ResourceType;

// --- Constraint-Typen (unverändert) ---

#[derive(Serialize, Deserialize, Clone, Debug, TS)]
#[serde(untagged)]
#[ts(export)]
pub enum PermissionConstraints {
    Database(DbConstraints),
    Filesystem(FsConstraints),
    Web(WebConstraints),
    Shell(ShellConstraints),
}

#[derive(Serialize, Deserialize, Clone, Debug, Default, TS)]
#[ts(export)]
pub struct DbConstraints {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub where_clause: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub columns: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub limit: Option<u32>,
}

#[derive(Serialize, Deserialize, Clone, Debug, Default, TS)]
#[ts(export)]
pub struct FsConstraints {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_file_size: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub allowed_extensions: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub recursive: Option<bool>,
}

#[derive(Serialize, Deserialize, Clone, Debug, Default, TS)]
#[ts(export)]
pub struct WebConstraints {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub methods: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rate_limit: Option<RateLimit>,
}

#[derive(Serialize, Deserialize, Clone, Debug, TS)]
#[ts(export)]
pub struct RateLimit {
    pub requests: u32,
    pub per_minutes: u32,
}

#[derive(Serialize, Deserialize, Clone, Debug, Default, TS)]
#[ts(export)]
pub struct ShellConstraints {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub allowed_subcommands: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub allowed_flags: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub forbidden_args: Option<Vec<String>>,
}

/// Splits a constraints **Value** into the `(typed, raw)` pair used by
/// `ExtensionPermission`.
///
/// This is the single place that encodes the passwords-vs-other invariant:
/// `passwords` rows mark their *default label* via a free-form
/// `{"default":true}` constraint that the typed (untagged)
/// [`PermissionConstraints`] enum can't represent, so they are kept *raw*
/// (`constraints = None`, `raw_constraints = Some`). Every other resource type
/// parses into the typed enum (`constraints = Some`, `raw_constraints = None`).
///
/// Used by the manifest path, whose input is already a `serde_json::Value`.
pub(crate) fn split_constraints_value(
    resource_type: ResourceType,
    value: Option<&serde_json::Value>,
) -> (Option<PermissionConstraints>, Option<serde_json::Value>) {
    if resource_type == ResourceType::Passwords {
        (None, value.cloned())
    } else {
        let typed = value.and_then(|v| serde_json::from_value(v.clone()).ok());
        (typed, None)
    }
}

/// Splits a constraints **text** column (DB `constraints`) into the
/// `(typed, raw)` pair used by `ExtensionPermission`.
///
/// Same passwords-vs-other invariant as [`split_constraints_value`], but the
/// input is the raw DB text (the READ/DB-text direction): for `passwords` the
/// text is parsed into a free-form `Value` and kept raw; every other resource
/// type parses the text into the typed enum.
pub(crate) fn split_constraints(
    resource_type: ResourceType,
    raw_text: Option<&str>,
) -> (Option<PermissionConstraints>, Option<serde_json::Value>) {
    if resource_type == ResourceType::Passwords {
        let raw = raw_text.and_then(|s| serde_json::from_str::<serde_json::Value>(s).ok());
        (None, raw)
    } else {
        let typed = raw_text.and_then(|s| serde_json::from_str(s).ok());
        (typed, None)
    }
}

/// Combines the `(typed, raw)` constraints pair back into the DB `constraints`
/// text column (the WRITE direction).
///
/// Prefers the raw, free-form constraints (passwords `{"default":true}`) when
/// present — the typed enum can't represent them. Otherwise falls back to
/// serializing the typed constraints (Db/Fs/Web/Shell).
pub(crate) fn combine_constraints(
    typed: Option<&PermissionConstraints>,
    raw: Option<&serde_json::Value>,
) -> Option<String> {
    raw.and_then(|c| serde_json::to_string(c).ok())
        .or_else(|| typed.and_then(|c| serde_json::to_string(c).ok()))
}
