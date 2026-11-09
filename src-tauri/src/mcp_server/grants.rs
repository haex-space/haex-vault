//! What a registered agent may do (spec 021 research R3, FR-008, FR-012 to FR-016). One function
//! decides the tool list and every call, so a tool an agent does not see is also a tool it cannot
//! call. The rights group "holzi bedienen" (`operate`) has per scope of the action catalog nothing,
//! reading, or reading and changing; the group `browser` (spec 065) opens the browser tools and
//! names the websites an agent may read or operate; the group `extension` names the extensions
//! whose tools it sees (US4, FR-018). Files and contacts follow the agent's rows in
//! `agent_file_permissions` ([`DataRights`], US5, FR-020), and a native action is offered only when
//! its executor serves external agents. The group `passwords` opens the search of the password
//! manager and the single secret, within its tags (US5, FR-019); the group `commands` opens holzi's
//! command tool (`command_tool.rs`). The scope `guardrails` is never a right. A later group adds a
//! field that is off when missing (FR-008).

use std::collections::{BTreeMap, BTreeSet, HashSet};
use std::sync::Arc;

use serde::{Deserialize, Serialize};
use ts_rs::TS;
use uuid::Uuid;

use crate::adapters::cli_delegate::autonomy::{AutonomyMode, DenyCategory};
use crate::agent_browser::sites::Site;
use crate::chat::action_commands::GUARDRAILS_SCOPE;
use crate::chat::tools::action_tool::{ActionEffect, ActionRunner, AgentActionDef};
use crate::chat::tools::Tool;
use crate::contacts::access::GrantAction;
use crate::contacts::agent::grants_from;
use crate::files::access::{AgentGrants, DeviceGrant, FolderGrant, Locality, StorageGrant};
use crate::files::permissions::AgentFileStatus;
use crate::passwords::access::{
    Grant as PasswordGrant, GrantAction as PasswordAction, Scope as PasswordScope,
};

/// The scopes of the group "holzi bedienen" (`src/lib/actions/scopes.ts`): the window manager, the
/// chat and the settings an agent may reach. Files, contacts and passwords are their own groups.
pub const OPERATE_SCOPES: [&str; 8] = [
    "wm.layout",
    "wm.navigation",
    "wm.read",
    "chat.read",
    "chat.write",
    "settings.read",
    "settings.device",
    "settings.models",
];

/// How far an agent may act in one scope.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/types/bindings/")]
pub enum OperateLevel {
    /// Only actions that read.
    Read,
    /// Reading, changing and destroying; the approval mode decides what asks first.
    Write,
}

impl OperateLevel {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Read => "read",
            Self::Write => "write",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "read" => Some(Self::Read),
            "write" => Some(Self::Write),
            _ => None,
        }
    }
}

/// The scope of the browser tools (spec 065); it belongs to the group `browser`, not to
/// "holzi bedienen".
pub const BROWSER_SCOPE: &str = "browser";

/// How far an agent may act on one website (spec 065 FR-005, FR-008).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/types/bindings/")]
pub enum SiteLevel {
    /// Open, read, follow links, scroll, screenshots.
    Read,
    /// Also type, choose, click; submitting still asks.
    Operate,
    /// Never open: the user's blocklist (FR-047). Only a listed website can be blocked, never
    /// "all other websites" (`BrowserRights::all` is `None` for nothing).
    Blocked,
}

impl SiteLevel {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Read => "read",
            Self::Operate => "operate",
            Self::Blocked => "blocked",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "read" => Some(Self::Read),
            "operate" => Some(Self::Operate),
            "blocked" => Some(Self::Blocked),
            _ => None,
        }
    }

    /// Whether a website at this level may be opened at all.
    pub fn opens(self) -> bool {
        !matches!(self, Self::Blocked)
    }
}

/// What an agent's browser does with a request a page sends to a website, without asking
/// (spec 065 FR-044): the user's "Für diese Website merken".
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/types/bindings/")]
pub enum RequestRule {
    /// Send it.
    Allow,
    /// Refuse it.
    Deny,
}

