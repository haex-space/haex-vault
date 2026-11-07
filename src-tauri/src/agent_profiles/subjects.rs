//! Who profiles apply to, as the pages list them (spec 083 FR-016, data-model.md "Member count"):
//! the agents of every device of the vault (`haex_mcp_agents` is synced) and the models. The member
//! count of a profile is derived from this list, so the number a page shows and the agents it lists
//! are the same; every subject is a member of "Standard".

use std::collections::HashMap;

use haex_crdt::rusqlite::params;
use serde::Serialize;
use ts_rs::TS;
use uuid::Uuid;

use super::choices::ProfileChoice;
use super::{store, Subject, STANDARD_PROFILE_ID};
use crate::mcp_server::registry::{AgentKind, AgentStatus};
use crate::storage::known_devices;
use crate::storage::models;
use crate::storage::providers::{self, Provider, ProviderCapability, ProviderKind};
use crate::storage::query::Query;

/// What kind of agent a subject is, as the pages label it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/types/bindings/")]
pub enum SubjectKind {
    /// "Externer Agent".
    External,
    /// "Abo-Agent": a delegate that works through a subscription.
    Subscription,
    /// "Lokaler Agent": a model of the chat agent.
    Builtin,
}

/// One subject with the profile it is in.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/types/bindings/")]
pub struct AgentSubjectView {
    /// `agent:<id>` or `model:<id>`, what the assign command takes back.
    pub subject: String,
    pub kind: SubjectKind,
    pub name: String,
    /// The alias of the device an agent is registered on; absent for a model.
    pub device: Option<String>,
    /// The profile that applies to the subject now: the active one when it is a member of it, else
    /// "Standard" (spec 083 FR-006a).
    pub profile_id: String,
    pub profile_name: String,
    /// The profiles the subject is a member of; "Standard", which every subject is in, is not among
    /// them.
    pub profiles: Vec<ProfileChoice>,
    /// Present for registered agents.
    pub status: Option<AgentStatus>,
    /// A model that is assigned but not offered any more, so it can be removed (FR-008).
    pub missing: bool,
    /// A model of the local provider: downloaded or imported on a device of the vault. The page
    /// lists these as rows, the models of the provider APIs in a pick list.
    pub local: bool,
}

/// A profile with the number of subjects in it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/types/bindings/")]
pub struct AgentProfileView {
    pub id: String,
    pub name: String,
    pub is_standard: bool,
    /// The profile that is active for the vault (spec 083 FR-006a).
    pub is_active: bool,
    pub member_count: u32,
}

/// Every profile, "Standard" first, then by name, each with its member count.
pub fn profile_views(q: &mut impl Query) -> haex_crdt::Result<Vec<AgentProfileView>> {
    let subjects = list_subjects(q)?;
    let active = store::active(q)?;
    let active_id = match active {
        store::Active::Profile(id) => Some(id),
        store::Active::Unset => Some(STANDARD_PROFILE_ID),
        store::Active::Unreadable => None,
    };
    let mut counts: HashMap<String, u32> = HashMap::new();
    for subject in &subjects {
        for profile in &subject.profiles {
            *counts.entry(profile.id.clone()).or_default() += 1;
        }
    }
    let mut views = vec![AgentProfileView {
        id: STANDARD_PROFILE_ID.to_string(),
        name: store::STANDARD_NAME.to_owned(),
        is_standard: true,
        is_active: active_id == Some(STANDARD_PROFILE_ID),
        member_count: u32::try_from(subjects.len()).unwrap_or(u32::MAX),
    }];
    let profiles = store::profiles(q)?;
    // An active profile whose row is gone counts as "Standard", as the resolver does.
    let active_exists = active_id.is_some_and(|active| profiles.iter().any(|p| p.id == active));
    if active_id.is_some() && !active_exists {
        views[0].is_active = true;
    }
    for profile in profiles {
        if profile.id == STANDARD_PROFILE_ID {
            continue;
        }
        views.push(AgentProfileView {
            id: profile.id.to_string(),
            member_count: counts.get(&profile.id.to_string()).copied().unwrap_or(0),
            is_active: active_id == Some(profile.id),
            name: profile.name,
            is_standard: false,
        });
    }
    Ok(views)
}

