use crate::extension::error::ExtensionError;
use serde::{Deserialize, Serialize};
use std::str::FromStr;
use ts_rs::TS;

use super::permission::ResourceType;

// --- Spezifische Aktionen ---

/// Definiert Aktionen, die auf eine Datenbank angewendet werden können.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, TS)]
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
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize, TS)]
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
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize, TS)]
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
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize, TS)]
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

/// Generische Read/ReadWrite-Aktion, geteilt von den Sync-Ressourcen
/// (`SyncServers`, `CloudStorage`, `SyncRules`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub enum RwAction {
    Read,
    ReadWrite,
}

impl RwAction {
    /// Prüft, ob diese Aktion Lesezugriff gewährt.
    pub fn allows_read(&self) -> bool {
        matches!(self, RwAction::Read | RwAction::ReadWrite)
    }

    /// Prüft, ob diese Aktion Schreibzugriff gewährt.
    pub fn allows_write(&self) -> bool {
        matches!(self, RwAction::ReadWrite)
    }

    /// Returns the action as a string for serialization
    pub fn as_str(&self) -> &'static str {
        match self {
            RwAction::Read => "read",
            RwAction::ReadWrite => "readWrite",
        }
    }
}

impl FromStr for RwAction {
    type Err = ExtensionError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "read" => Ok(RwAction::Read),
            "readwrite" | "read_write" => Ok(RwAction::ReadWrite),
            _ => Err(ExtensionError::InvalidActionString {
                input: s.to_string(),
                resource_type: "rwAction".to_string(),
            }),
        }
    }
}

/// Definiert Aktionen, die auf Shared Spaces angewendet werden können.
/// Read = Spaces lesen/anzeigen, ReadWrite = zusätzlich Spaces anlegen.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub enum SpaceAction {
    Read,
    ReadWrite,
}

/// Definiert Aktionen, die auf Identitäten angewendet werden können.
///
/// Read = list/view identities + contacts. Write = add a NEW contact only
/// (private_key NULL); never returns/sets private_key, never creates/deletes
/// owned identities. Enforcement lives in the identity bridge commands
/// (later phase).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub enum IdentityAction {
    Read,
    Write,
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
    /// Read und Write sind DISTINCT capabilities, keine Hierarchie:
    /// Write impliziert kein Read.
    pub fn allows_read(&self) -> bool {
        matches!(self, IdentityAction::Read)
    }

    /// Write = add a NEW contact only; impliziert kein Read.
    pub fn allows_write(&self) -> bool {
        matches!(self, IdentityAction::Write)
    }

    /// Returns the action as a string for serialization
    pub fn as_str(&self) -> &'static str {
        match self {
            IdentityAction::Read => "read",
            IdentityAction::Write => "write",
        }
    }
}

impl FromStr for IdentityAction {
    type Err = ExtensionError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "read" => Ok(IdentityAction::Read),
            "write" => Ok(IdentityAction::Write),
            _ => Err(ExtensionError::InvalidActionString {
                input: s.to_string(),
                resource_type: "identities".to_string(),
            }),
        }
    }
}

/// Aktionen auf dem Mail-Modul (IMAP fetch + SMTP send).
///
/// Der natürliche Split bei Mail ist Protokoll-basiert (IMAP vs SMTP),
/// nicht read/write — IMAP-Server und SMTP-Server sind oft unterschiedliche
/// Hosts. `target` ist der Mailserver-Hostname (z.B. "imap.gmail.com")
/// oder "*" als Wildcard. Subdomain-Match: target="gmail.com" matched
/// "imap.gmail.com" und "smtp.gmail.com".
///
/// `Fetch` umfasst alle IMAP-Operationen (LIST, FETCH, STORE/Flags, MOVE,
/// DELETE, APPEND) — extra read/write-Trennung lohnt nicht, weil "lesen"
/// bei IMAP bereits den vollen Datenzugriff bedeutet.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub enum MailAction {
    Fetch,
    Send,
}

impl MailAction {
    pub fn as_str(&self) -> &'static str {
        match self {
            MailAction::Fetch => "fetch",
            MailAction::Send => "send",
        }
    }
}

impl FromStr for MailAction {
    type Err = ExtensionError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "fetch" => Ok(MailAction::Fetch),
            "send" => Ok(MailAction::Send),
            _ => Err(ExtensionError::InvalidActionString {
                input: s.to_string(),
                resource_type: "mail".to_string(),
            }),
        }
    }
}

