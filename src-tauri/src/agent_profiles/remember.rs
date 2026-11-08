//! What an answer of "Für diese Website merken" changes (spec 083 FR-015): the rights group
//! `browser` of the agent profile of the agent, so it holds for every agent in that profile. The
//! one place the website, fill and request-rule answers and the switch-on of the browser are
//! written, for an external agent and for the local agent alike.

use haex_crdt::CrdtTransaction;
use uuid::Uuid;

use super::{resolve, store, Subject, STANDARD_PROFILE_ID};
use crate::mcp_server::grants::{BrowserRights, Rights};
use crate::mcp_server::registry;

/// Applies `change` to the browser rights of the profile `agent` is in and stores them when it
/// says it changed something; "Standard" gets its row with the first change. An agent that has no
/// browser right changes nothing: the question would not have been asked.
pub fn change_browser(
    tx: &mut CrdtTransaction<'_>,
    device: Uuid,
    agent: Uuid,
    change: impl FnOnce(&mut BrowserRights) -> bool,
) -> haex_crdt::Result<()> {
    if registry::load_rights(tx, device, agent)?.browser.is_none() {
        return Ok(());
    }
    let subject = registry::subject_of_id(tx, device, agent)?;
    change_browser_of(tx, Some(&subject), change)
}

/// [`change_browser`] for any subject; `None` is the local agent outside a turn, in "Standard". A
/// profile without the browser group is left alone.
pub fn change_browser_of(
    tx: &mut CrdtTransaction<'_>,
    subject: Option<&Subject>,
    change: impl FnOnce(&mut BrowserRights) -> bool,
) -> haex_crdt::Result<()> {
    let profile = profile_of(tx, subject)?;
    let mut rights = resolve::profile_rights(tx, profile)?;
    if let Some(browser) = rights.browser.as_mut() {
        if change(browser) {
            store::set_rights(tx, profile, &rights)?;
        }
    }
    Ok(())
}

/// Switches the browser group on in the profile of `subject` when it is off: every website may be
/// operated, as a group switched on by hand (spec 065 FR-040, FR-043). Already on: nothing.
pub fn switch_browser_on(
    tx: &mut CrdtTransaction<'_>,
    subject: Option<&Subject>,
) -> haex_crdt::Result<()> {
    let profile = profile_of(tx, subject)?;
    let mut rights = resolve::profile_rights(tx, profile)?;
    if rights.browser.is_none() {
        rights.browser = Some(BrowserRights::switched_on());
        store::set_rights(tx, profile, &rights)?;
    }
    Ok(())
}

/// Remembers an answer about the address book or files (spec 083 FR-032, FR-033): applies `change`
/// to the rights of the profile of `subject` (`None` is the local agent outside a turn, in
/// "Standard"). `expected` is the profile the question was asked in; when the subject's profile is
/// another one by now (the profile was switched while the question was open) nothing is written and
/// the answer is `false`.
pub fn change_rights_of(
    tx: &mut CrdtTransaction<'_>,
    subject: Option<&Subject>,
    expected: Uuid,
    change: impl FnOnce(&mut Rights),
) -> haex_crdt::Result<bool> {
    let profile = profile_of(tx, subject)?;
    if profile != expected {
        return Ok(false);
    }
    let mut rights = resolve::profile_rights(tx, profile)?;
    change(&mut rights);
    store::set_rights(tx, profile, &rights)?;
    Ok(true)
}

/// The profile an answer for `subject` would be written to; what [`change_contacts_of`] expects.
pub fn profile_to_remember(
    q: &mut impl crate::storage::query::Query,
    subject: Option<&Subject>,
) -> haex_crdt::Result<Uuid> {
    match subject {
        Some(subject) => resolve::profile_for_remember(q, subject)?.ok_or_else(|| {
            haex_crdt::Error::consumer("cannot remember a permission for an unreadable profile")
        }),
        None => Ok(STANDARD_PROFILE_ID),
    }
}

fn profile_of(tx: &mut CrdtTransaction<'_>, subject: Option<&Subject>) -> haex_crdt::Result<Uuid> {
    match subject {
        Some(subject) => resolve::profile_for_remember(tx, subject)?.ok_or_else(|| {
            haex_crdt::Error::consumer("cannot remember a permission for an unreadable profile")
        }),
        None => Ok(STANDARD_PROFILE_ID),
    }
}
