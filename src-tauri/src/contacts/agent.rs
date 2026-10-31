//! Contacts for agents (spec 051 US6, FR-030, FR-031, research R15): the built-in agent and each
//! external agent (spec 021 US5) read and, with a grant, change contacts through the service like
//! every caller. Their grants are rows of kind `contacts` in `agent_file_permissions` (target `*` or
//! `group:<id>`) under their own id. Without one that
//! covers the call, holzi asks the user (the question of spec 044): "Einmal erlauben" holds for
//! this call, "Immer erlauben" and "Ablehnen" are kept. A refusal looks like a missing contact.

use std::collections::BTreeSet;

use tokio_util::sync::CancellationToken;

use super::access::{Caller, Grant, GrantAction, Scope};
use super::model::{ContactPatch, Group};
use super::outside::{self, OutsideContact, OutsideSummary};
use super::service::ContactsService;
use crate::error::{HolziError, Result};
use crate::files::agent::prompt::{FilesAgentChoice, FilesAgentWant, PermissionPrompt};
use crate::files::permissions::{self, AgentFileKind, AgentFilePermission, AgentFileStatus};
use crate::vault_gate::VaultDb;

/// Hits of a search when the call names no limit, and the most.
const SEARCH_LIMIT: (u32, u32) = (20, 100);

/// How holzi's own lookup of the group names is written into the service's checks.
const GROUP_LOOKUP: Caller = Caller::Internal {
    feature: "contacts_agent",
};

/// The rows of one agent: target and status.
type Rows = [(String, AgentFileStatus)];

fn action_of(status: AgentFileStatus) -> Option<GrantAction> {
    match status {
        AgentFileStatus::Read => Some(GrantAction::Read),
        AgentFileStatus::ReadWrite => Some(GrantAction::ReadWrite),
        _ => None,
    }
}

fn covers(status: AgentFileStatus, want: GrantAction) -> bool {
    action_of(status)
        .is_some_and(|have| have == GrantAction::ReadWrite || want == GrantAction::Read)
}

fn star(rows: &Rows) -> Option<AgentFileStatus> {
    rows.iter()
        .find(|(target, _)| target == "*")
        .map(|(_, status)| *status)
}

/// The group ids of the denied group rows.
fn denied_groups(rows: &Rows) -> BTreeSet<String> {
    rows.iter()
        .filter(|(_, status)| *status == AgentFileStatus::Denied)
        .filter_map(|(target, _)| target.strip_prefix("group:"))
        .map(str::to_owned)
        .collect()
}

/// The grants of the service from the rows: a denied `*` takes all away; a denied group hides its
/// members from a `*` grant and voids a grant for the same group (as for extensions).
pub fn grants_from(rows: &Rows) -> Vec<Grant> {
    if star(rows) == Some(AgentFileStatus::Denied) {
        return Vec::new();
    }
    let denied = denied_groups(rows);
    rows.iter()
        .filter_map(|(target, status)| {
            let action = action_of(*status)?;
            let scope = match target.strip_prefix("group:") {
                None if target == "*" => Scope::all_except(denied.clone()),
                Some(id) if !denied.contains(id) => Scope::Groups([id.to_owned()].into()),
                _ => return None,
            };
            Some(Grant::new(action, scope))
        })
        .collect()
}

/// Whether holzi asks before a call that wants `want`: not under a denied `*`, not when a row
/// already covers it.
pub fn asks(rows: &Rows, want: GrantAction) -> bool {
    star(rows) != Some(AgentFileStatus::Denied)
        && !rows.iter().any(|(target, status)| {
            covers(*status, want)
                && target
                    .strip_prefix("group:")
                    .is_none_or(|id| !denied_groups(rows).contains(id))
        })
}

/// A refusal of the service as the agent gets it: like a missing contact (FR-031).
fn hidden(error: HolziError) -> HolziError {
    match error {
        HolziError::ContactsForbidden => HolziError::ContactsNotFound,
        other => other,
    }
}

/// An agent's way into the address book.
pub struct ContactsAgent {
    db: VaultDb,
    prompt: PermissionPrompt,
    agent_id: String,
    caller: Caller,
}

impl ContactsAgent {
    pub fn builtin(db: VaultDb, prompt: PermissionPrompt) -> Self {
        Self {
            db,
            prompt,
            agent_id: permissions::BUILTIN_AGENT.to_owned(),
            caller: Caller::BuiltinAgent,
        }
    }

    /// The external agent `agent_id` (spec 021 US5).
    pub fn external(db: VaultDb, prompt: PermissionPrompt, agent_id: &str) -> Self {
        Self {
            db,
            prompt,
            agent_id: agent_id.to_owned(),
            caller: Caller::ExternalAgent {
                id: agent_id.to_owned(),
            },
        }
    }