/// Aktionen auf dem generischen Notifications-Modul.
///
/// Aktuell nur `Show` (OS-Notification anzeigen). `target` ist immer "*" —
/// Notifications sind nicht ressourcen-gescoped; die Identität wird über den
/// Public Key der aufrufenden Extension gepinnt (siehe `extension::notifications`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub enum NotificationsAction {
    Show,
}

impl NotificationsAction {
    pub fn as_str(&self) -> &'static str {
        match self {
            NotificationsAction::Show => "show",
        }
    }
}

impl FromStr for NotificationsAction {
    type Err = ExtensionError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "show" => Ok(NotificationsAction::Show),
            _ => Err(ExtensionError::InvalidActionString {
                input: s.to_string(),
                resource_type: "notifications".to_string(),
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
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize, TS)]
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

/// Ergebnis einer Passwords-Permission-Prüfung.
///
/// Beschreibt welche Tag-Scopes eine Extension für die angefragte Aktion lesen
/// bzw. schreiben darf. Wird von den Bridge-Commands verwendet um SQL-Queries
/// auf die erlaubten Tags zu begrenzen.
#[derive(Debug, Clone, PartialEq)]
pub enum PasswordsScope {
    /// Wildcard — Extension darf auf Einträge mit beliebigen Tags zugreifen.
    /// Vollzugriff hat kein Default-Label (nichts wird beim Erstellen erzwungen).
    All,
    /// Extension darf nur auf Einträge zugreifen die mindestens eines dieser
    /// Tags haben.
    ///
    /// `default` ist das *Default-Label*, das neu erstellten Einträgen
    /// automatisch zugewiesen wird:
    /// - Genau ein erlaubtes Label → dieses ist implizit der Default.
    /// - Mehrere erlaubte Labels → genau eine Permission-Row muss explizit per
    ///   `{"default":true}` markiert sein; `default` trägt dann dieses Label.
    /// - Read-only-Scopes brauchen keinen Default (`None` ist erlaubt).
    Tags {
        tags: Vec<String>,
        default: Option<String>,
    },
}

impl PasswordsScope {
    /// Das Default-Label dieses Scopes (das neu erstellten Einträgen
    /// zugewiesen wird). `All` und ein Read-only-`Tags`-Scope ohne Default
    /// liefern `None`.
    pub fn default_label(&self) -> Option<&str> {
        match self {
            PasswordsScope::All => None,
            PasswordsScope::Tags { default, .. } => default.as_deref(),
        }
    }
}

// --- Haupt-Typ für Aktions-Container ---

/// Ein typsicherer Container, der die spezifische Aktion für einen Ressourcentyp enthält.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize, TS)]
#[ts(export)]
pub enum Action {
    Database(DbAction),
    Filesystem(FsAction),
    Web(WebAction),
    Shell(ShellAction),
    SyncServers(RwAction),
    CloudStorage(RwAction),
    SyncRules(RwAction),
    Spaces(SpaceAction),
    Identities(IdentityAction),
    Passwords(PasswordsAction),
    Mail(MailAction),
    Notifications(NotificationsAction),
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
            Action::SyncServers(action) => serde_json::to_string(action)
                .unwrap_or_default()
                .trim_matches('"')
                .to_string(),
            Action::CloudStorage(action) => serde_json::to_string(action)
                .unwrap_or_default()
                .trim_matches('"')
                .to_string(),
            Action::SyncRules(action) => serde_json::to_string(action)
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
            Action::Mail(action) => serde_json::to_string(action)
                .unwrap_or_default()
                .trim_matches('"')
                .to_string(),
            Action::Notifications(action) => serde_json::to_string(action)
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
            ResourceType::SyncServers => Ok(Action::SyncServers(RwAction::from_str(s)?)),
            ResourceType::CloudStorage => Ok(Action::CloudStorage(RwAction::from_str(s)?)),
            ResourceType::SyncRules => Ok(Action::SyncRules(RwAction::from_str(s)?)),
            ResourceType::Spaces => Ok(Action::Spaces(SpaceAction::from_str(s)?)),
            ResourceType::Identities => Ok(Action::Identities(IdentityAction::from_str(s)?)),
            ResourceType::Passwords => Ok(Action::Passwords(PasswordsAction::from_str(s)?)),
            ResourceType::Mail => Ok(Action::Mail(MailAction::from_str(s)?)),
            ResourceType::Notifications => {
                Ok(Action::Notifications(NotificationsAction::from_str(s)?))
            }
        }
    }
}
