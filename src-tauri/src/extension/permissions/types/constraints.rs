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
///
/// **Fails closed:** a malformed constraints `Value` for a non-`Passwords`
/// resource returns `Err` rather than collapsing to `(None, None)`. The previous
/// `.ok()`-shaped silent fallback let a row that *had* constraints (and was
/// therefore meant to restrict permissions) be treated as "no constraints" —
/// i.e. fail-open. Callers must translate the `Err` into a deny decision (e.g.
/// `From<HaexPrincipalPermissions> for ExtensionPermission` forces
/// `PermissionStatus::Denied`).
pub(crate) fn split_constraints_value(
    resource_type: ResourceType,
    value: Option<&serde_json::Value>,
) -> Result<(Option<PermissionConstraints>, Option<serde_json::Value>), serde_json::Error> {
    if resource_type == ResourceType::Passwords {
        Ok((None, value.cloned()))
    } else {
        match value {
            None => Ok((None, None)),
            Some(v) => {
                let typed: PermissionConstraints = serde_json::from_value(v.clone())?;
                Ok((Some(typed), None))
            }
        }
    }
}

/// Splits a constraints **text** column (DB `constraints`) into the
/// `(typed, raw)` pair used by `ExtensionPermission`.
///
/// Same passwords-vs-other invariant as [`split_constraints_value`], but the
/// input is the raw DB text (the READ/DB-text direction): for `passwords` the
/// text is parsed into a free-form `Value` and kept raw; every other resource
/// type parses the text into the typed enum.
///
/// **Fails closed** the same way as [`split_constraints_value`]: malformed JSON
/// (or a typed parse failure) on a non-`Passwords` row returns `Err` instead of
/// silently dropping the constraints. The `Passwords` raw-pass-through path
/// also surfaces JSON-syntax errors. `None` input is always `Ok((None, None))`
/// — a missing constraints column is legitimate.
pub(crate) fn split_constraints(
    resource_type: ResourceType,
    raw_text: Option<&str>,
) -> Result<(Option<PermissionConstraints>, Option<serde_json::Value>), serde_json::Error> {
    match raw_text {
        None => Ok((None, None)),
        Some(s) => {
            if resource_type == ResourceType::Passwords {
                let raw: serde_json::Value = serde_json::from_str(s)?;
                Ok((None, Some(raw)))
            } else {
                let typed: PermissionConstraints = serde_json::from_str(s)?;
                Ok((Some(typed), None))
            }
        }
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