impl RequestRule {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Allow => "allow",
            Self::Deny => "deny",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "allow" => Some(Self::Allow),
            "deny" => Some(Self::Deny),
            _ => None,
        }
    }
}

/// One website the user listed: allowed for reading or operating, or blocked (FR-047).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/types/bindings/")]
pub struct SiteGrant {
    pub level: SiteLevel,
    /// "Für diese Website merken": filling credentials asks no more (spec 065 FR-021).
    #[serde(default)]
    pub fill_remembered: bool,
}

/// The rights group `browser` (spec 065 data-model.md). Present means on.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/types/bindings/")]
pub struct BrowserRights {
    /// Handed-over tabs in the user's own browser (US6).
    #[serde(default)]
    pub user_browser: bool,
    #[serde(default)]
    #[ts(as = "BTreeMap<String, SiteGrant>")]
    pub sites: BTreeMap<Site, SiteGrant>,
    /// Rules for the requests a page sends to a website (FR-044), by the website they go to.
    #[serde(default)]
    #[ts(as = "BTreeMap<String, RequestRule>")]
    pub requests: BTreeMap<Site, RequestRule>,
    /// What the agent may do on every website not listed in `sites` (FR-043); `None` is nothing,
    /// and `Blocked` is no valid level here (`validate`, FR-047). A website of `sites` is an
    /// exception and wins, also as a blocked one; `http` never gets more than reading (R9).
    #[serde(default)]
    #[ts(optional = nullable)]
    pub all: Option<SiteLevel>,
}

impl BrowserRights {
    /// A group switched on from now on: every website may be operated (FR-043). A group stored
    /// before has no level for all websites and keeps `None`.
    pub fn switched_on() -> Self {
        Self {
            all: Some(SiteLevel::Operate),
            ..Self::default()
        }
    }
}

/// The scopes of the group `passwords`: the search and the single secret.
pub const PASSWORD_SCOPES: [&str; 2] = ["passwords.read", "passwords.secret"];

/// What the group `passwords` lets an agent do with the entries in its tags (034 FR-028).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/types/bindings/")]
pub enum PasswordLevel {
    Read,
    /// Takes effect once agents get writing password actions (research R13).
    ReadWrite,
}

impl PasswordLevel {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Read => "read",
            Self::ReadWrite => "read_write",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "read" => Some(Self::Read),
            "read_write" => Some(Self::ReadWrite),
            _ => None,
        }
    }
}

/// The rights group `passwords` (data-model.md). Present means on.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/types/bindings/")]
pub struct PasswordRights {
    pub level: PasswordLevel,
    /// The tag names whose entries it reaches; `None` reaches every entry.
    #[serde(default)]
    pub tags: Option<Vec<String>>,
    /// The tag a new entry of the agent gets (spec 065 FR-036, like a browser extension's grant in
    /// spec 049 FR-010); with [`PasswordLevel::ReadWrite`] and chosen tags it is one of them.
    #[serde(default)]
    #[ts(optional = nullable)]
    pub default_tag: Option<String>,
}

impl PasswordRights {
    /// The one grant the password manager checks the agent's calls with, like the approval of a
    /// browser client (spec 049).
    pub fn grants(&self) -> Vec<PasswordGrant> {
        let action = match self.level {
            PasswordLevel::Read => PasswordAction::Read,
            PasswordLevel::ReadWrite => PasswordAction::ReadWrite,
        };
        let scope = match &self.tags {
            None => PasswordScope::All,
            Some(tags) => PasswordScope::tags(tags.iter()),
        };
        vec![PasswordGrant::new(action, scope)]
    }
}

