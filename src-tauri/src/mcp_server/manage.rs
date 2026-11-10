//! What the user changes about a registered agent (spec 021 US7, FR-007, FR-026, FR-027,
//! data-model.md "Zustände"): rights and approval mode, revocation, lifting a block, and the expiry
//! after 90 days without a connection. Only the window reaches these, through the guardrail actions
//! `settings.agents.mcp.*`; the running server learns the change through [`super::live`].

use haex_crdt::rusqlite::params;
use haex_crdt::{CrdtTransaction, Result};
use serde::Serialize;
use ts_rs::TS;
use uuid::Uuid;

use super::grants::Rights;
use super::registry::{self, Agent, AgentKind, AgentStatus, Transport};
use crate::storage::query::Query;

/// A registration ends after this long without a connection (FR-007).
pub const EXPIRES_AFTER_MILLIS: i64 = 90 * 24 * 60 * 60 * 1000;

/// One agent as the settings show it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/types/bindings/")]
pub struct AgentView {
    pub id: String,
    pub kind: AgentKind,
    pub slug: String,
    /// The agent's own claim.
    pub client_name: String,
    pub client_version: Option<String>,
    pub transport: Transport,
    pub status: AgentStatus,
    /// Unix milliseconds.
    #[ts(type = "number")]
    pub registered_at: i64,
    #[ts(type = "number | null")]
    pub last_seen_at: Option<i64>,
    pub connected: bool,
    /// The agent profile the agent is in (spec 083); "Standard" when it has none.
    pub profile_id: String,
    pub profile_name: String,
    /// What the profile gives, for the page to show read only.
    pub rights: Rights,
}

impl AgentView {
    pub fn new(agent: Agent, rights: Rights, profile: (Uuid, String), connected: bool) -> Self {
        Self {
            id: agent.id.to_string(),
            kind: agent.kind,
            slug: agent.slug,
            client_name: agent.client_name,
            client_version: agent.client_version,
            transport: agent.transport,
            status: agent.status,
            registered_at: agent.registered_at,
            last_seen_at: agent.last_seen_at,
            connected,
            profile_id: profile.0.to_string(),
            profile_name: profile.1,
            rights,
        }
    }
}

/// Whether `agent` may no longer connect with its credential. ponytail: expiry is checked only
/// when the agent connects, with no job in the background (research R9); the list shows a stale
/// registration as registered until then.
pub fn is_expired(agent: &Agent, now: i64) -> bool {
    match agent.status {
        AgentStatus::Expired => true,
        AgentStatus::Registered => {
            agent
                .last_seen_at
                .unwrap_or(agent.registered_at)
                .saturating_add(EXPIRES_AFTER_MILLIS)
                < now
        }
        AgentStatus::Blocked => false,
    }
}

/// `registered → expired`: the credential stops working; the row keeps it, so the next connection
/// with it finds the old rights to prefill the dialog.
pub fn expire(tx: &mut CrdtTransaction<'_>, device: Uuid, id: Uuid) -> Result<()> {
    tx.execute(
        "UPDATE haex_mcp_agents SET status = 'expired' \
         WHERE id = ?1 AND vault_device_uuid = ?2 AND status = 'registered'",
        params![id.to_string(), device.to_string()],
    )?;
    Ok(())
}

/// What a renewed registration brings along.
#[derive(Debug, Clone)]
pub struct Renewal {
    pub client_name: String,
    pub client_version: Option<String>,
}

/// `expired → registered` with a new credential and the agent profile the user chose again.
pub fn renew(
    tx: &mut CrdtTransaction<'_>,
    device: Uuid,
    id: Uuid,
    renewal: &Renewal,
    credential_hash: &str,
    profile: Uuid,
    now: i64,
) -> Result<Option<Agent>> {
    let name = registry::kept_name(&renewal.client_name);
    let version = renewal.client_version.as_deref().map(registry::kept_name);
    let renewed = tx.execute(
        "UPDATE haex_mcp_agents SET status = 'registered', token_hash = ?3, \
         client_name = ?4, client_version = ?5, last_seen_at = ?6 \
         WHERE id = ?1 AND vault_device_uuid = ?2 AND status = 'expired'",
        params![
            id.to_string(),
            device.to_string(),
            credential_hash,
            name,
            version,
            now,
        ],
    )?;
    if renewed == 0 {
        return Ok(None);
    }
    // The agent keeps its memberships (FR-014); this adds the profile the user chose to join, if any.
    crate::agent_profiles::store::add_member(
        tx,
        &crate::agent_profiles::Subject::Agent(id),
        profile,
    )?;
    registry::get(tx, device, id)
}

/// Forgets the agent (FR-027): its row and its assignment to a profile; its log stays until
/// it runs out. `false` when no agent has the id.
pub fn revoke(tx: &mut CrdtTransaction<'_>, device: Uuid, id: Uuid) -> Result<bool> {
    let (id, device) = (id.to_string(), device.to_string());
    let removed = tx.execute(
        "DELETE FROM haex_mcp_agents WHERE vault_device_uuid = ?1 AND id = ?2",
        params![&device, &id],
    )?;
    if removed == 0 {
        return Ok(false);
    }
    // Spec 083: the agent's memberships in agent profiles go with it; the profiles stay.
    if let Ok(agent) = Uuid::parse_str(&id) {
        crate::agent_profiles::store::remove_all_memberships(
            tx,
            &crate::agent_profiles::Subject::Agent(agent),
        )?;
    }
    Ok(true)
}

/// Lifts a block and returns its transport and slug to clear from the server's session-local
/// declines; `None` when no blocked agent has the id.
pub fn unblock(
    tx: &mut CrdtTransaction<'_>,
    device: Uuid,
    id: Uuid,
) -> Result<Option<(String, Option<Transport>)>> {
    let blocked = tx.query_row(
        "SELECT slug, transport FROM haex_mcp_agents \
         WHERE vault_device_uuid = ?1 AND id = ?2 AND status = 'blocked'",
        params![device.to_string(), id.to_string()],
        |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?)),
    )?;
    let Some((slug, transport)) = blocked else {
        return Ok(None);
    };
    crate::agent_profiles::store::remove_all_memberships(
        tx,
        &crate::agent_profiles::Subject::Agent(id),
    )?;
    tx.execute(
        "DELETE FROM haex_mcp_agents \
         WHERE vault_device_uuid = ?1 AND id = ?2 AND status = 'blocked'",
        params![device.to_string(), id.to_string()],
    )?;
    Ok(Some((slug, Transport::parse(&transport))))
}

/// An agent with what its profile gives and the profile.
pub type AgentWithRights = (Agent, Rights, (Uuid, String));

/// Every agent of `device` with its rights and profile, newest first.
pub fn list_with_rights(q: &mut impl Query, device: Uuid) -> Result<Vec<AgentWithRights>> {
    // A delegate (Abo-Agent) is no agent of its own: its models are listed (spec 083 FR-027).
    registry::list(q, device)?
        .into_iter()
        .filter(|agent| agent.kind == registry::AgentKind::External)
        .map(|agent| {
            let rights = registry::load_rights(q, device, agent.id)?;
            let profile = crate::agent_profiles::resolve::profile_of(
                q,
                &crate::agent_profiles::Subject::Agent(agent.id),
            )?;
            Ok((agent, rights, profile))
        })
        .collect()
}