    fn service(&self) -> ContactsService {
        ContactsService::new(self.db.clone())
    }

    async fn rows(&self) -> Result<Vec<(String, AgentFileStatus)>> {
        let agent = self.agent_id.clone();
        self.db
            .read(move |q| Ok(permissions::contacts_of(q, &agent)?))
            .await
    }

    async fn store(&self, status: AgentFileStatus) -> Result<()> {
        let row = AgentFilePermission {
            agent_id: self.agent_id.clone(),
            kind: AgentFileKind::Contacts,
            target: "*".to_owned(),
            status,
            updated_at: crate::passwords::clock::unix_millis(std::time::SystemTime::now()),
            reach: None,
        };
        self.db
            .write(move |tx| Ok(permissions::set(tx, &row)?))
            .await
    }

    /// The grants for a call that wants `want`, after asking the user when no row covers it. A
    /// write question answered with "Ablehnen" keeps a read grant for all contacts.
    async fn grants(&self, want: GrantAction, cancel: &CancellationToken) -> Result<Vec<Grant>> {
        let rows = self.rows().await?;
        if !asks(&rows, want) {
            return Ok(grants_from(&rows));
        }
        let _turn = self.prompt.turn().await;
        // The call before may have asked the same.
        let rows = self.rows().await?;
        if !asks(&rows, want) {
            return Ok(grants_from(&rows));
        }
        let wants = match want {
            GrantAction::Read => FilesAgentWant::Read,
            GrantAction::ReadWrite => FilesAgentWant::ReadWrite,
        };
        let Some(answer) = self
            .prompt
            .ask_contacts(&self.agent_id, wants, cancel)
            .await
        else {
            return Ok(grants_from(&rows));
        };
        let status = match answer.choice {
            FilesAgentChoice::Read => AgentFileStatus::Read,
            FilesAgentChoice::ReadWrite => AgentFileStatus::ReadWrite,
            FilesAgentChoice::Deny => {
                // A global denial cannot be stored alongside a group read grant: the row model
                // would make that denial hide the existing read grant as well. Keep that grant
                // intact; with no grant at all, a denied star still remembers the refusal.
                let has_grant = rows.iter().any(|(_, status)| action_of(*status).is_some());
                if star(&rows).is_none() && !has_grant {
                    self.store(AgentFileStatus::Denied).await?;
                }
                return Ok(grants_from(&self.rows().await?));
            }
        };
        if answer.remember {
            self.store(status).await?;
            return Ok(grants_from(&self.rows().await?));
        }
        let mut grants = grants_from(&rows);
        if let Some(action) = action_of(status) {
            grants.push(Grant::new(action, Scope::all_except(denied_groups(&rows))));
        }
        Ok(grants)
    }

    async fn groups(&self) -> Result<Vec<Group>> {
        self.service().groups(&GROUP_LOOKUP).await
    }

    /// `contacts.search`: the contacts in scope that match `query`.
    pub async fn search(
        &self,
        query: &str,
        limit: Option<u32>,
        cancel: &CancellationToken,
    ) -> Result<Vec<OutsideSummary>> {
        let grants = self.grants(GrantAction::Read, cancel).await?;
        let headers = self
            .service()
            .search(&self.caller, &grants, query)
            .await
            .map_err(hidden)?;
        let groups = self.groups().await?;
        let limit = limit.unwrap_or(SEARCH_LIMIT.0).min(SEARCH_LIMIT.1);
        Ok(headers
            .into_iter()
            .take(usize::try_from(limit).unwrap_or(usize::MAX))
            .map(|header| outside::summary(header, &groups))
            .collect())
    }

    /// `contacts.get`: one contact.
    pub async fn get(&self, id: &str, cancel: &CancellationToken) -> Result<OutsideContact> {
        let grants = self.grants(GrantAction::Read, cancel).await?;
        self.service()
            .get(&self.caller, &grants, id)
            .await
            .map(outside::contact)
            .map_err(hidden)
    }

    /// `contacts.update`: changes head fields and values, never the groups. It reaches every own
    /// device and the phones like a change in the window.
    pub async fn update(
        &self,
        id: &str,
        patch: ContactPatch,
        cancel: &CancellationToken,
    ) -> Result<OutsideContact> {
        if patch.groups.is_some() {
            return Err(HolziError::ContactsInvalid {
                reason: "agents do not change groups".to_owned(),
            });
        }
        let grants = self.grants(GrantAction::ReadWrite, cancel).await?;
        self.service()
            .update(&self.caller, &grants, id, patch)
            .await
            .map(outside::contact)
            .map_err(hidden)
    }
}

#[cfg(test)]
#[path = "agent_tests.rs"]
mod tests;