/// The rights of one agent. A scope that is missing is "nothing"; a group that is missing is off.
/// `grant_rows.rs` stores them.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/types/bindings/")]
pub struct Rights {
    pub operate: BTreeMap<String, OperateLevel>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub browser: Option<BrowserRights>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub passwords: Option<PasswordRights>,
    /// The extensions whose tools the agent sees (US4); a new extension is not among them.
    #[serde(default, skip_serializing_if = "BTreeSet::is_empty")]
    #[ts(as = "Option<Vec<String>>", optional)]
    pub extensions: BTreeSet<Uuid>,
    /// "Alle Erweiterungen" (spec 083 plan Q3): every extension while it is on, a new one too.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    #[ts(as = "Option<bool>", optional)]
    pub all_extensions: bool,
    /// "Befehle ausführen": the command tool, where the device has it (US5, FR-012).
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    #[ts(as = "Option<bool>", optional)]
    pub commands: bool,
    /// How an Abo-Agent's own tools run (spec 083 FR-030); a profile without the group asks for
    /// every call.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub delegate: Option<DelegateRights>,
    /// What the agent may do with the address book (spec 083 FR-032); no entry asks the user at the
    /// first access, like before.
    #[serde(default, skip_serializing_if = "ContactsRights::is_empty")]
    #[ts(as = "Option<ContactsRights>", optional)]
    pub contacts: ContactsRights,
    /// What the agent may do with files (spec 083 FR-033); the device is off without an entry, a
    /// storage or an encrypted folder asks the user at the first access.
    #[serde(default, skip_serializing_if = "FilesRights::is_empty")]
    #[ts(as = "Option<FilesRights>", optional)]
    pub files: FilesRights,
}

/// The right for one encrypted folder: how far, and whether cloud models reach it too (spec 048
/// FR-030a).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/types/bindings/")]
pub struct FolderRight {
    pub level: AccessLevel,
    /// Cloud models reach the folder too; without it only models on this device do.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    #[ts(as = "Option<bool>", optional)]
    pub cloud: bool,
}

/// The group `files` of a profile (spec 083 FR-033; specs 044, 048, 056, 061): the files of this
/// device, storages, encrypted folders, and managing file shares and sync rules. Files of the
/// device are always this device's, whichever device the profile is read on.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/types/bindings/")]
pub struct FilesRights {
    /// "Dateien des Geräts".
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    #[ts(as = "Option<bool>", optional)]
    pub device: bool,
    /// By storage id (another own device or a received share counts as one).
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    #[ts(as = "Option<BTreeMap<String, AccessLevel>>", optional)]
    pub storages: BTreeMap<String, AccessLevel>,
    /// By `<storage id>/<folder id>`.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    #[ts(as = "Option<BTreeMap<String, FolderRight>>", optional)]
    pub folders: BTreeMap<String, FolderRight>,
    /// "Dateifreigaben verwalten" (spec 056 FR-045).
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    #[ts(as = "Option<bool>", optional)]
    pub file_shares: bool,
    /// "Sync-Regeln verwalten" (spec 061 FR-035).
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    #[ts(as = "Option<bool>", optional)]
    pub sync_rules: bool,
}

fn storage_grant(level: AccessLevel) -> StorageGrant {
    match level {
        AccessLevel::Read => StorageGrant::Read,
        AccessLevel::ReadWrite => StorageGrant::ReadWrite,
        AccessLevel::Denied => StorageGrant::Denied,
    }
}

impl FilesRights {
    pub fn is_empty(&self) -> bool {
        *self == Self::default()
    }

    /// The grants the file checks read ([`crate::files::access::check`]). The profile is the only
    /// authority: the device is granted or not, for the chat's agent as for any other.
    pub fn grants(&self) -> AgentGrants {
        AgentGrants {
            device: Some(if self.device {
                DeviceGrant::Granted
            } else {
                DeviceGrant::Denied
            }),
            storages: self
                .storages
                .iter()
                .map(|(id, level)| (id.clone(), storage_grant(*level)))
                .collect(),
            folders: self
                .folders
                .iter()
                .map(|(target, right)| {
                    (
                        target.clone(),
                        FolderGrant {
                            grant: storage_grant(right.level),
                            reach: if right.cloud {
                                Locality::Cloud
                            } else {
                                Locality::Local
                            },
                        },
                    )
                })
                .collect(),
            file_shares: self.file_shares,
            sync_rules: self.sync_rules,
            profile: None,
        }
    }
}

