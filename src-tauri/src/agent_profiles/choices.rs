//! What a registration asks (spec 083 US4, FR-013, FR-014): the profiles to pick from, the one that
//! is preselected and what it would give. An agent is a member of no profile but "Standard" unless
//! the user picks one to join; one that registers again keeps the memberships it had, and the field
//! adds one more.

use serde::Serialize;
use ts_rs::TS;
use uuid::Uuid;

use super::{resolve, store, STANDARD_PROFILE_ID};
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
    /// What the selected profile would give: its own rights while it is the active profile, else
    /// those of "Standard".
    pub rights: Rights,
}

/// The profiles to pick from, "Standard" (no profile to join) preselected.
pub fn choices(
    q: &mut impl Query,
    retained: Option<&super::Subject>,
) -> haex_crdt::Result<Choices> {
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
    let selected = STANDARD_PROFILE_ID;
    let rights = match retained {
        Some(subject) => resolve::rights_of(q, subject)?,
        None => resolve::rights_if_member(q, selected)?,
    };
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
