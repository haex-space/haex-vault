//! The rows of agents on the HTTP way (spec 021 T094, data-model.md): `transport = 'http'`, their
//! `oauth_client_id` from dynamic registration, and the refresh token as the credential
//! (`token_hash`), which rotates on every use.

use haex_crdt::rusqlite::params;
use haex_crdt::{CrdtTransaction, Result};
use uuid::Uuid;

use super::registry::{self, Agent, AgentKind, NewAgent, Transport};
use crate::storage::query::Query;

/// Turns a client's name into a slug (`[a-z0-9-]{1,32}`); `http-agent` when nothing is left.
pub fn slug_of(name: Option<&str>) -> String {
    let mut slug = String::new();
    for c in name
        .unwrap_or_default()
        .chars()
        .flat_map(char::to_lowercase)
    {
        if c.is_ascii_alphanumeric() {
            slug.push(c);
        } else if !slug.is_empty() && !slug.ends_with('-') {
            slug.push('-');
        }
    }
    let slug: String = slug.trim_end_matches('-').chars().take(32).collect();
    let slug = slug.trim_end_matches('-');
    if slug.is_empty() {
        "http-agent".to_owned()
    } else {
        slug.to_owned()
    }
}

/// The HTTP agent of `slug` that is registered or expired, to authorize again in its profile.
pub fn previous(q: &mut impl Query, device: Uuid, slug: &str) -> Result<Option<Agent>> {
    let id: Option<String> = q.query_row(
        "SELECT id FROM haex_mcp_agents WHERE vault_device_uuid = ?1 AND slug = ?2 \
             AND transport = 'http' AND kind = 'external' AND status IN ('registered', 'expired') \
             ORDER BY registered_at DESC LIMIT 1",
        params![device.to_string(), slug],
        |row| row.get(0),
    )?;
    let Some(id) = id.and_then(|id| Uuid::parse_str(&id).ok()) else {
        return Ok(None);
    };
    registry::get(q, device, id)
}

/// What an authorization the user granted writes.
#[derive(Debug, Clone)]
pub struct Authorized {
    pub slug: String,
    pub client_name: String,
    pub client_id: String,
}

/// Registers the agent of an authorization, or authorizes the earlier one of the same slug again,
/// with `refresh_hash` as its credential.
pub fn authorize(
    tx: &mut CrdtTransaction<'_>,
    device: Uuid,
    previous: Option<Uuid>,
    authorized: &Authorized,
    refresh_hash: &str,
    profile: Uuid,
    now: i64,
) -> Result<Option<Agent>> {
    let id = match previous {
        Some(id) => {
            let updated = tx.execute(
                "UPDATE haex_mcp_agents SET status = 'registered', token_hash = ?3, \
                 oauth_client_id = ?4, client_name = ?5, last_seen_at = ?6 \
                 WHERE id = ?1 AND vault_device_uuid = ?2 AND transport = 'http' \
                 AND status IN ('registered', 'expired')",
                params![
                    id.to_string(),
                    device.to_string(),
                    refresh_hash,
                    authorized.client_id,
                    registry::kept_name(&authorized.client_name),
                    now,
                ],
            )?;
            if updated == 0 {
                // Revoked while the dialog was open.
                return Ok(None);
            }
            crate::agent_profiles::store::add_member(
                tx,
                &crate::agent_profiles::Subject::Agent(id),
                profile,
            )?;
            id
        }
        None => {
            let new = NewAgent {
                kind: AgentKind::External,
                slug: authorized.slug.clone(),
                client_name: authorized.client_name.clone(),
                client_version: None,
                transport: Transport::Http,
            };
            let agent = registry::register(tx, device, &new, refresh_hash, profile, now)?;
            tx.execute(
                "UPDATE haex_mcp_agents SET oauth_client_id = ?3 \
                 WHERE id = ?1 AND vault_device_uuid = ?2",
                params![
                    agent.id.to_string(),
                    device.to_string(),
                    authorized.client_id
                ],
            )?;
            agent.id
        }
    };
    registry::get(tx, device, id)
}

/// The client id an HTTP agent was authorized for.
pub fn client_of(q: &mut impl Query, device: Uuid, agent: Uuid) -> Result<Option<String>> {
    Ok(q.query_row(
        "SELECT oauth_client_id FROM haex_mcp_agents WHERE vault_device_uuid = ?1 AND id = ?2 \
         AND transport = 'http'",
        params![device.to_string(), agent.to_string()],
        |row| row.get::<_, Option<String>>(0),
    )?
    .flatten())
}

/// Rotates the refresh token: only the holder of `old_hash` can, once. `false` when it was used.
pub fn rotate(
    tx: &mut CrdtTransaction<'_>,
    device: Uuid,
    agent: Uuid,
    old_hash: &str,
    new_hash: &str,
    now: i64,
) -> Result<bool> {
    let updated = tx.execute(
        "UPDATE haex_mcp_agents SET token_hash = ?4, last_seen_at = ?5 \
         WHERE id = ?1 AND vault_device_uuid = ?2 AND token_hash = ?3 AND status = 'registered'",
        params![
            agent.to_string(),
            device.to_string(),
            old_hash,
            new_hash,
            now
        ],
    )?;
    Ok(updated > 0)
}
