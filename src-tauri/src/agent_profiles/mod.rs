//! Agent profiles (spec 083): one set of rights over every kind of access, kept once per vault.
//!
//! A profile is a row of `haex_agent_profiles` with its rights as rows of
//! `haex_agent_profile_grants`, in the shape `mcp_server::grant_rows` already converts to and from
//! [`Rights`](crate::mcp_server::grants::Rights). A subject can be a member of any number of
//! profiles: one row of `haex_agent_profile_members` per membership, with an id that derives from
//! the subject and the profile. Exactly one profile is active for the vault (a preference), and a
//! subject has the rights of the active profile when it is a member of it, else those of "Standard"
//! ([`STANDARD_PROFILE_ID`]), which every subject is in and which has no row of its own until it is
//! edited and then resolves to the factory rights ([`resolve`]). A profile is its id, never its
//! name. Existing agent memberships move to profiles; the local agent's old rights are reset, while
//! Abo-Agent autonomy settings are migrated into Standard and the profiles of their existing models
//! (spec 083, FR-024, FR-030, FR-031).
//!
//! - [`store`]: the three tables.
//! - [`subjects`], [`choices`], [`commands`]: what the pages and the registration dialog ask.
//! - [`resolve`]: the rights of a subject, the one place `mcp_server::registry::load_rights`
//!   reads them from.

use uuid::Uuid;

pub mod choices;
pub mod commands;
pub mod delegates;
pub mod identity;
pub mod local;
pub mod remember;
pub mod resolve;
pub mod store;
pub mod subjects;

#[cfg(test)]
mod delegates_tests;
#[cfg(test)]
mod identity_tests;
#[cfg(test)]
mod provider_tests;
#[cfg(test)]
mod remember_tests;
#[cfg(test)]
mod resolve_tests;
#[cfg(test)]
mod store_tests;
#[cfg(test)]
mod subjects_tests;
#[cfg(test)]
pub(crate) mod test_support;

/// The id of the profile "Standard": every subject without an assignment is in it. It cannot be
/// deleted or renamed. A constant, never generated, so every device means the same profile.
pub const STANDARD_PROFILE_ID: Uuid = Uuid::from_u128(0x6170_5f70_726f_6669_6c65_5f73_7464_0083);

/// Namespace of the ids of `haex_agent_profile_members`.
const NS_PROFILE_MEMBER: Uuid = Uuid::from_u128(0x6170_5f6d_656d_6265_725f_7370_6563_0083);
/// Namespace of the ids of `haex_agent_profile_grants`.
const NS_PROFILE_GRANT: Uuid = Uuid::from_u128(0x6170_5f67_7261_6e74_5f73_7065_6330_0083);

/// Who a profile is assigned to.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Subject {
    /// An external agent or an Abo-Agent: a row of `haex_mcp_agents`.
    Agent(Uuid),
    /// A model of the local agent: its catalog id, or `<provider id>:<remote id>` for a provider.
    Model(String),
    /// A provider of models (spec 083 PR M): every model it has, a later one too, counts as a member
    /// wherever the provider is, in addition to its own memberships.
    Provider(Uuid),
}

impl Subject {
    /// The text stored in `haex_agent_profile_members.subject`.
    pub fn key(&self) -> String {
        match self {
            Self::Agent(id) => format!("agent:{id}"),
            Self::Model(id) => format!("model:{id}"),
            Self::Provider(id) => format!("provider:{id}"),
        }
    }

    /// The subject a stored text names; `None` for anything this build does not know.
    pub fn parse(key: &str) -> Option<Self> {
        if let Some(id) = key.strip_prefix("agent:") {
            return Uuid::parse_str(id).ok().map(Self::Agent);
        }
        if let Some(id) = key.strip_prefix("provider:") {
            return Uuid::parse_str(id).ok().map(Self::Provider);
        }
        key.strip_prefix("model:")
            .filter(|id| !id.is_empty())
            .map(|id| Self::Model(id.to_owned()))
    }
}

/// The id of the member row of `subject` in `profile`: a function of both (spec 083 R3, revised for
/// several memberships), so two devices that add the same membership write the same row and
/// different memberships never conflict.
pub fn member_id(subject: &Subject, profile: Uuid) -> String {
    Uuid::new_v5(
        &NS_PROFILE_MEMBER,
        format!("{}:{profile}", subject.key()).as_bytes(),
    )
    .to_string()
}

/// The id member rows had before a subject could be in several profiles: a function of the subject
/// alone. Such a row still is a membership of the profile in its `profile_id`; it is never rewritten
/// (see [`store::memberships`]), because a rewrite is an insert that a later sync would take for newer
/// than a removal another device made, and the tombstone of that removal expires.
pub fn legacy_member_id(subject: &Subject) -> String {
    Uuid::new_v5(&NS_PROFILE_MEMBER, subject.key().as_bytes()).to_string()
}

/// The id of a rights row of `profile`.
pub fn grant_id(profile: Uuid, group: &str, key: &str) -> String {
    Uuid::new_v5(
        &NS_PROFILE_GRANT,
        format!("{profile}:{group}:{key}").as_bytes(),
    )
    .to_string()
}
