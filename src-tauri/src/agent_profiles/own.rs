//! The interim write path of the per-agent rights inputs (spec 083 research R8): the agent page,
//! the registration, the OAuth consent, the delegate registration and what a prompt remembers all
//! still hand `registry::save_rights` the whole rights of one agent. Until the profile pages
//! replace those inputs (PR D), each such write lands in a profile of its own for that agent, so no
//! other agent's rights change. Rights equal to what the agent already has change nothing; rights
//! equal to "Standard" leave the agent unassigned.
//!
//! Removed with `registry::save_rights` in PR E (spec 083 T066).

use haex_crdt::rusqlite::params;
use haex_crdt::CrdtTransaction;
use uuid::Uuid;

use super::resolve::{normalized, rights_of, standard_rights};
use super::{own_profile_id, store, Subject};
use crate::mcp_server::grants::Rights;

/// The shown name of an agent's own profile: what the agent calls itself.
fn agent_name(tx: &mut CrdtTransaction<'_>, agent: Uuid) -> haex_crdt::Result<String> {
    let name: Option<String> = tx.query_row(
        "SELECT client_name FROM haex_mcp_agents WHERE id = ?1",
        params![agent.to_string()],
        |row| row.get(0),
    )?;
    Ok(name
        .map(|name| name.trim().to_owned())
        .filter(|name| !name.is_empty())
        .unwrap_or_else(|| "Agent".to_owned()))
}

/// Gives `agent` exactly `rights`.
pub fn set_own_rights(
    tx: &mut CrdtTransaction<'_>,
    device: Uuid,
    agent: Uuid,
    rights: &Rights,
) -> haex_crdt::Result<()> {
    let subject = Subject::Agent(agent);
    let wanted = normalized(rights);
    if wanted == normalized(&rights_of(tx, &subject)?) {
        return Ok(());
    }
    if wanted == normalized(&standard_rights(tx)?) {
        store::unassign(tx, &subject)?;
        return forget_own_profile(tx, device, agent);
    }
    let own = own_profile_id(device, agent);
    if store::profile(tx, own)?.is_none() {
        let name = agent_name(tx, agent)?;
        store::create_profile(tx, own, &name)?;
    }
    store::set_rights(tx, own, &wanted)?;
    store::assign(tx, &subject, own)
}

/// Removes the profile `agent` got of its own, unless other subjects have been put into it since.
pub fn forget_own_profile(
    tx: &mut CrdtTransaction<'_>,
    device: Uuid,
    agent: Uuid,
) -> haex_crdt::Result<()> {
    let own = own_profile_id(device, agent);
    if store::profile(tx, own)?.is_some() && store::members(tx, own)?.is_empty() {
        store::delete_profile(tx, own)?;
    }
    Ok(())
}

/// Forgets everything profiles hold about a removed agent: its assignment and its own profile.
pub fn forget_agent(
    tx: &mut CrdtTransaction<'_>,
    device: Uuid,
    agent: Uuid,
) -> haex_crdt::Result<()> {
    store::unassign(tx, &Subject::Agent(agent))?;
    forget_own_profile(tx, device, agent)
}
