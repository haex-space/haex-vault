//! What an answer of "Für diese Website merken" changes (spec 083 FR-015): the rights group
//! `browser` of the agent profile of the agent, so it holds for every agent in that profile. The
//! one place the website, fill and request-rule answers of an external agent are written.

use haex_crdt::CrdtTransaction;
use uuid::Uuid;

use super::{resolve, store, Subject};
use crate::mcp_server::grants::BrowserRights;
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
    let (profile, _) = resolve::profile_of(tx, &Subject::Agent(agent))?;
    let mut rights = resolve::profile_rights(tx, profile)?;
    if let Some(browser) = rights.browser.as_mut() {
        if change(browser) {
            store::set_rights(tx, profile, &rights)?;
        }
    }
    Ok(())
}
