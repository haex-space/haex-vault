use crate::extension::error::ExtensionError;
use serde::{Deserialize, Serialize};
use std::str::FromStr;
use ts_rs::TS;

/// FileSync target types for permission matching
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum FileSyncTarget {
    /// All FileSync resources
    All,
    /// File spaces
    Spaces,
    /// Storage backends
    Backends,
    /// Sync rules
    Rules,
}

impl FileSyncTarget {
    pub fn as_str(&self) -> &str {
        match self {
            FileSyncTarget::All => "*",
            FileSyncTarget::Spaces => "spaces",
            FileSyncTarget::Backends => "backends",
            FileSyncTarget::Rules => "rules",
        }
    }

    pub fn from_str(s: &str) -> Option<Self> {
        match s {
            "*" => Some(FileSyncTarget::All),
            "spaces" => Some(FileSyncTarget::Spaces),
            "backends" => Some(FileSyncTarget::Backends),
            "rules" => Some(FileSyncTarget::Rules),
            _ => None,
        }
    }

    /// Checks if this target matches the required target
    pub fn matches(&self, required: FileSyncTarget) -> bool {
        match self {
            FileSyncTarget::All => true, // * matches everything
            other => *other == required,
        }
    }
}

// --- Spezifische Aktionen ---

/// Definiert Aktionen, die auf eine Datenbank angewendet werden können.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub enum DbAction {
    Read,
    ReadWrite,
    Create,
    Delete,
    AlterDrop,
}

impl DbAction {
    /// Prüft, ob diese Aktion Lesezugriff gewährt (implizites Recht).
    pub fn allows_read(&self) -> bool {
        matches!(self, DbAction::Read | DbAction::ReadWrite)
    }

    /// Prüft, ob diese Aktion Schreibzugriff gewährt.
    pub fn allows_write(&self) -> bool {
        matches!(
            self,
            DbAction::ReadWrite | DbAction::Create | DbAction::Delete
        )
    }

    /// Returns the action as a lowercase string for serialization
    pub fn as_str(&self) -> &'static str {
        match self {
            DbAction::Read => "read",
            DbAction::ReadWrite => "readWrite",
            DbAction::Create => "create",
            DbAction::Delete => "delete",
            DbAction::AlterDrop => "alterDrop",
        }
    }
}

impl FromStr for DbAction {
    type Err = ExtensionError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "read" => Ok(DbAction::Read),
            "readwrite" | "read_write" => Ok(DbAction::ReadWrite),
            "create" => Ok(DbAction::Create),
            "delete" => Ok(DbAction::Delete),
            "alterdrop" | "alter_drop" => Ok(DbAction::AlterDrop),
            _ => Err(ExtensionError::InvalidActionString {
                input: s.to_string(),
                resource_type: "database".to_string(),
            }),
        }
    }
}

/// Definiert Aktionen, die auf das Dateisystem angewendet werden können.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub enum FsAction {
    Read,
    ReadWrite,
}

impl FsAction {
    /// Prüft, ob diese Aktion Lesezugriff gewährt (implizites Recht).
    pub fn allows_read(&self) -> bool {
        matches!(self, FsAction::Read | FsAction::ReadWrite)
    }

    /// Prüft, ob diese Aktion Schreibzugriff gewährt.
    pub fn allows_write(&self) -> bool {
        matches!(self, FsAction::ReadWrite)
    }

    /// Returns the action as a string for serialization
    pub fn as_str(&self) -> &'static str {
        match self {
            FsAction::Read => "read",
            FsAction::ReadWrite => "readWrite",
        }
    }
}

impl FromStr for FsAction {
    type Err = ExtensionError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "read" => Ok(FsAction::Read),
            "readwrite" | "read_write" => Ok(FsAction::ReadWrite),
            _ => Err(ExtensionError::InvalidActionString {
                input: s.to_string(),
                resource_type: "filesystem".to_string(),
            }),
        }
    }
}

/// Definiert Aktionen (HTTP-Methoden), die auf Web-Anfragen angewendet werden können.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "UPPERCASE")]
#[ts(export)]
pub enum WebAction {
    Get,
    Post,
    Put,
    Patch,
    Delete,
    #[serde(rename = "*")]
    All,
}

impl FromStr for WebAction {
    type Err = ExtensionError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_uppercase().as_str() {
            "GET" => Ok(WebAction::Get),
            "POST" => Ok(WebAction::Post),
            "PUT" => Ok(WebAction::Put),
            "PATCH" => Ok(WebAction::Patch),
            "DELETE" => Ok(WebAction::Delete),
            "*" => Ok(WebAction::All),
            _ => Err(ExtensionError::InvalidActionString {
                input: s.to_string(),
                resource_type: "web".to_string(),
            }),
        }
    }
}

/// Definiert Aktionen, die auf Shell-Befehle angewendet werden können.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "lowercase")]
#[ts(export)]
pub enum ShellAction {
    Execute,
}

