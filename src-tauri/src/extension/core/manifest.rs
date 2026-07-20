use crate::extension::error::ExtensionError;
use crate::extension::permissions::types::{
    split_constraints_value, Action, DbAction, ExtensionPermission, FsAction, IdentityAction,
    MailAction, NotificationsAction, PasswordsAction, PermissionStatus, ResourceType, RwAction,
    ShellAction, SpaceAction, WebAction,
};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::str::FromStr;
use ts_rs::TS;

/// Drizzle migration journal entry from _journal.json
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct MigrationJournalEntry {
    /// Index/order of the migration
    pub idx: u32,
    /// Drizzle schema version
    pub version: String,
    /// Unix timestamp when migration was created
    pub when: u64,
    /// Migration file name without .sql extension (e.g., "0000_initial_schema")
    pub tag: String,
    /// Whether to use statement breakpoints
    pub breakpoints: bool,
}

/// Drizzle migration journal (_journal.json)
/// Note: We only parse the fields we need; Drizzle adds other fields like "dialect"
/// which we intentionally ignore (always SQLite for us)
#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(default)]
pub struct MigrationJournal {
    pub version: String,
    pub entries: Vec<MigrationJournalEntry>,
}

impl Default for MigrationJournal {
    fn default() -> Self {
        Self {
            version: String::new(),
            entries: Vec::new(),
        }
    }
}

/// Repräsentiert einen einzelnen Berechtigungseintrag im Manifest und im UI-Modell.
#[derive(Serialize, Deserialize, Clone, Debug, Default, TS)]
#[ts(export)]
pub struct PermissionEntry {
    pub target: String,

    /// Die auszuführende Aktion (z.B. "read", "read_write", "execute").
    /// Für Web-Permissions ist dies optional und wird ignoriert.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub operation: Option<String>,

    /// Optionale, spezifische Einschränkungen für diese Berechtigung.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(type = "Record<string, unknown>")]
    pub constraints: Option<serde_json::Value>,

    /// Der Status der Berechtigung (wird nur im UI-Modell verwendet).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub status: Option<PermissionStatus>,
}

#[derive(Serialize, Deserialize, TS)]
#[ts(export)]
#[serde(rename_all = "camelCase")]
pub struct ExtensionPreview {
    pub manifest: ExtensionManifest,
    pub is_valid_signature: bool,
    pub editable_permissions: EditablePermissions,
}
/// Definiert die einheitliche Struktur für alle Berechtigungsarten im Manifest und UI.
#[derive(Serialize, Deserialize, Clone, Debug, Default, TS)]
#[ts(export)]
pub struct ExtensionPermissions {
    #[serde(default)]
    pub database: Option<Vec<PermissionEntry>>,
    #[serde(default)]
    pub filesystem: Option<Vec<PermissionEntry>>,
    #[serde(default, alias = "web")]
    pub http: Option<Vec<PermissionEntry>>,
    #[serde(default)]
    pub shell: Option<Vec<PermissionEntry>>,
    #[serde(default, rename = "syncServers")]
    pub sync_servers: Option<Vec<PermissionEntry>>,
    #[serde(default, rename = "cloudStorage")]
    pub cloud_storage: Option<Vec<PermissionEntry>>,
    #[serde(default, rename = "syncRules")]
    pub sync_rules: Option<Vec<PermissionEntry>>,
    #[serde(default)]
    pub spaces: Option<Vec<PermissionEntry>>,
    #[serde(default)]
    pub identities: Option<Vec<PermissionEntry>>,
    #[serde(default)]
    pub passwords: Option<Vec<PermissionEntry>>,
    #[serde(default)]
    pub bookmarks: Option<Vec<PermissionEntry>>,
    #[serde(default)]
    pub mail: Option<Vec<PermissionEntry>>,
    #[serde(default)]
    pub notifications: Option<Vec<PermissionEntry>>,
}

/// Typ-Alias für bessere Lesbarkeit, wenn die Struktur als UI-Modell verwendet wird.
pub type EditablePermissions = ExtensionPermissions;

