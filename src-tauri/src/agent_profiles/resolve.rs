//! The rights of a subject (spec 083, data-model.md "Resolution"): the profile of its member row,
//! else "Standard", read through the converter `mcp_server::grant_rows` uses for every rights row.

use uuid::Uuid;

use super::{store, Subject, STANDARD_PROFILE_ID};
use crate::mcp_server::grant_rows::rights_of as rights_from_rows;
use crate::mcp_server::grants::{default_rights, Rights};
use crate::storage::query::Query;

/// The rights of `subject`.
///
/// - No member row: "Standard".
/// - A member row whose profile has no row (deleted on another device, not synced yet): "Standard".
/// - A member row this build cannot read: no right (FR-012), never a wider one.
/// - "Standard" without a row of its own: the factory rights, [`default_rights`].
/// - A rights row this build cannot read is skipped by the converter, so it gives less, not more.
pub fn rights_of(q: &mut impl Query, subject: &Subject) -> haex_crdt::Result<Rights> {
    let profile = match store::member_assignment(q, subject)? {
        store::MemberAssignment::Missing => STANDARD_PROFILE_ID,
        store::MemberAssignment::Invalid => return Ok(Rights::default()),
        store::MemberAssignment::Assigned(text) => match Uuid::parse_str(&text) {
            Ok(id) => id,
            Err(_) => return Ok(Rights::default()),
        },
    };
    profile_rights(q, profile)
}

/// The rights of a profile; one that has no row counts as "Standard".
pub fn profile_rights(q: &mut impl Query, profile: Uuid) -> haex_crdt::Result<Rights> {
    if store::profile(q, profile)?.is_none() {
        if profile != STANDARD_PROFILE_ID {
            return standard_rights(q);
        }
        return Ok(default_rights());
    }
    Ok(rights_from_rows(store::rights_rows(q, profile)?))
}

/// What "Standard" gives right now on this device: the factory rights until it is edited.
pub fn standard_rights(q: &mut impl Query) -> haex_crdt::Result<Rights> {
    profile_rights(q, STANDARD_PROFILE_ID)
}

/// `rights` as rows would store them, so two values of the same rights compare equal: the way to
/// ask whether an agent has what "Standard" gives.
pub fn normalized(rights: &Rights) -> Rights {
    rights_from_rows(crate::mcp_server::grant_rows::rows_of(rights))
}