/// How far an agent reaches into the address book, a storage or an encrypted folder (spec 051
/// FR-030, spec 044 FR-031).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/types/bindings/")]
pub enum AccessLevel {
    Read,
    ReadWrite,
    /// The user's refusal: for contacts it hides a group from a grant for all contacts, and for all
    /// contacts it takes everything away.
    Denied,
}

impl AccessLevel {
    /// The level of the user's answer to a question; a status that is none of them is a refusal.
    pub fn of(status: AgentFileStatus) -> Self {
        match status {
            AgentFileStatus::Read => Self::Read,
            AgentFileStatus::ReadWrite => Self::ReadWrite,
            _ => Self::Denied,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Read => "read",
            Self::ReadWrite => "readWrite",
            Self::Denied => "denied",
        }
    }

    /// An unknown level is a refusal: skipping it would give more than the user set.
    pub fn parse(value: &str) -> Self {
        match value {
            "read" => Self::Read,
            "readWrite" => Self::ReadWrite,
            _ => Self::Denied,
        }
    }

    pub fn status(self) -> AgentFileStatus {
        match self {
            Self::Read => AgentFileStatus::Read,
            Self::ReadWrite => AgentFileStatus::ReadWrite,
            Self::Denied => AgentFileStatus::Denied,
        }
    }
}

/// The group `contacts` of a profile: a level for all contacts and levels for single groups of the
/// address book (spec 083 FR-032). Without an entry the agent is asked at its first access.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/types/bindings/")]
pub struct ContactsRights {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub all: Option<AccessLevel>,
    /// By the id of the contact group.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    #[ts(as = "Option<BTreeMap<String, AccessLevel>>", optional)]
    pub groups: BTreeMap<String, AccessLevel>,
}

impl ContactsRights {
    pub fn is_empty(&self) -> bool {
        self.all.is_none() && self.groups.is_empty()
    }

    /// The entries as the address book's checks read them: target `*` or `group:<id>`, and status.
    pub fn rows(&self) -> Vec<(String, AgentFileStatus)> {
        self.all
            .iter()
            .map(|level| ("*".to_owned(), level.status()))
            .chain(
                self.groups
                    .iter()
                    .map(|(id, level)| (format!("group:{id}"), level.status())),
            )
            .collect()
    }
}

/// How an Abo-Agent's own tools run in the chat, a right of the agent profile of its model (spec 083
/// FR-030; specs 009, 032): the autonomy mode and what the permissive mode still refuses. It says
/// nothing about holzi's tools, which the other groups limit. Only the model of an Abo-Agent's
/// provider uses it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/types/bindings/")]
pub struct DelegateRights {
    pub autonomy: AutonomyMode,
    /// What `GatedPermissive` still refuses (specs 009).
    #[serde(default)]
    pub deny: BTreeSet<DenyCategory>,
}

impl DelegateRights {
    /// What a model with no such group gets: every tool call asks (spec 009 `Standard`), nothing
    /// is allowed on its own.
    pub fn asking() -> Self {
        Self {
            autonomy: AutonomyMode::Standard,
            deny: BTreeSet::new(),
        }
    }
}

/// What a new agent may do until the user changes it (FR-009): read in every scope of
/// "holzi bedienen", nothing else.
pub fn default_rights() -> Rights {
    Rights {
        operate: OPERATE_SCOPES
            .iter()
            .map(|scope| ((*scope).to_owned(), OperateLevel::Read))
            .collect(),
        browser: None,
        passwords: None,
        extensions: BTreeSet::new(),
        all_extensions: false,
        commands: false,
        // The device-wide setting of before spec 083 PR J3 defaulted to "ungated": "Standard" keeps it.
        delegate: Some(DelegateRights {
            autonomy: AutonomyMode::Ungated,
            deny: BTreeSet::new(),
        }),
        contacts: ContactsRights::default(),
        files: FilesRights::default(),
    }
}