impl FromStr for ShellAction {
    type Err = ExtensionError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "execute" => Ok(ShellAction::Execute),
            _ => Err(ExtensionError::InvalidActionString {
                input: s.to_string(),
                resource_type: "shell".to_string(),
            }),
        }
    }
}

/// Definiert Aktionen, die auf FileSync (Cloud-Sync) angewendet werden können.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub enum FileSyncAction {
    Read,
    ReadWrite,
}

impl FileSyncAction {
    /// Prüft, ob diese Aktion Lesezugriff gewährt.
    pub fn allows_read(&self) -> bool {
        matches!(self, FileSyncAction::Read | FileSyncAction::ReadWrite)
    }

    /// Prüft, ob diese Aktion Schreibzugriff gewährt.
    pub fn allows_write(&self) -> bool {
        matches!(self, FileSyncAction::ReadWrite)
    }
}

impl FromStr for FileSyncAction {
    type Err = ExtensionError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "read" => Ok(FileSyncAction::Read),
            "readwrite" | "read_write" => Ok(FileSyncAction::ReadWrite),
            _ => Err(ExtensionError::InvalidActionString {
                input: s.to_string(),
                resource_type: "filesync".to_string(),
            }),
        }
    }
}

/// Definiert Aktionen, die auf Shared Spaces angewendet werden können.
/// Read = Spaces lesen/anzeigen, ReadWrite = zusätzlich Spaces anlegen.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub enum SpaceAction {
    Read,
    ReadWrite,
}

/// Definiert Aktionen, die auf Identitäten angewendet werden können.
/// Read-only: Extensions können Identitäten nur auflisten/anzeigen.
/// Erstellen und Löschen bleibt haex-vault vorbehalten.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub enum IdentityAction {
    Read,
}

impl SpaceAction {
    pub fn allows_read(&self) -> bool {
        matches!(self, SpaceAction::Read | SpaceAction::ReadWrite)
    }

    pub fn allows_write(&self) -> bool {
        matches!(self, SpaceAction::ReadWrite)
    }
}

impl FromStr for SpaceAction {
    type Err = ExtensionError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "read" => Ok(SpaceAction::Read),
            "readwrite" | "read_write" => Ok(SpaceAction::ReadWrite),
            _ => Err(ExtensionError::InvalidActionString {
                input: s.to_string(),
                resource_type: "spaces".to_string(),
            }),
        }
    }
}

impl IdentityAction {
    pub fn allows_read(&self) -> bool {
        matches!(self, IdentityAction::Read)
    }
}

impl FromStr for IdentityAction {
    type Err = ExtensionError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "read" => Ok(IdentityAction::Read),
            _ => Err(ExtensionError::InvalidActionString {
                input: s.to_string(),
                resource_type: "identities".to_string(),
            }),
        }
    }
}

/// Aktionen auf dem Core-Passworttresor.
///
/// Scope wird über `ExtensionPermission.target` als Tag-Filter gesteuert
/// (z.B. target="calendar" => nur Items mit Tag "calendar", target="*" => alle).
/// Writes müssen mindestens ein Tag innerhalb des erlaubten Scopes setzen –
/// Enforcement geschieht in den Bridge-Commands.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub enum PasswordsAction {
    Read,
    ReadWrite,
}

impl PasswordsAction {
    pub fn allows_read(&self) -> bool {
        matches!(self, PasswordsAction::Read | PasswordsAction::ReadWrite)
    }

    pub fn allows_write(&self) -> bool {
        matches!(self, PasswordsAction::ReadWrite)
    }
}

impl FromStr for PasswordsAction {
    type Err = ExtensionError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "read" => Ok(PasswordsAction::Read),
            "readwrite" | "read_write" => Ok(PasswordsAction::ReadWrite),
            _ => Err(ExtensionError::InvalidActionString {
                input: s.to_string(),
                resource_type: "passwords".to_string(),
            }),
        }
    }
}

// --- Haupt-Typen für Berechtigungen ---

/// Ein typsicherer Container, der die spezifische Aktion für einen Ressourcentyp enthält.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub enum Action {
    Database(DbAction),
    Filesystem(FsAction),
    Web(WebAction),
    Shell(ShellAction),
    FileSync(FileSyncAction),
    Spaces(SpaceAction),
    Identities(IdentityAction),
    Passwords(PasswordsAction),
}

/// Die interne Repräsentation einer einzelnen, gewährten Berechtigung.
#[derive(Serialize, Deserialize, Clone, Debug, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct ExtensionPermission {
    pub id: String,
    pub extension_id: String,
    pub resource_type: ResourceType,
    pub action: Action,
    pub target: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub constraints: Option<PermissionConstraints>,
    pub status: PermissionStatus,
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, Hash, TS)]
#[serde(rename_all = "lowercase")]
#[ts(export)]
pub enum ResourceType {
    Fs,
    Web,
    Db,
    Shell,
    Filesync,
    Spaces,
    Identities,
    Passwords,
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, TS)]
#[serde(rename_all = "lowercase")]
#[ts(export)]
pub enum PermissionStatus {
    Ask,
    Granted,
    Denied,
}

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

