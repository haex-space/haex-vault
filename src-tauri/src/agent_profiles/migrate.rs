//! The post-open step that keeps every agent's rights when they move into profiles (spec 083
//! FR-024, FR-025, research R7). A migration of the schema cannot do it: a CRDT write needs the
//! HLC, which does not exist while migrations run, so this runs after the vault is open, next to
//! the steps of `storage::maintenance`.
//!
//! Step 1 (this file): for each agent of this device, load its rights from the old per-agent rows
//! (`haex_mcp_agent_grants`). Rights equal to what "Standard" gives now leave it unassigned; any
//! other rights go into a profile named after the agent, with the deterministic id of
//! [`own_profile_id`], and the agent is assigned to it. The old rows stay as they are and are not
//! read afterwards. The marker `agent_profiles.migrated_v1` (a device preference) is written in the
//! same transaction, so the step is all or nothing and runs once; the ids are deterministic, so a
//! second run would write the same rows anyway.

use haex_crdt::rusqlite::params;
use haex_crdt::CrdtTransaction;
use uuid::Uuid;

use super::resolve::{normalized, standard_rights};
use super::{own_profile_id, store, Subject};
use crate::mcp_server::registry;
use crate::storage::preferences::{self, PrefScope};

/// The device preference that marks step 1 as done.
pub const AGENTS_MARKER: &str = "agent_profiles.migrated_v1";

/// Runs step 1 once for `device`. Returns how many agents got a profile of their own.
pub fn migrate_agents(tx: &mut CrdtTransaction<'_>, device: Uuid) -> haex_crdt::Result<usize> {
    let scope = PrefScope::Device(device);
    if preferences::get(tx, scope, AGENTS_MARKER)?.is_some() {
        return Ok(0);
    }
    let agents: Vec<(String, String, String)> = tx.query_map(
        "SELECT id, client_name, status FROM haex_mcp_agents WHERE vault_device_uuid = ?1 \
         ORDER BY registered_at, id",
        params![device.to_string()],
        |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
    )?;
    let standard = normalized(&standard_rights(tx)?);
    let mut created = 0;
    for (id, name, status) in agents {
        let Ok(agent) = Uuid::parse_str(&id) else {
            continue;
        };
        // A blocked agent has no rights rows and is blocked whatever its profile says.
        if status == "blocked" {
            continue;
        }
        let subject = Subject::Agent(agent);
        if store::member_profile(tx, &subject)?.is_some() {
            continue;
        }
        let legacy = normalized(&registry::load_legacy_rights(tx, device, agent)?);
        if legacy == standard {
            continue;
        }
        let profile = own_profile_id(device, agent);
        if store::profile(tx, profile)?.is_none() {
            let shown = name.trim();
            let shown = if shown.is_empty() { "Agent" } else { shown };
            store::create_profile(tx, profile, shown)?;
        }
        store::set_rights(tx, profile, &legacy)?;
        store::assign(tx, &subject, profile)?;
        created += 1;
    }
    preferences::insert_or_update(tx, scope, AGENTS_MARKER, "1")?;
    Ok(created)
}
