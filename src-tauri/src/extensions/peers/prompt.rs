//! holzi's own invitation prompt (spec 057 T039, FR-014, FR-015, research R9): "[contact]
//! invites you to [extension]" with accept and decline. The extension is not told before the
//! user accepted here (FR-016), and the prompt does not depend on its `notifications`
//! permission.
//!
//! With a frame of the extension open the prompt is an event to holzi's window; without one it is
//! a system notification with two buttons, answered here in Rust. A click on the notification
//! itself brings the window prompt, and a system that shows no notification gets the window
//! prompt too. When the invitation goes away the prompt goes with it ([`Prompt::gone`]).

use std::sync::Arc;

use serde::Serialize;
use serde_json::Value;
use ts_rs::TS;
use uuid::Uuid;

use super::channel::Frames;
use crate::extensions::host::ExtensionHost;
use crate::extensions::notifications::{
    NotificationButton, NotificationResponse, NotificationSpec, Respond, ShownNotification,
};

/// The invitation, for holzi's window.
pub const INVITATION: &str = "extension-peer-invitation";
/// The invitation went away, for holzi's window.
pub const INVITATION_GONE: &str = "extension-peer-invitation-gone";

const ACCEPT: &str = "accept";
const DECLINE: &str = "decline";

/// What the window needs to ask (contracts/bridge.md §Befehle für holzis Fenster). `contact_name`
/// is the name from the address book of this vault and never goes to an extension.
#[derive(Debug, Clone, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/types/bindings/")]
pub struct ExtensionPeerInvitation {
    pub invite_id: String,
    pub extension_id: String,
    pub extension_name: String,
    pub contact_name: String,
    /// The user clicked the notification: the window comes forward with the prompt.
    pub raise: bool,
}

/// Why an invitation went away: its sender withdrew it or the connection ended, or its time ran
/// out here.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "lowercase")]
#[ts(export, export_to = "../../src/types/bindings/")]
pub enum ExtensionPeerInvitationGoneReason {
    Withdrawn,
    Expired,
}

#[derive(Debug, Clone, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/types/bindings/")]
pub struct ExtensionPeerInvitationGone {
    pub invite_id: String,
    pub reason: ExtensionPeerInvitationGoneReason,
}

/// What the user is asked about.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PromptInfo {
    pub invite: Uuid,
    pub extension_id: Uuid,
    pub extension_name: String,
    pub contact_name: String,
}

impl PromptInfo {
    fn event(&self, raise: bool) -> Value {
        serde_json::to_value(ExtensionPeerInvitation {
            invite_id: self.invite.to_string(),
            extension_id: self.extension_id.to_string(),
            extension_name: self.extension_name.clone(),
            contact_name: self.contact_name.clone(),
            raise,
        })
        .unwrap_or(Value::Null)
    }
}

/// What the user chose, as the notification's buttons report it.
pub type Answer = Arc<dyn Fn(bool) + Send + Sync>;

/// An invitation put in front of the user. Dropping it removes the notification, if one was
/// shown; [`Self::gone`] also takes the prompt out of the window.
pub struct Prompt {
    info: Arc<PromptInfo>,
    frames: Frames,
    shown: Option<Box<dyn ShownNotification>>,
}

impl Prompt {
    /// The invitation went away: the window drops its prompt and the notification disappears.
    pub fn gone(&mut self, reason: ExtensionPeerInvitationGoneReason) {
        self.frames.emit_window(
            INVITATION_GONE,
            serde_json::to_value(ExtensionPeerInvitationGone {
                invite_id: self.info.invite.to_string(),
                reason,
            })
            .unwrap_or(Value::Null),
        );
        self.close();
    }

    fn close(&mut self) {
        if let Some(shown) = self.shown.take() {
            shown.close();
        }
    }
}

impl Drop for Prompt {
    fn drop(&mut self) {
        self.close();
    }
}

/// Shows the prompt for `info`: in the window while a frame of the extension is open, else as a
/// notification, else, when the system shows none, in the window as well. `answer` hears the
/// notification's buttons.
pub async fn present(
    host: &Arc<ExtensionHost>,
    frames: &Frames,
    info: Arc<PromptInfo>,
    answer: Answer,
) -> Prompt {
    let mut prompt = Prompt {
        info: Arc::clone(&info),
        frames: frames.clone(),
        shown: None,
    };
    if !host.frames.of_extension(info.extension_id).is_empty() {
        frames.emit_window(INVITATION, info.event(false));
        return prompt;
    }
    let Some(desktop) = host.desktop() else {
        frames.emit_window(INVITATION, info.event(false));
        return prompt;
    };
    let locale = host.context().locale;
    let spec = NotificationSpec {
        title: text(
            &locale,
            "title",
            &[
                ("contact", &info.contact_name),
                ("extension", &info.extension_name),
            ],
        ),
        body: None,
        icon: None,
        buttons: vec![
            NotificationButton {
                id: ACCEPT.to_owned(),
                label: text(&locale, ACCEPT, &[]),
            },
            NotificationButton {
                id: DECLINE.to_owned(),
                label: text(&locale, DECLINE, &[]),
            },
        ],
    };
    let respond = respond(frames.clone(), Arc::clone(&info), answer);
    // The system calls are blocking (Android asks for its permission the first time).
    let shown = tokio::task::spawn_blocking(move || {
        desktop
            .notifications_allowed()
            .then(|| desktop.show_notification(&spec, respond))
    })
    .await;
    match shown {
        Ok(Some(Ok(shown))) => prompt.shown = Some(shown),
        Ok(Some(Err(error))) => {
            log::warn!("peers: the invitation could not be shown as a notification: {error}");
            frames.emit_window(INVITATION, info.event(false));
        }
        Ok(None) | Err(_) => frames.emit_window(INVITATION, info.event(false)),
    }
    prompt
}

/// What a click on the notification does: a button answers; the notification itself brings the
/// window prompt; closing it decides nothing, the invitation stays until it expires.
fn respond(frames: Frames, info: Arc<PromptInfo>, answer: Answer) -> Respond {
    Box::new(move |response| match response {
        NotificationResponse::Button(button) if button == ACCEPT => answer(true),
        NotificationResponse::Button(button) if button == DECLINE => answer(false),
        NotificationResponse::Button(_) | NotificationResponse::Closed => {}
        NotificationResponse::Body => frames.emit_window(INVITATION, info.event(true)),
    })
}

/// `extensions.peers.invitation.<key>` in the language of holzi's window, with `{name}`
/// placeholders filled; the key itself when it is missing.
fn text(locale: &str, key: &str, values: &[(&str, &str)]) -> String {
    let full = format!("extensions.peers.invitation.{key}");
    let found = crate::window_texts::text(locale, &full, values);
    if found == full {
        key.to_owned()
    } else {
        found
    }
}

#[cfg(test)]
#[path = "prompt_tests.rs"]
mod tests;
