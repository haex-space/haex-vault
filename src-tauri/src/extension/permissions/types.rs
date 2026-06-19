use crate::extension::error::ExtensionError;
use serde::{Deserialize, Serialize};
use std::str::FromStr;
use ts_rs::TS;

/// A permission principal — the actor a permission check is performed against.
///
/// Today every principal is an extension (`principal_id == extension_id`), so
/// the permission layer behaves exactly as before. `ExternalClient` is wired in
/// ahead of the external-bridge work where clients become first-class
/// principals sharing the same `haex_principal_permissions` machinery.
//
// `ExternalClient` + `kind_str` are not constructed/called in production yet
// (only in unit tests) — they exist for the upcoming external-client phases.
#[allow(dead_code)]
#[derive(Debug, Clone, PartialEq)]
pub enum Principal {
    Extension(String),
    ExternalClient(String),
}

impl Principal {
    /// The principal's id — the value stored in `haex_principal_permissions.principal_id`.
    pub fn id(&self) -> &str {
        match self {
            Self::Extension(i) | Self::ExternalClient(i) => i,
        }
    }

    /// The principal kind as it is persisted in `haex_principals.kind`.
    #[allow(dead_code)]
    pub fn kind_str(&self) -> &'static str {
        match self {
            Self::Extension(_) => "extension",
            Self::ExternalClient(_) => "external_client",
        }
    }

    /// Whether this principal is an extension. Used to gate extension-only
    /// behaviour (e.g. auto-allowed own tables) that external clients lack.
    pub fn is_extension(&self) -> bool {
        matches!(self, Self::Extension(_))
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

/// Generische Read/ReadWrite-Aktion, geteilt von den Sync-Ressourcen
/// (`SyncServers`, `CloudStorage`, `SyncRules`).
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, TS)]
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
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
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
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, TS)]
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
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, TS)]
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
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, TS)]
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

// --- Haupt-Typen für Berechtigungen ---

/// Ein typsicherer Container, der die spezifische Aktion für einen Ressourcentyp enthält.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
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
    Mail,
    Notifications,
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

// --- Konvertierungen zwischen ExtensionPermission und HaexPrincipalPermissions ---

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
            ResourceType::Mail => "mail",
            ResourceType::Notifications => "notifications",
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
            "mail" => Ok(ResourceType::Mail),
            "notifications" => Ok(ResourceType::Notifications),
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

        let status =
            PermissionStatus::from_str(db_perm.status.as_str()).unwrap_or(PermissionStatus::Denied);

        let (constraints, raw_constraints) =
            split_constraints(resource_type, db_perm.constraints.as_deref());

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
