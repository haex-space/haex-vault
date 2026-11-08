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
    /// "Lokaler Agent": a model of the chat agent, of a local, an API or an Abo-Agent's provider
    /// alike (spec 083 FR-027).
    Builtin,
    /// "Anbieter": a provider of models, a member of profiles for every model it has (PR M).
    Provider,
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
    /// Whether the subject is in "Standard": always for an agent, for a model unless the user took
    /// it out (spec 083 FR-006b).
    pub in_standard: bool,
    /// Whether the chat may use the model now: while "Standard" is active unless it left it, else
    /// only a member of the active profile. Always `true` for an agent (FR-006b).
    pub available: bool,
    /// Present for registered agents.
    pub status: Option<AgentStatus>,
    /// A model that is assigned but not offered any more, or a provider whose record disappeared.
    pub missing: bool,
    /// A model of the local provider: downloaded or imported on a device of the vault. The page
    /// lists these as rows, the models of the provider APIs in a pick list.
    pub local: bool,
    /// The name of the provider whose API or subscription answers a model; the page groups the
    /// models by it. Absent for an agent, a local model and a model that is gone.
    pub provider: Option<String>,
    /// `provider:<id>` of the provider a model belongs to: a membership of that subject counts for
    /// the model too (PR M).
    pub provider_subject: Option<String>,
    /// The profiles the model is in only because its provider is; they are not its own to remove.
    pub inherited: Vec<ProfileChoice>,
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
    // A provider is no subject to count: its models are, whether they are members themselves or
    // through it.
    for subject in subjects.iter().filter(|s| s.kind != SubjectKind::Provider) {
        for profile in subject.profiles.iter().chain(&subject.inherited) {
            *counts.entry(profile.id.clone()).or_default() += 1;
        }
    }
    let mut views = vec![AgentProfileView {
        id: STANDARD_PROFILE_ID.to_string(),
        name: store::STANDARD_NAME.to_owned(),
        is_standard: true,
        is_active: active_id == Some(STANDARD_PROFILE_ID),
        member_count: 0,
    }];
    let profiles = store::profiles(q)?;
    // An active profile whose row is gone counts as "Standard", as the resolver does.
    let active_exists = active_id.is_some_and(|active| profiles.iter().any(|p| p.id == active));
    if active_id.is_some() && !active_exists {
        views[0].is_active = true;
    }
    views[0].member_count = u32::try_from(
        subjects
            .iter()
            .filter(|s| s.kind != SubjectKind::Provider && s.in_standard)
            .count(),
    )
    .unwrap_or(u32::MAX);
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
    // The memberships by subject, a profile that has no row (deleted elsewhere, not synced yet) left out;
    // a row in "Standard" says the subject left it.
    let mut assigned: HashMap<String, Vec<Uuid>> = HashMap::new();
    let mut left_standard: std::collections::HashSet<String> = std::collections::HashSet::new();
    let mut unreadable: std::collections::HashSet<String> = std::collections::HashSet::new();
    let mut provider_ids = std::collections::HashSet::new();
    for (id, subject, profile) in q.query_map(
        "SELECT id, subject, profile_id FROM haex_agent_profile_members ORDER BY subject, profile_id",
        params![],
        |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
            ))
        },
    )? {
        let Some(parsed_subject) = Subject::parse(&subject) else {
            continue;
        };
        if let Subject::Provider(id) = &parsed_subject {
            provider_ids.insert(*id);
        }
        let Ok(profile_id) = Uuid::parse_str(&profile) else {
            unreadable.insert(subject);
            continue;
        };
        if id != super::member_id(&parsed_subject, profile_id)
            && id != super::legacy_member_id(&parsed_subject)
        {
            unreadable.insert(subject);
            continue;
        }
        match profile_id {
            id if id == STANDARD_PROFILE_ID => {
                left_standard.insert(subject);
            }
            id if names.contains_key(&id) => {
                assigned.entry(subject).or_default().push(id);
            }
            _ => {}
        }
    }
    let active = match store::active(q)? {
        store::Active::Profile(id) if names.contains_key(&id) => id,
        _ => STANDARD_PROFILE_ID,
    };
    // The keys whose memberships apply to a subject: its own and, for a model, its provider's.
    let chain_of = |subject: &Subject, provider: Option<Uuid>| -> Vec<String> {
        let mut keys = vec![subject.key()];
        keys.extend(provider.map(|id| Subject::Provider(id).key()));
        keys
    };
    let is_assigned = |key: &str, profile: Uuid| -> bool {
        assigned
            .get(key)
            .is_some_and(|profiles| profiles.contains(&profile))
    };
    let has_unreadable = |keys: &[String]| keys.iter().any(|key| unreadable.contains(key));
    // The profile that applies, as `resolve::profile_of` decides it, from the rows read once.
    let applying_of = |keys: &[String]| -> (String, String) {
        let id = if !has_unreadable(keys) && keys.iter().any(|key| is_assigned(key, active)) {
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
    let in_standard_of = |keys: &[String]| {
        !has_unreadable(keys) && !keys.iter().any(|key| left_standard.contains(key))
    };
    // Whether the chat may use a model now, as `resolve::model_available` decides it.
    let available_of = |keys: &[String]| -> bool {
        if has_unreadable(keys) {
            return false;
        }
        if active == STANDARD_PROFILE_ID {
            in_standard_of(keys)
        } else {
            keys.iter().any(|key| is_assigned(key, active))
        }
    };
    let choice = |id: &Uuid| ProfileChoice {
        id: id.to_string(),
        name: names.get(id).cloned().unwrap_or_default(),
    };
    let profiles_of = |key: &str| -> Vec<ProfileChoice> {
        let mut profiles: Vec<ProfileChoice> = assigned
            .get(key)
            .into_iter()
            .flatten()
            .map(choice)
            .collect();
        profiles.sort_by_key(|profile| profile.name.to_lowercase());
        profiles
    };
    // The profiles a model is in only through its provider.
    let inherited_of = |own: &[ProfileChoice], provider: Option<Uuid>| -> Vec<ProfileChoice> {
        let Some(provider) = provider else {
            return Vec::new();
        };
        profiles_of(&Subject::Provider(provider).key())
            .into_iter()
            .filter(|profile| !own.iter().any(|mine| mine.id == profile.id))
            .collect()
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
        // A delegate's row is a technical identity; its models are the subjects (FR-027).
        if kind != AgentKind::External {
            continue;
        }
        let subject = Subject::Agent(id);
        let (profile_id, profile_name) = applying_of(&chain_of(&subject, None));
        subjects.push(AgentSubjectView {
            subject: subject.key(),
            kind: SubjectKind::External,
            name,
            device: devices.get(&device).cloned().flatten(),
            profile_id,
            profile_name,
            profiles: profiles_of(&subject.key()),
            in_standard: true,
            available: true,
            status: Some(status),
            missing: false,
            local: false,
            provider: None,
            provider_subject: None,
            inherited: Vec::new(),
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
    provider_ids.extend(
        providers
            .values()
            .filter(|provider| provider.capability == ProviderCapability::Chat)
            .map(|provider| provider.id),
    );
    let mut listed: HashMap<String, AgentSubjectView> = HashMap::new();
    for model in models::list_all_models(q)? {
        // A model whose provider row is not here (yet) but whose file was installed is a local one.
        let provider = providers.get(&model.provider_id);
        let kind = match provider {
            Some(provider) => provider.kind,
            None if model.file_sha256.is_some() => ProviderKind::Local,
            None => continue,
        };
        if provider.is_some_and(|provider| provider.capability != ProviderCapability::Chat) {
            continue;
        }
        provider_ids.insert(model.provider_id);
        let subject = Subject::Model(model.id.clone());
        let name = match (kind, provider) {
            (ProviderKind::ApiKey | ProviderKind::CliDelegate, Some(provider)) => {
                format!("{} ({})", model.name, provider.name)
            }
            _ => model.name.clone(),
        };
        let keys = chain_of(&subject, Some(model.provider_id));
        let (profile_id, profile_name) = applying_of(&keys);
        let own = profiles_of(&subject.key());
        listed.insert(
            subject.key(),
            AgentSubjectView {
                subject: subject.key(),
                kind: SubjectKind::Builtin,
                name,
                device: None,
                profile_id,
                profile_name,
                inherited: inherited_of(&own, Some(model.provider_id)),
                profiles: own,
                in_standard: in_standard_of(&keys),
                available: available_of(&keys),
                status: None,
                // A subject is missing only when no provider model row remains (below).
                missing: false,
                local: kind == ProviderKind::Local,
                provider: match (kind, provider) {
                    (ProviderKind::ApiKey | ProviderKind::CliDelegate, Some(provider)) => {
                        Some(provider.name.clone())
                    }
                    _ => None,
                },
                provider_subject: Some(Subject::Provider(model.provider_id).key()),
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
        if !listed.contains_key(key) {
            let (profile_id, profile_name) = applying_of(&chain_of(&subject, None));
            listed.insert(
                key.clone(),
                AgentSubjectView {
                    subject: key.clone(),
                    kind: SubjectKind::Builtin,
                    name: model.clone(),
                    device: None,
                    profile_id,
                    profile_name,
                    profiles: profiles_of(&subject.key()),
                    in_standard: in_standard_of(&chain_of(&subject, None)),
                    available: available_of(&chain_of(&subject, None)),
                    status: None,
                    missing: true,
                    local: false,
                    provider: None,
                    provider_subject: None,
                    inherited: Vec::new(),
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
    // List chat providers even before they have models, and keep stored memberships manageable
    // when their provider record or last model has disappeared.
    let mut provider_ids: Vec<Uuid> = provider_ids.into_iter().collect();
    provider_ids.sort_by_key(|id| {
        providers
            .get(id)
            .map(|provider| provider.name.to_lowercase())
            .unwrap_or_else(|| id.to_string())
    });
    for id in provider_ids {
        let subject = Subject::Provider(id);
        let keys = chain_of(&subject, None);
        let (profile_id, profile_name) = applying_of(&keys);
        let provider = providers.get(&id);
        subjects.push(AgentSubjectView {
            subject: subject.key(),
            kind: SubjectKind::Provider,
            name: provider
                .map(|provider| provider.name.clone())
                .unwrap_or_else(|| id.to_string()),
            device: None,
            profile_id,
            profile_name,
            profiles: profiles_of(&subject.key()),
            in_standard: in_standard_of(&keys),
            available: true,
            status: None,
            missing: provider.is_none(),
            local: provider.is_some_and(|provider| provider.kind == ProviderKind::Local),
            provider: None,
            provider_subject: None,
            inherited: Vec::new(),
        });
    }
    Ok(subjects)
}
