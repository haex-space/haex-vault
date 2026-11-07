//! The update of the Abo-Agents (spec 083 PR J, FR-031): before it a delegate (Claude Code, Codex)
//! was an agent with memberships of its own; now its models are the subjects. The memberships of a
//! delegate's row become memberships of every model of its provider, and the row's memberships are
//! deleted by an ordinary delete, so the other devices converge and nothing a profile limited is
//! given wider rights: what the delegate was in, its models are in. A delegate whose provider has
//! no models yet keeps its memberships until the next open, when they can be converted.

use haex_crdt::rusqlite::params;
use haex_crdt::CrdtTransaction;
use uuid::Uuid;

use super::{store, Subject};
use crate::mcp_server::registry::AgentKind;
use crate::storage::models;
use crate::storage::providers::{self, ProviderKind};

/// The adapter name of the provider a delegate's models belong to.
fn vendor_of(kind: AgentKind) -> Option<&'static str> {
    match kind {
        AgentKind::ClaudeCodeDelegate => Some("claude"),
        AgentKind::CodexDelegate => Some("codex"),
        AgentKind::External => None,
    }
}

/// Moves the memberships of every delegate row to the models of its provider. Returns how many
/// delegate rows were converted; run on every open, a vault that is current has nothing to do.
pub fn convert_memberships(tx: &mut CrdtTransaction<'_>) -> haex_crdt::Result<usize> {
    let rows = tx.query_map(
        "SELECT id, kind FROM haex_mcp_agents WHERE kind <> 'external'",
        params![],
        |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?)),
    )?;
    let providers = providers::list_providers(tx)?;
    let mut converted = 0;
    for (id, kind) in rows {
        let (Ok(id), Some(kind)) = (Uuid::parse_str(&id), AgentKind::parse(&kind)) else {
            continue;
        };
        let Some(vendor) = vendor_of(kind) else {
            continue;
        };
        let subject = Subject::Agent(id);
        let memberships = store::memberships(tx, &subject)?;
        if memberships.profiles.is_empty() {
            continue;
        }
        let mut models_of_vendor = Vec::new();
        for provider in providers.iter().filter(|provider| {
            provider.kind == ProviderKind::CliDelegate
                && provider.adapter.as_deref() == Some(vendor)
        }) {
            models_of_vendor.extend(models::list_models_by_provider(tx, provider.id)?);
        }
        if models_of_vendor.is_empty() {
            continue;
        }
        for model in &models_of_vendor {
            for profile in &memberships.profiles {
                store::add_member(tx, &Subject::Model(model.id.clone()), *profile)?;
            }
        }
        store::remove_all_memberships(tx, &subject)?;
        converted += 1;
    }
    Ok(converted)
}
