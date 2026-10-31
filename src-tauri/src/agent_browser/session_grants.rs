//! The submit grants of a session (research R5): a submit the user allowed lets exactly its next
//! changing request of that tab go, and a new tab the action opens may take that grant for its
//! first document instead.

use std::sync::PoisonError;

use super::Session;

#[derive(Default)]
pub(super) struct SubmitGrants {
    next_id: u64,
    grants: Vec<SubmitGrant>,
}

struct SubmitGrant {
    id: u64,
    tab: String,
    available: bool,
    pending_new_tab: bool,
    disarm_requested: bool,
}

impl Session {
    /// The user allowed an action of tab `tab` that submits. A new tab opened by that action may
    /// reserve the grant until its first document decision completes.
    pub fn arm(&self, tab: &str) {
        let mut armed = self.armed.lock().unwrap_or_else(PoisonError::into_inner);
        let id = armed.next_id;
        armed.next_id = armed.next_id.wrapping_add(1);
        armed.grants.push(SubmitGrant {
            id,
            tab: tab.to_owned(),
            available: true,
            pending_new_tab: false,
            disarm_requested: false,
        });
    }

    pub fn disarm(&self) {
        let mut armed = self.armed.lock().unwrap_or_else(PoisonError::into_inner);
        if let Some(grant) = armed
            .grants
            .iter_mut()
            .rev()
            .find(|grant| !grant.disarm_requested)
        {
            grant.disarm_requested = true;
        }
        Self::clean_submit_grants(&mut armed);
    }

    pub(super) fn take_armed(&self, tab: &str) -> bool {
        let mut armed = self.armed.lock().unwrap_or_else(PoisonError::into_inner);
        let Some(grant) = armed
            .grants
            .iter_mut()
            .rev()
            .find(|grant| grant.tab == tab && grant.available && !grant.disarm_requested)
        else {
            return false;
        };
        grant.available = false;
        Self::clean_submit_grants(&mut armed);
        true
    }

    /// Reserves the active submit grant for the first new tab this opener creates.
    pub(in crate::agent_browser) fn reserve_submit_for_new_tab(&self, opener: &str) -> Option<u64> {
        let mut armed = self.armed.lock().unwrap_or_else(PoisonError::into_inner);
        let grant = armed.grants.iter_mut().rev().find(|grant| {
            grant.tab == opener
                && grant.available
                && !grant.pending_new_tab
                && !grant.disarm_requested
        })?;
        grant.pending_new_tab = true;
        Some(grant.id)
    }

    pub(super) fn take_pending_armed(&self, id: u64) -> bool {
        let mut armed = self.armed.lock().unwrap_or_else(PoisonError::into_inner);
        let Some(grant) = armed
            .grants
            .iter_mut()
            .find(|grant| grant.id == id && grant.available && grant.pending_new_tab)
        else {
            return false;
        };
        grant.available = false;
        Self::clean_submit_grants(&mut armed);
        true
    }

    pub(in crate::agent_browser) fn finish_pending_new_tab(&self, id: Option<u64>) {
        let Some(id) = id else {
            return;
        };
        let mut armed = self.armed.lock().unwrap_or_else(PoisonError::into_inner);
        if let Some(grant) = armed.grants.iter_mut().find(|grant| grant.id == id) {
            grant.pending_new_tab = false;
        }
        Self::clean_submit_grants(&mut armed);
    }

    fn clean_submit_grants(armed: &mut SubmitGrants) {
        armed
            .grants
            .retain(|grant| grant.pending_new_tab || (grant.available && !grant.disarm_requested));
    }
}
