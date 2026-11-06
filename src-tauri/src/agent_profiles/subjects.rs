//! Who profiles apply to, as the pages list them (spec 083 FR-016, data-model.md "Member count"):
//! the agents of every device of the vault (`haex_mcp_agents` is synced) and the models that are
//! assigned. The member count of a profile is derived from this list, so the number a page shows
//! and the agents it lists are the same.

use std::collections::HashMap;

use haex_crdt::rusqlite::params;
use serde::Serialize;
use ts_rs::TS;
use uuid::Uuid;

use super::{store, Subject, STANDARD_PROFILE_ID};
use crate::mcp_server::registry::{AgentKind, AgentStatus};
use crate::storage::known_devices;
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
    pub profile_id: String,
    pub profile_name: String,
    /// Present for registered agents.
    pub status: Option<AgentStatus>,
    /// A model that is assigned but not offered any more, so it can be removed (FR-008).
    pub missing: bool,
}

/// A profile with the number of subjects in it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/types/bindings/")]
pub struct AgentProfileView {
    pub id: String,
    pub name: String,
    pub is_standard: bool,
    pub member_count: u32,
}

/// Every profile, "Standard" first, then by name, each with its member count.
pub fn profile_views(q: &mut impl Query) -> haex_crdt::Result<Vec<AgentProfileView>> {
    let subjects = list_subjects(q)?;
    let mut counts: HashMap<String, u32> = HashMap::new();
    for subject in &subjects {
        *counts.entry(subject.profile_id.clone()).or_default() += 1;
    }
    let mut views = vec![AgentProfileView {
        id: STANDARD_PROFILE_ID.to_string(),
        name: store::STANDARD_NAME.to_owned(),
        is_standard: true,
        member_count: counts
            .get(&STANDARD_PROFILE_ID.to_string())
            .copied()
            .unwrap_or(0),
    }];
    for profile in store::profiles(q)? {
        if profile.id == STANDARD_PROFILE_ID {
            continue;
        }
        views.push(AgentProfileView {
            id: profile.id.to_string(),
            member_count: counts.get(&profile.id.to_string()).copied().unwrap_or(0),
            name: profile.name,
            is_standard: false,
        });
    }
    Ok(views)
}

/// Every subject of the vault with the profile it is in now: the agents of every device (a
/// blocked one has no rights and is left out), then the models that have a member row. A member
/// row whose profile is gone counts as "Standard", as the resolver does.
pub fn list_subjects(q: &mut impl Query) -> haex_crdt::Result<Vec<AgentSubjectView>> {
    let names: HashMap<Uuid, String> = store::profiles(q)?
        .into_iter()
        .map(|profile| (profile.id, profile.name))
        .collect();
    let assigned: HashMap<String, String> = q
        .query_map(
            "SELECT subject, profile_id FROM haex_agent_profile_members",
            params![],
            |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?)),
        )?
        .into_iter()
        .collect();
    let profile_of = |subject: &Subject| -> (String, String) {
        let id = assigned
            .get(&subject.key())
            .and_then(|text| Uuid::parse_str(text).ok())
            .filter(|id| names.contains_key(id))
            .unwrap_or(STANDARD_PROFILE_ID);
        let name = names
            .get(&id)
            .cloned()
            .unwrap_or_else(|| store::STANDARD_NAME.to_owned());
        (id.to_string(), name)
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
        let (profile_id, profile_name) = profile_of(&subject);
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
            status: Some(status),
            missing: false,
        });
    }
    // The models: only those that have a member row are known here; the chat adds the offered ones.
    let mut models: Vec<&String> = assigned
        .keys()
        .filter(|key| key.starts_with("model:"))
        .collect();
    models.sort();
    for key in models {
        let Some(subject) = Subject::parse(key) else {
            continue;
        };
        let Subject::Model(model) = &subject else {
            continue;
        };
        let (profile_id, profile_name) = profile_of(&subject);
        subjects.push(AgentSubjectView {
            subject: key.clone(),
            kind: SubjectKind::Builtin,
            name: model.clone(),
            device: None,
            profile_id,
            profile_name,
            status: None,
            missing: false,
        });
    }
    Ok(subjects)
}
