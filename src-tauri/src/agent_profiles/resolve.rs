//! The rights of a subject (spec 083, data-model.md "Resolution"): the profile of its member row,
//! else "Standard", read through the converter `mcp_server::grant_rows` uses for every rights row.

use uuid::Uuid;

use super::{store, Subject, STANDARD_PROFILE_ID};
use crate::mcp_server::grant_rows::rights_of as rights_from_rows;
use crate::mcp_server::grants::{default_rights, Rights};
use crate::storage::query::Query;

/// The profile that applies to `subject` now, or `None` when what is stored cannot be read.
///
/// - The active profile when `subject` is a member of it, or its provider is (spec 083 PR M); every
///   subject is in "Standard".
/// - Else "Standard": a subject in no active profile has the same rights as any other.
/// - An active profile whose row is gone (deleted on another device, not synced yet): "Standard".
/// - A value of the active profile, or a member row of the subject, this build cannot read while a
///   profile other than "Standard" is active: `None`, no right (FR-012), never a wider one.
fn applying(q: &mut impl Query, subject: &Subject) -> haex_crdt::Result<Option<Uuid>> {
    let active = match store::active(q)? {
        store::Active::Unset => STANDARD_PROFILE_ID,
        store::Active::Unreadable => return Ok(None),
        store::Active::Profile(id) => id,
    };
    let chain = store::chain(q, subject)?;
    for link in &chain {
        let expected_id = super::member_id(link, active);
        if store::member_subject(q, &expected_id)?.is_some_and(|stored| stored != link.key()) {
            return Ok(None);
        }
    }
    let memberships = store::effective_memberships(q, subject)?;
    if memberships.unreadable {
        return Ok(None);
    }
    if active == STANDARD_PROFILE_ID {
        return Ok(Some(STANDARD_PROFILE_ID));
    }
    if memberships.profiles.contains(&active) {
        return Ok(Some(if store::profile(q, active)?.is_some() {
            active
        } else {
            STANDARD_PROFILE_ID
        }));
    }
    Ok(Some(STANDARD_PROFILE_ID))
}

/// The profile a remembered browser permission may update. Unlike the display lookup, this
/// refuses an unreadable active value or membership instead of silently choosing Standard.
pub fn profile_for_remember(
    q: &mut impl Query,
    subject: &Subject,
) -> haex_crdt::Result<Option<Uuid>> {
    applying(q, subject)
}

/// Whether the chat may use the model `model` now (spec 083 FR-006b): while "Standard" is active
/// every model but those taken out of it; while another profile is active only its members. A
/// value of the active profile this build cannot read counts as "Standard" here; the rights of
/// the turn are none then, so nothing is widened.
pub fn model_available(q: &mut impl Query, model: &str) -> haex_crdt::Result<bool> {
    let subject = Subject::Model(model.to_owned());
    let chain = store::chain(q, &subject)?;
    let memberships = store::effective_memberships(q, &subject)?;
    if memberships.unreadable {
        return Ok(false);
    }
    let active = match store::active(q)? {
        store::Active::Profile(id) if store::profile(q, id)?.is_some() => id,
        _ => STANDARD_PROFILE_ID,
    };
    for link in &chain {
        let expected_id = super::member_id(link, active);
        if store::member_subject(q, &expected_id)?.is_some_and(|stored| stored != link.key()) {
            return Ok(false);
        }
    }
    if active == STANDARD_PROFILE_ID {
        return Ok(!memberships.left_standard);
    }
    Ok(memberships.profiles.contains(&active))
}

/// The rights of `subject`: those of [`applying`], none when that cannot be read, and for
/// "Standard" without a row of its own the factory rights ([`default_rights`]). A rights row this
/// build cannot read is skipped by the converter, so it gives less, not more.
pub fn rights_of(q: &mut impl Query, subject: &Subject) -> haex_crdt::Result<Rights> {
    match applying(q, subject)? {
        Some(profile) => profile_rights(q, profile),
        None => Ok(Rights::default()),
    }
}

/// How the Abo-Agent's own tools run for the model `model` (spec 083 FR-030): the group `delegate`
/// of the profile that applies to it; a profile without the group, or one this build cannot read,
/// asks for every call.
pub fn delegate_rights(
    q: &mut impl Query,
    model: &str,
) -> haex_crdt::Result<crate::mcp_server::grants::DelegateRights> {
    Ok(rights_of(q, &Subject::Model(model.to_owned()))?
        .delegate
        .unwrap_or_else(crate::mcp_server::grants::DelegateRights::asking))
}

/// The profile that applies to `subject`, with its name: what a page shows next to an agent and
/// where a remembered answer is written. Follows [`rights_of`] except that something this build
/// cannot read shows "Standard" (the rights are none).
pub fn profile_of(q: &mut impl Query, subject: &Subject) -> haex_crdt::Result<(Uuid, String)> {
    if let Some(id) = applying(q, subject)? {
        if let Some(profile) = store::profile(q, id)? {
            return Ok((profile.id, profile.name));
        }
    }
    Ok((STANDARD_PROFILE_ID, store::STANDARD_NAME.to_owned()))
}

/// What `profile` would give an agent that is a member of it: its own rights while it is the active
/// profile, else those of "Standard". The registration question shows this for the profile the
/// user picks.
pub fn rights_if_member(q: &mut impl Query, profile: Uuid) -> haex_crdt::Result<Rights> {
    let active = match store::active(q)? {
        store::Active::Unset => STANDARD_PROFILE_ID,
        store::Active::Unreadable => return Ok(Rights::default()),
        store::Active::Profile(id) => id,
    };
    if profile == active || profile == STANDARD_PROFILE_ID {
        profile_rights(q, profile)
    } else {
        standard_rights(q)
    }
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
