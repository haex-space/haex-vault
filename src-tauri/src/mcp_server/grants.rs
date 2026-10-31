//! What a registered agent may do (spec 021 research R3, FR-008, FR-012 to FR-016). One function
//! decides the tool list and every call, so a tool an agent does not see is also a tool it cannot
//! call. The rights group "holzi bedienen" (`operate`) has per scope of the action catalog nothing,
//! reading, or reading and changing; the group `browser` (spec 065) opens the browser tools and
//! names the websites an agent may read or operate; the group `extension` names the extensions
//! whose tools it sees (US4, FR-018). Files and contacts follow the agent's rows in
//! `agent_file_permissions` ([`DataRights`], US5, FR-020), and a native action is offered only when
//! its executor serves external agents. The scope `guardrails` is never a right. Later groups
//! (passwords, commands) add fields that are off when missing (FR-008).

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

/// The rights of one agent. A scope that is missing is "nothing"; a group that is missing is off.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/types/bindings/")]
pub struct Rights {
    pub operate: BTreeMap<String, OperateLevel>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub browser: Option<BrowserRights>,
    /// The extensions whose tools the agent sees (US4); a new extension is not among them.
    #[serde(default, skip_serializing_if = "BTreeSet::is_empty")]
    #[ts(as = "Option<Vec<String>>", optional)]
    pub extensions: BTreeSet<Uuid>,
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
        extensions: BTreeSet::new(),
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

/// One row of `haex_mcp_agent_grants` in the group `browser` (spec 065 data-model.md): key, level
/// and scope tags.
pub type BrowserRow = (String, String, Option<String>);

/// The key of the row that turns the group on.
const BROWSER_ON: &str = "";
/// The key of the row that allows the user's own browser.
const USER_BROWSER: &str = "user-browser";
/// The level of the two switch rows.
const ON: &str = "on";
/// The scope tags of a website whose credential fill is remembered.
const FILL_TAGS: &str = r#"["fill"]"#;

/// The rows the group `browser` is stored as.
pub fn browser_rows(browser: &BrowserRights) -> Vec<BrowserRow> {
    let mut rows = vec![(BROWSER_ON.to_owned(), ON.to_owned(), None)];
    if browser.user_browser {
        rows.push((USER_BROWSER.to_owned(), ON.to_owned(), None));
    }
    rows.extend(browser.sites.iter().map(|(site, grant)| {
        let tags = grant.fill_remembered.then(|| FILL_TAGS.to_owned());
        (site.to_string(), grant.level.as_str().to_owned(), tags)
    }));
    rows
}

/// The group from its rows; `None` without the row that turns it on (021 FR-008). A row this build
/// cannot read counts as no right.
pub fn browser_from_rows(rows: &[BrowserRow]) -> Option<BrowserRights> {
    rows.iter()
        .find(|(key, level, _)| key == BROWSER_ON && level == ON)?;
    let mut browser = BrowserRights::default();
    for (key, level, tags) in rows {
        if key == BROWSER_ON {
            continue;
        }
        if key == USER_BROWSER {
            browser.user_browser = level == ON;
            continue;
        }
        let (Ok(site), Some(level)) = (Site::parse(key), SiteLevel::parse(level)) else {
            continue;
        };
        let fill_remembered = level == SiteLevel::Operate && tags.as_deref() == Some(FILL_TAGS);
        browser.sites.insert(
            site,
            SiteGrant {
                level,
                fill_remembered,
            },
        );
    }
    Some(browser)
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