// --- Konvertierungen zwischen ExtensionPermission und HaexExtensionPermissions ---

impl ResourceType {
    pub fn as_str(&self) -> &str {
        match self {
            ResourceType::Fs => "fs",
            ResourceType::Web => "web",
            ResourceType::Db => "db",
            ResourceType::Shell => "shell",
            ResourceType::Filesync => "filesync",
            ResourceType::Spaces => "spaces",
            ResourceType::Identities => "identities",
            ResourceType::Passwords => "passwords",
        }
    }

    pub fn from_str(s: &str) -> Result<Self, ExtensionError> {
        match s {
            "fs" => Ok(ResourceType::Fs),
            "web" => Ok(ResourceType::Web),
            "db" => Ok(ResourceType::Db),
            "shell" => Ok(ResourceType::Shell),
            "filesync" => Ok(ResourceType::Filesync),
            "spaces" => Ok(ResourceType::Spaces),
            "identities" => Ok(ResourceType::Identities),
            "passwords" => Ok(ResourceType::Passwords),
            _ => Err(ExtensionError::ValidationError {
                reason: format!("Unknown resource type: {s}"),
            }),
        }
    }
}

impl Action {
    pub fn as_str(&self) -> String {
        match self {
            Action::Database(action) => serde_json::to_string(action)
                .unwrap_or_default()
                .trim_matches('"')
                .to_string(),
            Action::Filesystem(action) => serde_json::to_string(action)
                .unwrap_or_default()
                .trim_matches('"')
                .to_string(),
            Action::Web(action) => serde_json::to_string(action)
                .unwrap_or_default()
                .trim_matches('"')
                .to_string(),
            Action::Shell(action) => serde_json::to_string(action)
                .unwrap_or_default()
                .trim_matches('"')
                .to_string(),
            Action::FileSync(action) => serde_json::to_string(action)
                .unwrap_or_default()
                .trim_matches('"')
                .to_string(),
            Action::Spaces(action) => serde_json::to_string(action)
                .unwrap_or_default()
                .trim_matches('"')
                .to_string(),
            Action::Identities(action) => serde_json::to_string(action)
                .unwrap_or_default()
                .trim_matches('"')
                .to_string(),
            Action::Passwords(action) => serde_json::to_string(action)
                .unwrap_or_default()
                .trim_matches('"')
                .to_string(),
        }
    }

    pub fn from_str(resource_type: &ResourceType, s: &str) -> Result<Self, ExtensionError> {
        match resource_type {
            ResourceType::Db => Ok(Action::Database(DbAction::from_str(s)?)),
            ResourceType::Fs => Ok(Action::Filesystem(FsAction::from_str(s)?)),
            ResourceType::Web => {
                let action: WebAction =
                    serde_json::from_str(&format!("\"{s}\"")).map_err(|_| {
                        ExtensionError::InvalidActionString {
                            input: s.to_string(),
                            resource_type: "web".to_string(),
                        }
                    })?;
                Ok(Action::Web(action))
            }
            ResourceType::Shell => Ok(Action::Shell(ShellAction::from_str(s)?)),
            ResourceType::Filesync => Ok(Action::FileSync(FileSyncAction::from_str(s)?)),
            ResourceType::Spaces => Ok(Action::Spaces(SpaceAction::from_str(s)?)),
            ResourceType::Identities => Ok(Action::Identities(IdentityAction::from_str(s)?)),
            ResourceType::Passwords => Ok(Action::Passwords(PasswordsAction::from_str(s)?)),
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

impl From<&ExtensionPermission> for crate::database::generated::HaexExtensionPermissions {
    fn from(perm: &ExtensionPermission) -> Self {
        Self {
            id: perm.id.clone(),
            extension_id: perm.extension_id.clone(),
            resource_type: Some(perm.resource_type.as_str().to_string()),
            action: Some(perm.action.as_str().to_string()),
            target: Some(perm.target.clone()),
            constraints: perm
                .constraints
                .as_ref()
                .and_then(|c| serde_json::to_string(c).ok()),
            status: perm.status.as_str().to_string(),
            created_at: None,
            updated_at: None,
        }
    }
}

impl From<crate::database::generated::HaexExtensionPermissions> for ExtensionPermission {
    fn from(db_perm: crate::database::generated::HaexExtensionPermissions) -> Self {
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

        let status =
            PermissionStatus::from_str(db_perm.status.as_str()).unwrap_or(PermissionStatus::Denied);

        let constraints = db_perm
            .constraints
            .as_deref()
            .and_then(|s| serde_json::from_str(s).ok());

        Self {
            id: db_perm.id,
            extension_id: db_perm.extension_id,
            resource_type,
            action,
            target: db_perm.target.unwrap_or_default(),
            constraints,
            status,
        }
    }
}