/// Rights the window sent that holzi does not store.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum InvalidRights {
    /// `guardrails` or a scope outside "holzi bedienen" (FR-016).
    #[error("the scope {0} cannot be granted")]
    Scope(String),
    /// A website over plain http can only be read (spec 065 research R9).
    #[error("{0} is not encrypted and can only be read")]
    PlainSite(String),
    /// A tag scope of the passwords needs at least one tag.
    #[error("a tag scope needs at least one tag")]
    EmptyTags,
    /// Writing within chosen tags needs one of them for new entries.
    #[error("new entries need one of the chosen tags")]
    DefaultTag,
    /// Only a listed website can be blocked (FR-047).
    #[error("the level for all websites cannot be blocked")]
    BlockedAll,
}

/// Refuses a scope no agent may get, the guardrails above all (FR-016).
pub fn validate(rights: &Rights) -> Result<(), InvalidRights> {
    if let Some(scope) = rights
        .operate
        .keys()
        .find(|scope| !OPERATE_SCOPES.contains(&scope.as_str()))
    {
        return Err(InvalidRights::Scope(scope.clone()));
    }
    if rights.passwords.as_ref().is_some_and(|passwords| {
        passwords
            .tags
            .as_ref()
            .is_some_and(|tags| tags.iter().all(|tag| tag.trim().is_empty()))
    }) {
        return Err(InvalidRights::EmptyTags);
    }
    if let Some(PasswordRights {
        level: PasswordLevel::ReadWrite,
        tags: Some(tags),
        default_tag,
    }) = &rights.passwords
    {
        if !default_tag.as_ref().is_some_and(|tag| tags.contains(tag)) {
            return Err(InvalidRights::DefaultTag);
        }
    }
    if rights
        .browser
        .as_ref()
        .is_some_and(|browser| browser.all == Some(SiteLevel::Blocked))
    {
        return Err(InvalidRights::BlockedAll);
    }
    let sites = rights.browser.iter().flat_map(|browser| &browser.sites);
    for (site, grant) in sites {
        if site.is_plain_http() && grant.level == SiteLevel::Operate {
            return Err(InvalidRights::PlainSite(site.to_string()));
        }
    }
    Ok(())
}

/// Which data scopes the rows of an agent in `agent_file_permissions` open (research R3, FR-020):
/// a scope's tools appear once any row covers its kind and action; the service checks the reach of
/// each call as before and asks for a storage without a row. An external agent has no row until
/// the user grants one, not even the device.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct DataRights {
    pub files_read: bool,
    pub files_write: bool,
    pub file_shares: bool,
    pub sync_rules: bool,
    pub contacts_read: bool,
    pub contacts_write: bool,
}

impl DataRights {
    pub fn of(files: &AgentGrants, contacts: &[(String, AgentFileStatus)]) -> Self {
        let device = files.device == Some(DeviceGrant::Granted);
        let stored = files
            .storages
            .values()
            .chain(files.folders.values().map(|folder| &folder.grant));
        let (mut read, mut write) = (device, device);
        for grant in stored {
            read |= *grant != StorageGrant::Denied;
            write |= *grant == StorageGrant::ReadWrite;
        }
        let contact_grants = grants_from(contacts);
        Self {
            files_read: read,
            files_write: write,
            file_shares: files.file_shares,
            sync_rules: files.sync_rules,
            contacts_read: !contact_grants.is_empty(),
            contacts_write: contact_grants
                .iter()
                .any(|grant| grant.action == GrantAction::ReadWrite),
        }
    }

    /// Whether the data scope `scope` is open; `None` for a scope that is no data scope.
    fn covers(self, scope: &str) -> Option<bool> {
        match scope {
            "files.read" => Some(self.files_read),
            "files.write" => Some(self.files_write),
            "files.shares" => Some(self.file_shares),
            "files.syncRules" => Some(self.sync_rules),
            "contacts.read" => Some(self.contacts_read),
            "contacts.write" => Some(self.contacts_write),
            _ => None,
        }
    }
}

/// What decides an agent's tools besides its stored rights, read fresh for each list and call.
#[derive(Debug, Clone, Default)]
pub struct Reach {
    pub data: DataRights,
    /// The native actions whose executor serves external agents.
    pub native: HashSet<&'static str>,
}

