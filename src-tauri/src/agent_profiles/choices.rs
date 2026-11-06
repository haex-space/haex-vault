//! What a registration asks (spec 083 US4, FR-014): the profiles to pick from, the one that is
//! preselected and what it gives. A new agent starts in "Standard"; one that registers again
//! starts in the profile it had, so a renewal keeps the assignment unless the user picks another.

use serde::Serialize;
use ts_rs::TS;
use uuid::Uuid;

use super::{resolve, store, Subject, STANDARD_PROFILE_ID};
use crate::mcp_server::grants::Rights;
use crate::storage::query::Query;

/// A profile the registration dialog offers.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/types/bindings/")]
pub struct ProfileChoice {
    pub id: String,
    pub name: String,
}

/// The profiles, the preselected one and its rights.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Choices {
    /// "Standard" first, then the others by name.
    pub profiles: Vec<ProfileChoice>,
    pub selected: Uuid,
    /// What the selected profile gives now.
    pub rights: Rights,
}

/// The choices for `current`, the subject that registers again; `None` for a new agent.
pub fn choices(q: &mut impl Query, current: Option<&Subject>) -> haex_crdt::Result<Choices> {
    let mut profiles = vec![ProfileChoice {
        id: STANDARD_PROFILE_ID.to_string(),
        name: store::STANDARD_NAME.to_owned(),
    }];
    let others = store::profiles(q)?;
    profiles.extend(
        others
            .iter()
            .filter(|profile| profile.id != STANDARD_PROFILE_ID)
            .map(|profile| ProfileChoice {
                id: profile.id.to_string(),
                name: profile.name.clone(),
            }),
    );
    let selected = match current {
        Some(subject) => store::member_profile(q, subject)?
            .and_then(|text| Uuid::parse_str(&text).ok())
            .filter(|id| others.iter().any(|profile| profile.id == *id))
            .unwrap_or(STANDARD_PROFILE_ID),
        None => STANDARD_PROFILE_ID,
    };
    let rights = resolve::profile_rights(q, selected)?;
    Ok(Choices {
        profiles,
        selected,
        rights,
    })
}

/// Whether `profile` can be chosen: "Standard" always, another one while its row exists.
pub fn exists(q: &mut impl Query, profile: Uuid) -> haex_crdt::Result<bool> {
    Ok(profile == STANDARD_PROFILE_ID || store::profile(q, profile)?.is_some())
}