#[derive(Serialize, Deserialize, Clone, Debug, TS)]
#[ts(export)]
#[serde(rename_all = "lowercase")]
pub enum DisplayMode {
    /// Platform decides: Desktop = window, Mobile/Web = iframe (default)
    Auto,
    /// Always open in native window (if available, falls back to iframe)
    Window,
    /// Always open in iframe (embedded in main app)
    Iframe,
}

impl Default for DisplayMode {
    fn default() -> Self {
        Self::Auto
    }
}

/// Localized fields for a specific locale (e.g. "de", "en")
#[derive(Serialize, Deserialize, Clone, Debug, TS)]
#[ts(export)]
#[serde(rename_all = "camelCase")]
pub struct ManifestI18nEntry {
    pub name: Option<String>,
    pub description: Option<String>,
}

#[derive(Serialize, Deserialize, Clone, Debug, TS)]
#[ts(export)]
#[serde(rename_all = "camelCase")]
pub struct ExtensionManifest {
    pub name: String,
    #[serde(default = "default_version_value")]
    pub version: String,
    pub author: Option<String>,
    #[serde(default = "default_entry_value")]
    pub entry: Option<String>,
    pub icon: Option<String>,
    pub public_key: String,
    pub signature: String,
    pub permissions: ExtensionPermissions,
    pub homepage: Option<String>,
    pub description: Option<String>,
    #[serde(default)]
    pub single_instance: Option<bool>,
    #[serde(default)]
    pub display_mode: Option<DisplayMode>,
    /// Path to the migrations directory relative to the extension root.
    /// Contains Drizzle-style migrations with _journal.json and *.sql files.
    /// Example: "database/migrations"
    #[serde(default)]
    pub migrations_dir: Option<String>,
    /// Locale-specific overrides for name, description, etc.
    /// Key is locale code (e.g. "de", "en"), value contains localized fields.
    #[serde(default)]
    pub i18n: Option<HashMap<String, ManifestI18nEntry>>,
}

fn default_entry_value() -> Option<String> {
    Some("index.html".to_string())
}

fn default_version_value() -> String {
    "0.0.0-dev".to_string()
}

impl ExtensionManifest {
    /// Konvertiert die Manifest-Berechtigungen in das bearbeitbare UI-Modell,
    /// indem der Standardstatus `Granted` gesetzt wird.
    pub fn to_editable_permissions(&self) -> EditablePermissions {
        let mut editable = self.permissions.clone();
        editable.set_all_granted();
        editable
    }
}

impl ExtensionPermissions {
    /// Sets every declared entry's status to `Granted` in place. Used both by
    /// `ExtensionManifest::to_editable_permissions` (the install-preview UI
    /// model) and by the external-bridge authorization grant (declared
    /// core permissions are approved wholesale when the user allows the
    /// client, mirroring how an extension install grants its whole manifest).
    pub fn set_all_granted(&mut self) {
        let set_status_for_list = |list: Option<&mut Vec<PermissionEntry>>| {
            if let Some(entries) = list {
                for entry in entries.iter_mut() {
                    entry.status = Some(PermissionStatus::Granted);
                }
            }
        };

        set_status_for_list(self.database.as_mut());
        set_status_for_list(self.filesystem.as_mut());
        set_status_for_list(self.http.as_mut());
        set_status_for_list(self.shell.as_mut());
        set_status_for_list(self.sync_servers.as_mut());
        set_status_for_list(self.cloud_storage.as_mut());
        set_status_for_list(self.sync_rules.as_mut());
        set_status_for_list(self.spaces.as_mut());
        set_status_for_list(self.identities.as_mut());
        set_status_for_list(self.passwords.as_mut());
        set_status_for_list(self.bookmarks.as_mut());
        set_status_for_list(self.mail.as_mut());
        set_status_for_list(self.notifications.as_mut());
    }

