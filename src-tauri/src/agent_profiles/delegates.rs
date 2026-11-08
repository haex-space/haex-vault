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
/// A synced marker prevents another device's stale device-scoped value from running this migration
/// again after the first device has converted and deleted the old preferences.
const PREF_SETTINGS_MIGRATED: &str = "agent_profiles.abo_settings_migrated";

/// Reads the settings of before PR J3 into "Standard" and the profiles used by existing Abo-Agent
/// models, then removes them (spec 083 FR-030). Existing explicit delegate groups keep their value.
/// This preserves the old global restrictions for models already assigned to custom profiles.
/// If neither setting exists it does nothing. A value this build cannot read becomes the group that
/// asks for every call, never a wider one. Returns whether settings were converted.
pub fn convert_autonomy_settings(
    tx: &mut CrdtTransaction<'_>,
    device: Uuid,
) -> haex_crdt::Result<bool> {
    use super::{resolve, STANDARD_PROFILE_ID};
    use crate::adapters::cli_delegate::autonomy::{AutonomyMode, DenyCategory};
    use crate::mcp_server::grants::DelegateRights;
    use crate::storage::preferences::{self, PrefScope};

    let device = PrefScope::Device(device);
    if preferences::get(tx, PrefScope::Vault, PREF_SETTINGS_MIGRATED)?.as_deref() == Some("1") {
        for key in [PREF_AUTONOMY_MODE, PREF_DENY_RULES] {
            preferences::delete(tx, PrefScope::Vault, key)?;
            preferences::delete(tx, device, key)?;
        }
        return Ok(false);
    }
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
    let mut profiles = std::collections::BTreeSet::from([STANDARD_PROFILE_ID]);

    // Before PR J3 these settings applied to all Abo-Agent runs. Preserve them for models that had
    // already been assigned to a custom profile, not only for subjects that fall back to Standard.
    for provider in providers::list_providers(tx)?
        .into_iter()
        .filter(|provider| {
            provider.kind == ProviderKind::CliDelegate
                && matches!(provider.adapter.as_deref(), Some("claude" | "codex"))
        })
    {
        for model in models::list_models_by_provider(tx, provider.id)? {
            profiles.extend(store::memberships(tx, &Subject::Model(model.id))?.profiles);
        }
    }

    // A delegate whose provider has no models yet retains its memberships until a later open.
    // Seed those profiles too, so the conversion remains complete when the models arrive.
    let delegate_rows = tx.query_map(
        "SELECT id, kind FROM haex_mcp_agents WHERE kind <> 'external'",
        params![],
        |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?)),
    )?;
    for (id, kind) in delegate_rows {
        let (Ok(id), Some(_)) = (Uuid::parse_str(&id), AgentKind::parse(&kind)) else {
            continue;
        };
        profiles.extend(store::memberships(tx, &Subject::Agent(id))?.profiles);
    }

    for profile in profiles {
        if profile != STANDARD_PROFILE_ID && store::profile(tx, profile)?.is_none() {
            continue;
        }
        let mut rights = resolve::profile_rights(tx, profile)?;
        let has_delegate_group = store::rights_rows(tx, profile)?
            .iter()
            .any(|row| row.group == "delegate");
        if !has_delegate_group {
            rights.delegate = Some(delegate.clone());
            store::set_rights(tx, profile, &rights)?;
        }
    }
    for key in [PREF_AUTONOMY_MODE, PREF_DENY_RULES] {
        preferences::delete(tx, PrefScope::Vault, key)?;
        preferences::delete(tx, device, key)?;
    }
    preferences::insert_or_update(tx, PrefScope::Vault, PREF_SETTINGS_MIGRATED, "1")?;
    Ok(true)
}
