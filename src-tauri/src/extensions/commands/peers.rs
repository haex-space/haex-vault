//! `extension_peer_invitation_answer`: holzi's window answers an invitation prompt (spec 057
//! T040, contracts/bridge.md §Befehle für holzis Fenster). Only holzi's own window calls it; an
//! extension has no way to accept or decline anything (FR-016).

use serde::Serialize;
use tauri::State;
use ts_rs::TS;
use uuid::Uuid;

use crate::error::Result;
use crate::extensions::peers::invited::Answered;
use crate::state::AppState;

/// What the answer found: the invitation was still open, or it had gone (withdrawn, expired,
/// taken by another device or answered already).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "lowercase")]
#[ts(export, export_to = "../../src/types/bindings/")]
pub enum ExtensionPeerInvitationResult {
    Open,
    Gone,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, TS)]
#[ts(export, export_to = "../../src/types/bindings/")]
pub struct ExtensionPeerInvitationAnswer {
    pub result: ExtensionPeerInvitationResult,
}

/// Takes the user's answer to the invitation `invite_id`. An id that is not one is gone.
#[tauri::command]
pub async fn extension_peer_invitation_answer(
    state: State<'_, AppState>,
    invite_id: String,
    accept: bool,
) -> Result<ExtensionPeerInvitationAnswer> {
    let answered = match Uuid::parse_str(&invite_id) {
        Ok(invite) => state.extensions().peers.answer_invitation(invite, accept),
        Err(_) => Answered::Gone,
    };
    Ok(ExtensionPeerInvitationAnswer {
        result: match answered {
            Answered::Open => ExtensionPeerInvitationResult::Open,
            Answered::Gone => ExtensionPeerInvitationResult::Gone,
        },
    })
}