/// Whether `rights` cover `def`. Whether a covered call asks first is the approval mode's matter
/// (`prompts::call_gate`).
fn covers(rights: &Rights, reach: &Reach, def: &AgentActionDef) -> bool {
    // A native executor that serves only the built-in agent would refuse the call.
    if def.runner == Some(ActionRunner::Native) && !reach.native.contains(def.action_id.as_str()) {
        return false;
    }
    covers_scope(rights, reach.data, def)
}

/// Whether the rights of an agent profile cover `def` for the local agent (spec 083 FR-017): the
/// same decision as for an external agent, except that the native executors all serve it, and the
/// files and contacts (`files.*`, `contacts.*`, `files.shares`) keep the gate they had, which their
/// executors apply to each call (`agent_file_permissions` for `builtin`, 044 Stage 1).
pub fn covers_local(rights: &Rights, def: &AgentActionDef) -> bool {
    if def.scope == GUARDRAILS_SCOPE {
        return false;
    }
    if def.scope.starts_with("files.") || def.scope.starts_with("contacts.") {
        return true;
    }
    covers_scope(rights, DataRights::default(), def)
}

/// The decision by scope, shared by [`covers`] and [`covers_local`].
fn covers_scope(rights: &Rights, data: DataRights, def: &AgentActionDef) -> bool {
    if def.scope == GUARDRAILS_SCOPE {
        return false;
    }
    // The browser tools check the website of each call themselves (spec 065 research R8).
    if def.scope == BROWSER_SCOPE {
        return rights.browser.is_some();
    }
    // The password manager checks each entry against the group's tags (034 FR-029).
    if PASSWORD_SCOPES.contains(&def.scope.as_str()) {
        return rights.passwords.is_some();
    }
    // Files and contacts: their executors check each call against the same rows.
    if let Some(open) = data.covers(&def.scope) {
        return open && def.runner == Some(ActionRunner::Native);
    }
    match rights.operate.get(&def.scope) {
        None => false,
        Some(OperateLevel::Read) => def.effect == ActionEffect::Read,
        Some(OperateLevel::Write) => true,
    }
}

/// The actions of `catalog` an agent with `rights` sees (FR-012, FR-013).
pub fn offers<'a>(
    rights: &Rights,
    reach: &Reach,
    catalog: &'a [AgentActionDef],
) -> Vec<&'a AgentActionDef> {
    catalog
        .iter()
        .filter(|def| covers(rights, reach, def))
        .collect()
}

/// The action behind `tool_name`, if the agent may call it (FR-014); checked again on every call,
/// independent of the list the agent saw.
pub fn check<'a>(
    rights: &Rights,
    reach: &Reach,
    catalog: &'a [AgentActionDef],
    tool_name: &str,
) -> Option<&'a AgentActionDef> {
    catalog
        .iter()
        .find(|def| def.tool_name == tool_name)
        .filter(|def| covers(rights, reach, def))
}

/// Whether `rights` cover a tool of an extension (FR-018); holzi's own tools have no origin and
/// are never covered here.
pub fn covers_extension_tool(rights: &Rights, tool: &dyn Tool) -> bool {
    tool.origin()
        .and_then(|origin| Uuid::parse_str(&origin.extension_id).ok())
        .is_some_and(|id| rights.all_extensions || rights.extensions.contains(&id))
}

/// The extension tools an agent with `rights` sees (US4).
pub fn extension_offers<'a>(rights: &Rights, tools: &'a [Arc<dyn Tool>]) -> Vec<&'a Arc<dyn Tool>> {
    tools
        .iter()
        .filter(|tool| covers_extension_tool(rights, tool.as_ref()))
        .collect()
}

/// The extension tool behind `tool_name`, if the agent may call it; checked again on every call.
pub fn check_extension_tool<'a>(
    rights: &Rights,
    tools: &'a [Arc<dyn Tool>],
    tool_name: &str,
) -> Option<&'a Arc<dyn Tool>> {
    tools
        .iter()
        .find(|tool| tool.name() == tool_name)
        .filter(|tool| covers_extension_tool(rights, tool.as_ref()))
}
