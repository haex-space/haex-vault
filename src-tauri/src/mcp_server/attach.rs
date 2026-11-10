//! What every session does once rmcp serves it, on the program way and the HTTP way (spec 021 US3,
//! US7, FR-017): it joins the open connections, a known agent gets its rights fresh from the
//! database, and a registered agent hears when its tool list changed.

use std::sync::Arc;

use rmcp::service::{Peer, RoleServer};
use tokio::sync::broadcast::error::RecvError;
use uuid::Uuid;

use super::hello::CloseReason;
use super::live::{Link, LiveGuard};
use super::registry::{self, AgentStatus};
use super::server::{Core, Notice};
use super::session::{Session, SessionAgent};

/// Tables whose changes can change an agent's profile or rights, including changes received by sync.
const AGENT_PROFILE_TABLES: [&str; 3] = [
    "haex_agent_profiles",
    "haex_agent_profile_grants",
    "haex_agent_profile_members",
];
/// The active profile is one preference among many unrelated rows in this table.
const ACTIVE_PROFILE_PREFERENCE_TABLE: &str = "preferences";

/// Adds the session to the open connections. A known agent is installed only after a database
/// refresh serialized with rights, mode and revocation changes: otherwise a management update can
/// commit before the connection is in `live`, miss it, and leave stale rights. A revoked agent
/// ends the link.
pub(super) async fn join(
    core: &Arc<Core>,
    session: &Session,
    peer: &Peer<RoleServer>,
    link: &Link,
    known: Option<Uuid>,
    delegate_thread: Option<Uuid>,
) -> (
    LiveGuard,
    tokio::sync::broadcast::Receiver<Arc<Vec<String>>>,
) {
    // Keep profile changes published during the initial rights read for the watcher.
    let tables = core.vault_changes.subscribe();
    let _agent_guard = match known {
        Some(id) => Some(core.live.synchronize(id).await),
        None => None,
    };
    let live = core.live.add(session.clone(), peer.clone());
    let Some(id) = known else {
        return (live, tables);
    };
    let device = core.device;
    match core
        .db
        .read(move |q| {
            let Some(agent) = registry::get(q, device, id)? else {
                return Ok(None);
            };
            if agent.status != AgentStatus::Registered {
                return Ok(None);
            }
            let rights = registry::load_rights(q, device, id)?;
            let active_profile = crate::agent_profiles::store::active(q)?;
            Ok(Some(SessionAgent {
                id,
                rights,
                active_profile,
                mode: agent.mode,
                client_name: agent.client_name,
                delegate_thread,
            }))
        })
        .await
    {
        Ok(Some(agent)) => {
            session.set_agent(agent);
            let _ = peer.notify_tool_list_changed().await;
        }
        Ok(None) => link.end(CloseReason::Revoked),
        Err(error) => {
            log::warn!("mcp server: could not refresh an agent's rights: {error}");
            link.end(CloseReason::Disabled);
        }
    }
    (live, tables)
}

/// Tells a registered agent when the window pushed a new action list (FR-017), the extension tools
/// changed: one installed, removed, turned off, or its tool settings changed (US4), the file and
/// contact permissions changed (US5), or its profile changed. Profile changes re-resolve rights
/// from the database so sync updates take effect for live sessions too.
pub(super) async fn tell_catalog_changes(
    core: Arc<Core>,
    session: Session,
    peer: Peer<RoleServer>,
    mut tables: tokio::sync::broadcast::Receiver<Arc<Vec<String>>>,
) {
    let mut actions = core.catalog.subscribe();
    let mut extension_tools = core.extension_tools.subscribe();
    let mut active_profile = session.agent().map(|agent| agent.active_profile);
    loop {
        let (changed, refresh_profile_rights) = tokio::select! {
            changed = actions.changed() => (changed.is_ok(), false),
            changed = extension_tools.changed() => (changed.is_ok(), false),
            received = tables.recv() => match received {
                Ok(tables) => {
                    let profile_tables_changed = tables.iter().any(|table| AGENT_PROFILE_TABLES.contains(&table.as_str()));
                    let active_preference_changed = if tables.iter().any(|table| table == ACTIVE_PROFILE_PREFERENCE_TABLE) {
                        match core.db.read(|q| crate::agent_profiles::store::active(q)).await {
                            Ok(active) => {
                                let previous = active_profile
                                    .or_else(|| session.agent().map(|agent| agent.active_profile));
                                let changed = previous.is_none_or(|previous| previous != active);
                                active_profile = Some(active);
                                changed
                            }
                            Err(error) => {
                                log::warn!("mcp server: could not inspect the active profile preference: {error}");
                                true
                            }
                        }
                    } else {
                        false
                    };
                    let profile_changed = profile_tables_changed || active_preference_changed;
                    if !profile_changed {
                        continue;
                    }
                    (true, profile_changed)
                }
                // We do not know which tables were skipped, so refresh rights as well as the list.
                Err(RecvError::Lagged(_)) => {
                    active_profile = None;
                    (true, true)
                }
                Err(RecvError::Closed) => (false, false),
            },
        };
        if !changed {
            break;
        }
        if refresh_profile_rights {
            if let Some(agent) = session.agent() {
                let _agent_guard = core.live.synchronize(agent.id).await;
                let device = core.device;
                match core
                    .db
                    .read(move |q| {
                        Ok((
                            registry::load_rights(q, device, agent.id)?,
                            crate::agent_profiles::store::active(q)?,
                        ))
                    })
                    .await
                {
                    Ok((rights, active)) => {
                        let lost_browser =
                            agent.rights.browser.is_some() && rights.browser.is_none();
                        active_profile = Some(active);
                        session.update_agent(|known| {
                            known.rights = rights;
                            known.active_profile = active;
                        });
                        if lost_browser {
                            (core.notify)(Notice::BrowserRightLost(agent.id));
                        }
                    }
                    Err(error) => {
                        log::warn!(
                            "mcp server: could not refresh an agent's profile rights: {error}"
                        );
                        session.link().end(CloseReason::Disabled);
                        break;
                    }
                }
            }
        }
        if session.agent().is_some() && peer.notify_tool_list_changed().await.is_err() {
            break;
        }
    }
}
