//! Maps the `permissions` object of a manifest to declared permissions (contracts/permissions.md
//! §Arten, research R15).
//!
//! Manifests of existing haextensions use several spellings for the same thing (`http` and `web`,
//! `cloudStorage` and `remoteStorage`, `operation` and `action`, `readWrite` and `read_write`).
//! They are unified here. Categories holzi does not offer are listed for the installation dialog
//! and never become permissions; a declaration holzi cannot read is listed as invalid instead of
//! being guessed.

use serde_json::Value;

use super::model::{Action, PermissionKind};
use super::target::Target;
use crate::extensions::bundle::{ManifestTool, ToolEffect};

/// One permission an extension declares in its manifest.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeclaredPermission {
    pub kind: PermissionKind,
    pub action: Action,
    pub target: Target,
    /// The target exactly as the manifest writes it; this is what is stored, and [`Target::parse`]
    /// reads it back the same way.
    pub target_text: String,
}

/// A declaration that could not be read.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InvalidDeclaration {
    pub category: String,
    pub target: Option<String>,
    pub action: Option<String>,
}

/// The result of [`map_manifest_permissions`].
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct DeclaredPermissions {
    pub declared: Vec<DeclaredPermission>,
    /// Categories with at least one entry that holzi does not offer, sorted.
    pub unsupported_categories: Vec<String>,
    /// Categories a manifest must never name, as it names them (spec 048 FR-037): the bundle is
    /// refused, whatever their entries are.
    pub forbidden_categories: Vec<String>,
    pub invalid: Vec<InvalidDeclaration>,
}

/// The kind a manifest category stands for, or `None` if holzi does not offer it.
fn category_kind(category: &str) -> Option<PermissionKind> {
    match category {
        "database" => Some(PermissionKind::Database),
        "filesystem" => Some(PermissionKind::Filesystem),
        "http" | "web" => Some(PermissionKind::Web),
        "notifications" => Some(PermissionKind::Notifications),
        "passwords" => Some(PermissionKind::Passwords),
        "cloudStorage" | "remoteStorage" => Some(PermissionKind::RemoteStorage),
        "mail" => Some(PermissionKind::Mail),
        "shell" => Some(PermissionKind::Shell),
        _ => None,
    }
}

/// Whether a manifest category asks for access to encrypted folders, in any spelling
/// (`encryptedFolder`, `encrypted_folders`, `Encrypted-Folder`, …). Only the user grants that, when
/// asked (spec 048 FR-036, FR-037).
fn is_forbidden(category: &str) -> bool {
    let folded: String = category
        .chars()
        .filter(|c| !matches!(c, '-' | '_' | ' ' | '.'))
        .flat_map(char::to_lowercase)
        .collect();
    matches!(folded.as_str(), "encryptedfolder" | "encryptedfolders")
}

/// The action a declaration without one gets: `read` where a kind has `read` and `readWrite`,
/// every method for `web`, the only action of `notifications` and `shell`.
/// `mail` has no such default and must name its action.
fn default_action(kind: PermissionKind) -> Option<&'static str> {
    match kind {
        PermissionKind::Database
        | PermissionKind::Filesystem
        | PermissionKind::Passwords
        | PermissionKind::RemoteStorage => Some("read"),
        PermissionKind::Web => Some("*"),
        PermissionKind::Notifications => Some("show"),
        PermissionKind::Shell => Some("execute"),
        PermissionKind::Mail | PermissionKind::EncryptedFolder | PermissionKind::AgentTool => None,
    }
}

/// Brings the manifest spelling of an action into the stored one.
fn normalise_action(kind: PermissionKind, action: &str) -> String {
    match (kind, action) {
        (_, "read_write") => "readWrite".to_owned(),
        (PermissionKind::Web, method) => method.to_ascii_uppercase(),
        (_, other) => other.to_owned(),
    }
}

/// Maps a manifest's `permissions` value. Anything that is not an object declares nothing.
pub fn map_manifest_permissions(permissions: &Value) -> DeclaredPermissions {
    let mut result = DeclaredPermissions::default();
    let Some(categories) = permissions.as_object() else {
        return result;
    };
    for (category, entries) in categories {
        if is_forbidden(category) {
            result.forbidden_categories.push(category.clone());
            continue;
        }
        let Some(kind) = category_kind(category) else {
            if entries.as_array().is_some_and(|e| !e.is_empty()) {
                result.unsupported_categories.push(category.clone());
            }
            continue;
        };
        let Some(entries) = entries.as_array() else {
            result.invalid.push(InvalidDeclaration {
                category: category.clone(),
                target: None,
                action: None,
            });
            continue;
        };
        for entry in entries {
            match map_entry(kind, entry) {
                Some(declared) => result.declared.push(declared),
                None => result.invalid.push(InvalidDeclaration {
                    category: category.clone(),
                    target: entry
                        .get("target")
                        .and_then(Value::as_str)
                        .map(str::to_owned),
                    action: entry
                        .get("operation")
                        .or_else(|| entry.get("action"))
                        .and_then(Value::as_str)
                        .map(str::to_owned),
                }),
            }
        }
    }
    result.unsupported_categories.sort();
    result.forbidden_categories.sort();
    result
}

/// Each tool of the manifest's `tools` block as an `agentTool` permission (spec 018, research R4):
/// the effect is the action, the name the target. Tools are declared only there; a `permissions`
/// category `agentTool` is not one holzi offers.
pub fn map_manifest_tools(tools: &[ManifestTool]) -> Vec<DeclaredPermission> {
    tools
        .iter()
        .map(|tool| DeclaredPermission {
            kind: PermissionKind::AgentTool,
            action: match tool.effect {
                ToolEffect::Read => Action::Read,
                ToolEffect::Change => Action::Change,
                ToolEffect::Destructive => Action::Destructive,
            },
            target: Target::Tool(tool.name.clone()),
            target_text: tool.name.clone(),
        })
        .collect()
}

fn map_entry(kind: PermissionKind, entry: &Value) -> Option<DeclaredPermission> {
    let target_text = entry.get("target")?.as_str()?.to_owned();
    let action_text = match entry.get("operation").or_else(|| entry.get("action")) {
        Some(value) => value.as_str()?.to_owned(),
        None => default_action(kind)?.to_owned(),
    };
    let action = Action::parse(kind, &normalise_action(kind, &action_text))?;
    let target = Target::parse(kind, &target_text)?;
    Some(DeclaredPermission {
        kind,
        action,
        target,
        target_text,
    })
}

#[cfg(test)]
#[path = "manifest_map_tests.rs"]
mod tests;
