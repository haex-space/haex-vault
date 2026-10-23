//! Ends channels whose ground went away while they were open (research R11, R12; FR-008,
//! FR-034): the permission `peers` of their extension, once it is denied, or the other vault, now
//! blocked or no longer a contact. It follows the vault's changes like the registry does; a subscriber that
//! fell behind checks everything.
//!
//! The other ways an extension stops (its last frame closed, disabled or removed here or on
//! another device) end its channels where the host ends its shells (`PeerState::end_all`).

use std::sync::Arc;

use tauri::{AppHandle, Runtime};
use tokio::sync::broadcast::error::RecvError;
use uuid::Uuid;

use super::FEATURE;
use crate::contacts::service::{Caller, ContactsService, VaultCard};
use crate::extensions::host::ExtensionHost;
use crate::extensions::permissions::store::{candidates, TABLES};
use crate::extensions::permissions::{
    evaluate, Action, Decision, PermissionKind, PermissionRequest, RequestTarget,
};
use crate::state::AppState;
use crate::storage::wm_session_commands::current_device_uuid;
use crate::sync::foreign_auth::{Trust, Verified};
use crate::vault_gate::VaultDb;

/// The address book tables whose change can take a vault's ground away.
const CONTACT_TABLES: [&str; 2] = ["haex_contacts_blocked_vaults", "haex_contacts_cards"];

/// What a change asks to check again.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Recheck {
    pub permissions: bool,
    pub contacts: bool,
}

impl Recheck {
    pub const ALL: Recheck = Recheck {
        permissions: true,
        contacts: true,
    };

    fn of(tables: &[String]) -> Self {
        let touched = |names: &[&str]| tables.iter().any(|t| names.contains(&t.as_str()));
        Self {
            permissions: touched(&TABLES),
            contacts: touched(&CONTACT_TABLES),
        }
    }

    fn any(self) -> bool {
        self.permissions || self.contacts
    }
}

/// Follows the vault session `state` just published until it ends. A failure to start is logged;
/// the bridge still checks the permission on every call.
pub fn start_for_active_instance<R: Runtime>(app: &AppHandle<R>, state: &AppState) {
    let db = match state.database() {
        Ok(db) => db,
        Err(error) => {
            log::warn!("peers: no open vault, channels are not watched: {error}");
            return;
        }
    };
    let device = match current_device_uuid(app, &db) {
        Ok(device) => device,
        Err(error) => {
            log::warn!("peers: this device is unknown, channels are not watched: {error}");
            return;
        }
    };
    let mut changes = state.vault_changes().subscribe();
    let host = state.extensions();
    let token = state.gate().token();
    let watch = async move {
        loop {
            let due = tokio::select! {
                () = token.cancelled() => return,
                received = changes.recv() => match received {
                    Ok(tables) => Recheck::of(&tables),
                    Err(RecvError::Lagged(_)) => Recheck::ALL,
                    Err(RecvError::Closed) => return,
                },
            };
            if due.any() {
                recheck(&db, &host, device, due).await;
            }
        }
    };
    if let Err(error) = state.gate().spawn(watch) {
        log::warn!("peers: watching the channels did not start: {error}");
    }
}

/// Ends what lost its ground among the open channels of `host` on `device`. A failed read ends
/// nothing; the next change or bridge call checks again.
pub async fn recheck(db: &VaultDb, host: &Arc<ExtensionHost>, device: Uuid, due: Recheck) {
    if due.permissions {
        for extension_id in host.peers.extensions() {
            match peers_denied(db, host, extension_id, device).await {
                Ok(false) => {}
                Ok(true) => host.peers.end_all(extension_id),
                Err(error) => log::warn!("peers: a permission could not be read: {error}"),
            }
        }
    }
    if due.contacts {
        let contacts = ContactsService::new(db.clone());
        for (channel_id, remote) in host.peers.remotes() {
            match grounded(&contacts, &remote).await {
                Some(true) => {}
                Some(false) => host.peers.end_where(|channel| channel.id == channel_id),
                None => return,
            }
        }
        // An invitation that waits for an answer needs the same ground as the channel it would
        // become.
        for (invite, remote) in host.peers.invitation_remotes() {
            match grounded(&contacts, &remote).await {
                Some(true) => {}
                Some(false) => host.peers.end_invitation(invite),
                None => return,
            }
        }
    }
}

/// Whether the other side still stands where it was admitted: a contact whose card is still on
/// the same contact and whose vault is not blocked. `None` when the address book could not be
/// read.
async fn grounded(contacts: &ContactsService, remote: &Verified) -> Option<bool> {
    let caller = Caller::Internal { feature: FEATURE };
    let card = match contacts.card_of_vault(&caller, remote.vault).await {
        Ok(card) => card,
        Err(error) => {
            log::warn!("peers: the address book could not be read: {error}");
            return None;
        }
    };
    Some(match (&remote.trust, card) {
        (_, VaultCard::Blocked) => false,
        (
            Trust::Contact { contact_id, .. },
            VaultCard::Stored {
                contact_id: now, ..
            },
        ) => *contact_id == now,
        (Trust::Contact { .. }, VaultCard::Unknown) => false,
        (Trust::OwnVault | Trust::Unverified, _) => true,
    })
}

/// Whether `peers`/`connect` on `*` is denied for `extension_id`, remembered or held for the
/// session, as a bridge call would find it. An undecided permission is no denial: a channel that
/// an invitation opened for an extension nobody has asked yet stays, and the bridge asks when the
/// extension first uses it.
async fn peers_denied(
    db: &VaultDb,
    host: &ExtensionHost,
    extension_id: Uuid,
    device: Uuid,
) -> crate::error::Result<bool> {
    let mut grants = db
        .read(move |q| {
            candidates(q, extension_id, PermissionKind::Peers, device).map_err(Into::into)
        })
        .await?;
    grants.extend(
        host.permissions
            .temporary(extension_id, PermissionKind::Peers),
    );
    let request = PermissionRequest {
        kind: PermissionKind::Peers,
        action: Action::Connect,
        target: RequestTarget::Any,
    };
    Ok(evaluate(&grants, &request, device) == Decision::Deny)
}

#[cfg(test)]
#[path = "watch_tests.rs"]
mod tests;