    /// Konvertiert das UI-Modell in die flache Liste von internen `ExtensionPermission`-Objekten.
    pub fn to_internal_permissions(&self, extension_id: &str) -> Vec<ExtensionPermission> {
        let mut permissions = Vec::new();

        if let Some(entries) = &self.database {
            for p in entries {
                if let Some(perm) = Self::create_internal(extension_id, ResourceType::Db, p) {
                    permissions.push(perm);
                }
            }
        }
        if let Some(entries) = &self.filesystem {
            for p in entries {
                if let Some(perm) = Self::create_internal(extension_id, ResourceType::Fs, p) {
                    permissions.push(perm);
                }
            }
        }
        if let Some(entries) = &self.http {
            for p in entries {
                if let Some(perm) = Self::create_internal(extension_id, ResourceType::Web, p) {
                    permissions.push(perm);
                }
            }
        }
        if let Some(entries) = &self.shell {
            for p in entries {
                if let Some(perm) = Self::create_internal(extension_id, ResourceType::Shell, p) {
                    permissions.push(perm);
                }
            }
        }
        if let Some(entries) = &self.sync_servers {
            for p in entries {
                if let Some(perm) =
                    Self::create_internal(extension_id, ResourceType::SyncServers, p)
                {
                    permissions.push(perm);
                }
            }
        }
        if let Some(entries) = &self.cloud_storage {
            for p in entries {
                if let Some(perm) =
                    Self::create_internal(extension_id, ResourceType::CloudStorage, p)
                {
                    permissions.push(perm);
                }
            }
        }
        if let Some(entries) = &self.sync_rules {
            for p in entries {
                if let Some(perm) = Self::create_internal(extension_id, ResourceType::SyncRules, p)
                {
                    permissions.push(perm);
                }
            }
        }
        if let Some(entries) = &self.spaces {
            for p in entries {
                if let Some(perm) = Self::create_internal(extension_id, ResourceType::Spaces, p) {
                    permissions.push(perm);
                }
            }
        }
        if let Some(entries) = &self.identities {
            for p in entries {
                if let Some(perm) = Self::create_internal(extension_id, ResourceType::Identities, p)
                {
                    permissions.push(perm);
                }
            }
        }
        if let Some(entries) = &self.passwords {
            for p in entries {
                if let Some(perm) = Self::create_internal(extension_id, ResourceType::Passwords, p)
                {
                    permissions.push(perm);
                }
            }
        }
        if let Some(entries) = &self.bookmarks {
            for p in entries {
                if let Some(perm) = Self::create_internal(extension_id, ResourceType::Bookmarks, p)
                {
                    permissions.push(perm);
                }
            }
        }
        if let Some(entries) = &self.mail {
            for p in entries {
                if let Some(perm) = Self::create_internal(extension_id, ResourceType::Mail, p) {
                    permissions.push(perm);
                }
            }
        }
        if let Some(entries) = &self.notifications {
            for p in entries {
                if let Some(perm) =
                    Self::create_internal(extension_id, ResourceType::Notifications, p)
                {
                    permissions.push(perm);
                }
            }
        }

        permissions
    }

