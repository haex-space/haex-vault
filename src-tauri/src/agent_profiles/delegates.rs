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

/// The vault preference that held the autonomy mode of Abo-Agents before spec 083 PR J3.
const PREF_AUTONOMY_MODE: &str = "chat.autonomy_mode";
/// The vault preference that held the deny categories of the permissive mode before PR J3.
const PREF_DENY_RULES: &str = "cli_delegate.deny_rules";

/// Reads the settings of before PR J3 into "Standard" once, then removes them (spec 083 FR-030): the
/// autonomy mode and the deny rules of Abo-Agents are rights of a profile now, and what the user set
/// is the starting value of "Standard", which every model without a profile of its own is in. Does
/// nothing when neither setting exists (a vault that is current, or one that never set them, which
/// has the factory value of "Standard"). A value this build cannot read (a mode that is no mode, deny
/// rules that are no list of known categories) becomes the group that asks for every call, never a
/// wider one. Returns whether anything was converted.
pub fn convert_autonomy_settings(
    tx: &mut CrdtTransaction<'_>,
    device: Uuid,
) -> haex_crdt::Result<bool> {
    use super::{resolve, STANDARD_PROFILE_ID};
    use crate::adapters::cli_delegate::autonomy::{AutonomyMode, DenyCategory};
    use crate::mcp_server::grants::DelegateRights;
    use crate::storage::preferences::{self, PrefScope};

    let device = PrefScope::Device(device);
    let read = |tx: &mut CrdtTransaction<'_>, key: &str| -> haex_crdt::Result<Option<String>> {
        Ok(match preferences::get(tx, PrefScope::Vault, key)? {
            Some(value) => Some(value),
            None => preferences::get(tx, device, key)?,
        })
    };
    let mode = read(tx, PREF_AUTONOMY_MODE)?;
    let deny = read(tx, PREF_DENY_RULES)?;
    if mode.is_none() && deny.is_none() {
        return Ok(false);
    }
    // The old product default was "ungated" (spec 009, amendment of 2026-09-19).
    let autonomy = match mode.as_deref() {
        None => Some(AutonomyMode::Ungated),
        Some(label) => AutonomyMode::parse(label),
    };
    let deny_rules: Option<std::collections::BTreeSet<DenyCategory>> = match deny.as_deref() {
        None => Some(std::collections::BTreeSet::new()),
        Some(raw) => serde_json::from_str::<Vec<String>>(raw)
            .ok()
            .and_then(|labels| {
                labels
                    .iter()
                    .map(|label| DenyCategory::parse(label))
                    .collect()
            }),
    };
    let delegate = match (autonomy, deny_rules) {
        (Some(autonomy), Some(deny)) => DelegateRights { autonomy, deny },
        _ => DelegateRights::asking(),
    };
    let mut rights = resolve::profile_rights(tx, STANDARD_PROFILE_ID)?;
    rights.delegate = Some(delegate);
    store::set_rights(tx, STANDARD_PROFILE_ID, &rights)?;
    for key in [PREF_AUTONOMY_MODE, PREF_DENY_RULES] {
        preferences::delete(tx, PrefScope::Vault, key)?;
        preferences::delete(tx, device, key)?;
    }
    Ok(true)
}