/// Every subject of the vault with the profile that applies to it now: the agents of every device (a
/// blocked one has no rights and is left out), then the models that have a member row. A member
/// row whose profile is gone counts as "Standard", as the resolver does.
pub fn list_subjects(q: &mut impl Query) -> haex_crdt::Result<Vec<AgentSubjectView>> {
    let names: HashMap<Uuid, String> = store::profiles(q)?
        .into_iter()
        .map(|profile| (profile.id, profile.name))
        .collect();
    // The memberships by subject, a profile that has no row (deleted elsewhere, not synced yet) left out.
    let mut assigned: HashMap<String, Vec<Uuid>> = HashMap::new();
    for (subject, profile) in q.query_map(
        "SELECT subject, profile_id FROM haex_agent_profile_members ORDER BY subject, profile_id",
        params![],
        |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?)),
    )? {
        if let Some(id) = Uuid::parse_str(&profile)
            .ok()
            .filter(|id| names.contains_key(id))
        {
            assigned.entry(subject).or_default().push(id);
        }
    }
    let active = match store::active(q)? {
        store::Active::Profile(id) if names.contains_key(&id) => id,
        _ => STANDARD_PROFILE_ID,
    };
    // The profile that applies, as `resolve::profile_of` decides it, from the rows read once.
    let applying_of = |subject: &Subject| -> (String, String) {
        let id = if assigned
            .get(&subject.key())
            .is_some_and(|profiles| profiles.contains(&active))
        {
            active
        } else {
            STANDARD_PROFILE_ID
        };
        let name = names
            .get(&id)
            .cloned()
            .unwrap_or_else(|| store::STANDARD_NAME.to_owned());
        (id.to_string(), name)
    };
    let profiles_of = |subject: &Subject| -> Vec<ProfileChoice> {
        let mut profiles: Vec<ProfileChoice> = assigned
            .get(&subject.key())
            .into_iter()
            .flatten()
            .map(|id| ProfileChoice {
                id: id.to_string(),
                name: names.get(id).cloned().unwrap_or_default(),
            })
            .collect();
        profiles.sort_by_key(|profile| profile.name.to_lowercase());
        profiles
    };
    let devices: HashMap<Uuid, Option<String>> = known_devices::list_devices(q)?
        .into_iter()
        .map(|device| (device.vault_device_uuid, device.alias))
        .collect();
    let rows = q.query_map(
        "SELECT id, vault_device_uuid, kind, client_name, status FROM haex_mcp_agents \
         ORDER BY registered_at DESC",
        params![],
        |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, String>(3)?,
                row.get::<_, String>(4)?,
            ))
        },
    )?;
    let mut subjects = Vec::new();
    for (id, device, kind, name, status) in rows {
        let (Ok(id), Ok(device), Some(kind), Some(status)) = (
            Uuid::parse_str(&id),
            Uuid::parse_str(&device),
            AgentKind::parse(&kind),
            AgentStatus::parse(&status),
        ) else {
            continue;
        };
        if status == AgentStatus::Blocked {
            continue;
        }
        let subject = Subject::Agent(id);
        let (profile_id, profile_name) = applying_of(&subject);
        subjects.push(AgentSubjectView {
            subject: subject.key(),
            kind: match kind {
                AgentKind::External => SubjectKind::External,
                AgentKind::ClaudeCodeDelegate | AgentKind::CodexDelegate => {
                    SubjectKind::Subscription
                }
            },
            name,
            device: devices.get(&device).cloned().flatten(),
            profile_id,
            profile_name,
            profiles: profiles_of(&subject),
            status: Some(status),
            missing: false,
            local: false,
        });
    }
    // The models of the local agent (spec 083 FR-008): every model of the local provider (its row
    // exists once a file of it was downloaded or imported on any device of the vault, whatever its
    // state on this one: the user may switch between several), the chat models of the vault's API
    // providers, and every model that has a member row. One that has a member row but no model row
    // any more is `missing`, so the user can remove it.
    let providers: HashMap<Uuid, Provider> = providers::list_providers(q)?
        .into_iter()
        .map(|provider| (provider.id, provider))
        .collect();
    let mut delegate_models = std::collections::HashSet::new();
    let mut listed: HashMap<String, AgentSubjectView> = HashMap::new();
    for model in models::list_all_models(q)? {
        // A model whose provider row is not here (yet) but whose file was installed is a local one.
        let provider = providers.get(&model.provider_id);
        let kind = match provider {
            Some(provider) => provider.kind,
            None if model.file_sha256.is_some() => ProviderKind::Local,
            None => continue,
        };
        if kind == ProviderKind::CliDelegate {
            delegate_models.insert(model.id.clone());
            continue;
        }
        if provider.is_some_and(|provider| provider.capability != ProviderCapability::Chat) {
            continue;
        }
        let subject = Subject::Model(model.id.clone());
        let name = match (kind, provider) {
            (ProviderKind::ApiKey, Some(provider)) => {
                format!("{} ({})", model.name, provider.name)
            }
            _ => model.name.clone(),
        };
        let (profile_id, profile_name) = applying_of(&subject);
        listed.insert(
            subject.key(),
            AgentSubjectView {
                subject: subject.key(),
                kind: SubjectKind::Builtin,
                name,
                device: None,
                profile_id,
                profile_name,
                profiles: profiles_of(&subject),
                status: None,
                // A subject is missing only when no provider model row remains (below).
                missing: false,
                local: kind == ProviderKind::Local,
            },
        );
    }
    for key in assigned.keys().filter(|key| key.starts_with("model:")) {
        let Some(subject) = Subject::parse(key) else {
            continue;
        };
        let Subject::Model(model) = &subject else {
            continue;
        };
        if delegate_models.contains(model) {
            continue;
        }
        if !listed.contains_key(key) {
            let (profile_id, profile_name) = applying_of(&subject);
            listed.insert(
                key.clone(),
                AgentSubjectView {
                    subject: key.clone(),
                    kind: SubjectKind::Builtin,
                    name: model.clone(),
                    device: None,
                    profile_id,
                    profile_name,
                    profiles: profiles_of(&subject),
                    status: None,
                    missing: true,
                    local: false,
                },
            );
        }
    }
    let mut models: Vec<AgentSubjectView> = listed.into_values().collect();
    models.sort_by(|a, b| {
        a.name
            .to_lowercase()
            .cmp(&b.name.to_lowercase())
            .then(a.subject.cmp(&b.subject))
    });
    subjects.extend(models);
    Ok(subjects)
}
