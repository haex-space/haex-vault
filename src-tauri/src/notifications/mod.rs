//! holzi's system notifications, shared by extensions (spec 017 US8) and the chat with contacts
//! (spec 072 FR-023, research R17): what is shown, what the user did with it, and where a tap leads
//! ([`Target`]). [`system::SystemNotifier`] shows them through `tauri-plugin-notification` and keeps
//! the one registry of shown notifications that the plugin's clicks are routed through.

pub mod system;

/// Event to holzi's window: open the chat at a conversation, `{threadId}` (spec 072 FR-023).
pub const CHAT_OPEN: &str = "chat-notification-open";
/// Event to holzi's window where the system shows no notification (Android: the person refused
/// the permission, spec 043 FR-023): `{threadId, title, body?}`.
pub const CHAT_IN_APP: &str = "chat-notification-in-app";

/// A button of a notification.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NotificationButton {
    pub id: String,
    pub label: String,
}

/// What the system is asked to show.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NotificationSpec {
    pub title: String,
    pub body: Option<String>,
    /// The image and its file extension (`png`, `jpg`, …).
    pub icon: Option<(Vec<u8>, &'static str)>,
    pub buttons: Vec<NotificationButton>,
}

/// What the user did with a notification, as the system reports it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NotificationResponse {
    /// A click on the notification itself.
    Body,
    /// A click on the button with this id.
    Button(String),
    /// Closed without a click (by the user, by timeout or by holzi).
    Closed,
}

/// Called once with the response; from any thread.
pub type Respond = Box<dyn FnOnce(NotificationResponse) + Send>;

/// A notification the system shows; closing it removes it from the screen.
pub trait ShownNotification: Send {
    fn close(self: Box<Self>);
}

/// Where a notification's response goes.
pub enum Target {
    /// To whoever showed it (extensions decide themselves what a click means).
    Respond(Respond),
    /// A tap opens the chat at this conversation; closing it does nothing.
    Chat { thread_id: String },
}

impl Target {
    /// What `response` leads to: the respond callback is called, a chat tap names the
    /// conversation to open.
    pub fn answer(self, response: NotificationResponse) -> Option<String> {
        match self {
            Self::Respond(respond) => {
                respond(response);
                None
            }
            Self::Chat { thread_id } => {
                (response == NotificationResponse::Body).then_some(thread_id)
            }
        }
    }
}

#[cfg(test)]
#[path = "mod_tests.rs"]
mod tests;
