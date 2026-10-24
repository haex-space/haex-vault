//! `extension_peer_invitation_answer`: holzi's window answers an invitation prompt (spec 057
//! T040, contracts/bridge.md §Befehle für holzis Fenster). Only holzi's own window calls it; an
//! extension has no way to accept or decline anything (FR-016).

use serde::Serialize;
use tauri::State;
use ts_rs::TS;
use uuid::Uuid;

use crate::error::Result;
use crate::extensions::host::ExtensionHost;
use crate::extensions::peers::channel::EndReason;
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

/// A frame's queue had to drop a reliable `peers:message` of `channel_id` (the frame did not get
/// ready): the channel ends with `lost` on both sides (FR-038). A frame that is gone, an id that
/// is no id and a channel of another extension end nothing.
pub fn frame_overflow(host: &ExtensionHost, frame: &str, channel_id: &str) {
    let (Some(session), Ok(channel)) = (host.frames.get(frame), Uuid::parse_str(channel_id)) else {
        return;
    };
    let _ = host
        .peers
        .end(session.extension_id, channel, EndReason::Lost);
}

/// holzi's window reports the overflow of a frame's queue (`FrameEventQueue`, spec 057 T066).
#[tauri::command]
pub async fn extension_peers_frame_overflow(
    state: State<'_, AppState>,
    frame: String,
    channel_id: String,
) -> Result<()> {
    frame_overflow(&state.extensions(), &frame, &channel_id);
    Ok(())
}

#[cfg(test)]
#[path = "peers_tests.rs"]
mod tests;
