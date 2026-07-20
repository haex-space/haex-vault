use crate::extension::error::ExtensionError;
use serde::{Deserialize, Serialize};
use ts_rs::TS;

use super::actions::{Action, DbAction};
use super::constraints::{combine_constraints, split_constraints, PermissionConstraints};

/// Die interne Repräsentation einer einzelnen, gewährten Berechtigung.
#[derive(Serialize, Deserialize, Clone, Debug, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct ExtensionPermission {
    pub id: String,
    pub principal_id: String,
    pub resource_type: ResourceType,
    pub action: Action,
    pub target: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub constraints: Option<PermissionConstraints>,
    pub status: PermissionStatus,
    /// Raw, free-form constraints JSON for resource types whose constraints
    /// can't be represented by the typed (untagged) [`PermissionConstraints`]
    /// enum — currently only `passwords`, which marks its *default label* row
    /// via `{"default": true}`.
    ///
    /// Backend-only write-path carrier: populated from the manifest in
    /// `create_internal` and written to the DB `constraints` column by
    /// `From<&ExtensionPermission> for HaexPrincipalPermissions`. The typed
    /// `constraints` field above is left `None` for these rows. Never crosses
    /// the JSON boundary to the frontend, hence `#[serde(skip)]` / `#[ts(skip)]`.
    #[serde(skip)]
    #[ts(skip)]
    pub raw_constraints: Option<serde_json::Value>,
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, Hash, TS)]
#[serde(rename_all = "lowercase")]
#[ts(export)]
pub enum ResourceType {
    Fs,
    Web,
    Db,
    Shell,
    #[serde(rename = "syncServers")]
    SyncServers,
    #[serde(rename = "cloudStorage")]
    CloudStorage,
    #[serde(rename = "syncRules")]
    SyncRules,
    Spaces,
    Identities,
    Passwords,
    Bookmarks,
    Mail,
    Notifications,
    #[serde(rename = "extensionApi")]
    ExtensionApi,
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, TS)]
#[serde(rename_all = "lowercase")]
#[ts(export)]
pub enum PermissionStatus {
    Ask,
    Granted,
    Denied,
}

impl ResourceType {
    pub fn as_str(&self) -> &str {
        match self {
            ResourceType::Fs => "fs",
            ResourceType::Web => "web",
            ResourceType::Db => "db",
            ResourceType::Shell => "shell",
            ResourceType::SyncServers => "syncServers",
            ResourceType::CloudStorage => "cloudStorage",
            ResourceType::SyncRules => "syncRules",
            ResourceType::Spaces => "spaces",
            ResourceType::Identities => "identities",
            ResourceType::Passwords => "passwords",
            ResourceType::Bookmarks => "bookmarks",
            ResourceType::Mail => "mail",
            ResourceType::Notifications => "notifications",
            ResourceType::ExtensionApi => "extensionApi",
        }
    }

    pub fn from_str(s: &str) -> Result<Self, ExtensionError> {
        match s {
            "fs" => Ok(ResourceType::Fs),
            "web" => Ok(ResourceType::Web),
            "db" => Ok(ResourceType::Db),
            "shell" => Ok(ResourceType::Shell),
            "syncServers" => Ok(ResourceType::SyncServers),
            "cloudStorage" => Ok(ResourceType::CloudStorage),
            "syncRules" => Ok(ResourceType::SyncRules),
            "spaces" => Ok(ResourceType::Spaces),
            "identities" => Ok(ResourceType::Identities),
            "passwords" => Ok(ResourceType::Passwords),
            "bookmarks" => Ok(ResourceType::Bookmarks),
            "mail" => Ok(ResourceType::Mail),
            "notifications" => Ok(ResourceType::Notifications),
            "extensionApi" => Ok(ResourceType::ExtensionApi),
            _ => Err(ExtensionError::ValidationError {
                reason: format!("Unknown resource type: {s}"),
            }),
        }
    }
}

impl PermissionStatus {
    pub fn as_str(&self) -> &str {
        match self {
            PermissionStatus::Ask => "ask",
            PermissionStatus::Granted => "granted",
            PermissionStatus::Denied => "denied",
        }
    }

    pub fn from_str(s: &str) -> Result<Self, ExtensionError> {
        match s {
            "ask" => Ok(PermissionStatus::Ask),
            "granted" => Ok(PermissionStatus::Granted),
            "denied" => Ok(PermissionStatus::Denied),
            _ => Err(ExtensionError::ValidationError {
                reason: format!("Unknown permission status: {s}"),
            }),
        }
    }
}

// --- Konvertierungen zwischen ExtensionPermission und HaexPrincipalPermissions ---

impl From<&ExtensionPermission> for crate::database::generated::HaexPrincipalPermissions {
    fn from(perm: &ExtensionPermission) -> Self {
        Self {
            id: perm.id.clone(),
            principal_id: perm.principal_id.clone(),
            resource_type: Some(perm.resource_type.as_str().to_string()),
            action: Some(perm.action.as_str().to_string()),
            target: Some(perm.target.clone()),
            constraints: combine_constraints(
                perm.constraints.as_ref(),
                perm.raw_constraints.as_ref(),
            ),
            status: perm.status.as_str().to_string(),
            created_at: None,
            updated_at: None,
        }
    }
}

impl From<crate::database::generated::HaexPrincipalPermissions> for ExtensionPermission {
    fn from(db_perm: crate::database::generated::HaexPrincipalPermissions) -> Self {
        let resource_type = db_perm
            .resource_type
            .as_deref()
            .and_then(|s| ResourceType::from_str(s).ok())
            .unwrap_or(ResourceType::Db);

        let action = db_perm
            .action
            .as_deref()
            .and_then(|s| Action::from_str(&resource_type, s).ok())
            .unwrap_or(Action::Database(DbAction::Read));

        let mut status =
            PermissionStatus::from_str(db_perm.status.as_str()).unwrap_or(PermissionStatus::Denied);

        // Fail closed: a row whose `constraints` column is malformed JSON used
        // to silently become `(None, None)` — i.e. "no constraints" — which
        // *grants* anything the row's (resource_type, target, action) matches.
        // The row was MEANT to restrict access, so the security-correct response
        // is to force the row to `Denied`. Deny-first precedence then makes
        // sure this row participates in matching and denies the request rather
        // than disappearing.
        let (constraints, raw_constraints) = match split_constraints(
            resource_type,
            db_perm.constraints.as_deref(),
        ) {
            Ok(pair) => pair,
            Err(err) => {
                eprintln!(
                    "[permissions] malformed constraints JSON on permission id={} principal_id={} resource_type={} target={:?} — forcing status=Denied (parse error: {})",
                    db_perm.id,
                    db_perm.principal_id,
                    resource_type.as_str(),
                    db_perm.target,
                    err
                );
                status = PermissionStatus::Denied;
                (None, None)
            }
        };

        Self {
            id: db_perm.id,
            principal_id: db_perm.principal_id,
            resource_type,
            action,
            target: db_perm.target.unwrap_or_default(),
            constraints,
            status,
            raw_constraints,
        }
    }
}
