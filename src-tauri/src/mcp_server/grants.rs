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

use crate::agent_browser::sites::Site;
use crate::chat::action_commands::GUARDRAILS_SCOPE;
use crate::chat::tools::action_tool::{ActionEffect, ActionRunner, AgentActionDef};
use crate::chat::tools::Tool;
use crate::contacts::access::GrantAction;
use crate::contacts::agent::grants_from;
use crate::files::access::{AgentGrants, DeviceGrant, StorageGrant};
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
}

impl SiteLevel {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Read => "read",
            Self::Operate => "operate",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "read" => Some(Self::Read),
            "operate" => Some(Self::Operate),
            _ => None,
        }
    }
}

/// One allowed website.
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
    /// "Befehle ausführen": the command tool, where the device has it (US5, FR-012).
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    #[ts(as = "Option<bool>", optional)]
    pub commands: bool,
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
        commands: false,
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
    if def.scope == GUARDRAILS_SCOPE {
        return false;
    }
    // A native executor that serves only the built-in agent would refuse the call.
    if def.runner == Some(ActionRunner::Native) && !reach.native.contains(def.action_id.as_str()) {
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
    if let Some(open) = reach.data.covers(&def.scope) {
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
fn covers_extension_tool(rights: &Rights, tool: &dyn Tool) -> bool {
    tool.origin()
        .and_then(|origin| Uuid::parse_str(&origin.extension_id).ok())
        .is_some_and(|id| rights.extensions.contains(&id))
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