    /// Parst einen einzelnen `PermissionEntry` und wandelt ihn in die interne, typsichere `ExtensionPermission`-Struktur um.
    fn create_internal(
        extension_id: &str,
        resource_type: ResourceType,
        p: &PermissionEntry,
    ) -> Option<ExtensionPermission> {
        let operation_str = p.operation.as_deref().unwrap_or_default();

        let action = match resource_type {
            ResourceType::Db => DbAction::from_str(operation_str).ok().map(Action::Database),
            ResourceType::Fs => FsAction::from_str(operation_str)
                .ok()
                .map(Action::Filesystem),
            ResourceType::Web => {
                // For web permissions, operation is optional - default to All
                if operation_str.is_empty() {
                    Some(Action::Web(WebAction::All))
                } else {
                    WebAction::from_str(operation_str).ok().map(Action::Web)
                }
            }
            ResourceType::Shell => ShellAction::from_str(operation_str).ok().map(Action::Shell),
            ResourceType::SyncServers => RwAction::from_str(operation_str)
                .ok()
                .map(Action::SyncServers),
            ResourceType::CloudStorage => RwAction::from_str(operation_str)
                .ok()
                .map(Action::CloudStorage),
            ResourceType::SyncRules => RwAction::from_str(operation_str)
                .ok()
                .map(Action::SyncRules),
            ResourceType::Bookmarks => RwAction::from_str(operation_str)
                .ok()
                .map(Action::Bookmarks),
            ResourceType::Spaces => SpaceAction::from_str(operation_str)
                .ok()
                .map(Action::Spaces),
            ResourceType::Identities => IdentityAction::from_str(operation_str)
                .ok()
                .map(Action::Identities),
            ResourceType::Passwords => PasswordsAction::from_str(operation_str)
                .ok()
                .map(Action::Passwords),
            ResourceType::Mail => MailAction::from_str(operation_str).ok().map(Action::Mail),
            ResourceType::Notifications => NotificationsAction::from_str(operation_str)
                .ok()
                .map(Action::Notifications),
            // `ExtensionPermissions` (the manifest/UI model) has no
            // `extensionApi` field — those rows are built directly by the
            // external bridge grant flow, never through this manifest path.
            ResourceType::ExtensionApi => None,
        };

        // Passwords mark their default-label row via a free-form `{"default":true}`
        // constraint that the typed (untagged) `PermissionConstraints` enum can't
        // represent. The passwords-vs-other invariant lives in
        // `permissions::types::split_constraints_value` (single source of truth)
        // so a construction site can't silently get it wrong.
        //
        // Fail closed: a malformed `constraints` value in the manifest used to
        // silently degrade to `(None, None)` ("no constraints" = grants
        // anything within the target). Force the row to `Denied` instead so
        // deny-first precedence makes the request fail rather than fail-open.
        let mut requested_status = p.status.clone().unwrap_or(PermissionStatus::Ask);
        let (constraints, raw_constraints) = match split_constraints_value(
            resource_type,
            p.constraints.as_ref(),
        ) {
            Ok(pair) => pair,
            Err(err) => {
                eprintln!(
                        "[permissions] malformed constraints JSON in manifest entry for extension_id={} resource_type={} target={} — forcing status=Denied (parse error: {})",
                        extension_id,
                        resource_type.as_str(),
                        p.target,
                        err
                    );
                requested_status = PermissionStatus::Denied;
                (None, None)
            }
        };

        action.map(|act| ExtensionPermission {
            id: uuid::Uuid::new_v4().to_string(),
            principal_id: extension_id.to_string(),
            resource_type: resource_type.clone(),
            action: act,
            target: p.target.clone(),
            constraints,
            status: requested_status,
            raw_constraints,
        })
    }
}

#[derive(Serialize, Deserialize, Clone, Debug, TS)]
#[ts(export)]
#[serde(rename_all = "camelCase")]
pub struct ExtensionInfoResponse {
    pub id: String,
    pub public_key: String,
    pub name: String,
    pub version: String,
    pub author: Option<String>,
    pub enabled: bool,
    pub description: Option<String>,
    pub homepage: Option<String>,
    pub icon: Option<String>,
    pub entry: Option<String>,
    pub single_instance: Option<bool>,
    pub display_mode: Option<DisplayMode>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub dev_server_url: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub i18n: Option<HashMap<String, ManifestI18nEntry>>,
}

impl ExtensionInfoResponse {
    pub fn from_extension(
        extension: &crate::extension::core::types::Extension,
    ) -> Result<Self, ExtensionError> {
        use crate::extension::core::types::ExtensionSource;

        let dev_server_url = match &extension.source {
            ExtensionSource::Development { dev_server_url, .. } => Some(dev_server_url.clone()),
            ExtensionSource::Production { .. } => None,
        };

        Ok(Self {
            id: extension.id.clone(),
            public_key: extension.manifest.public_key.clone(),
            name: extension.manifest.name.clone(),
            version: extension.manifest.version.clone(),
            author: extension.manifest.author.clone(),
            enabled: extension.enabled,
            description: extension.manifest.description.clone(),
            homepage: extension.manifest.homepage.clone(),
            icon: extension.manifest.icon.clone(),
            entry: extension.manifest.entry.clone(),
            single_instance: extension.manifest.single_instance,
            display_mode: extension.manifest.display_mode.clone(),
            dev_server_url,
            i18n: extension.manifest.i18n.clone(),
        })
    }
}
