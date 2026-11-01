//! System notifications of new contact messages (spec 072 FR-023, research R17): while the vault
//! is open, a message from another vault in a conversation that is not muted and not open in a
//! focused window shows one, if this device's `chat.notifications` is on. A request shows only
//! "Neue Anfrage", never its text. A tap opens the chat at the conversation; where the system shows
//! none (Android: permission refused), holzi's window shows it as a message.

use std::sync::Arc;

use haex_crdt::rusqlite::params;
use tauri::{AppHandle, Emitter, Manager, Runtime};
use tokio::sync::broadcast;
use tokio_util::sync::CancellationToken;
use uuid::Uuid;

use super::receive::Stored;
use super::session::PeerRegistry;
use super::settings;
use super::store::preview;
use super::verify::vault_bytes;
use crate::contacts::card::received;
use crate::error::Result;
use crate::notifications::system::SystemNotifier;
use crate::notifications::{NotificationSpec, ShownNotification, Target, CHAT_IN_APP};
use crate::state::AppState;
use crate::storage::query::Query;
use crate::vault_gate::VaultDb;

/// Notifications of the chat open at most; a new one closes the oldest. Where the system reports
/// no close, holzi cannot tell when one is gone.
const MAX_OPEN: usize = 20;

/// What a notification of a new message shows.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Notice {
    pub thread_id: String,
    pub title: String,
    pub body: Option<String>,
}

/// The notice for `stored` on `device`, or `None` when none is due: not new, from an own device,
/// muted, or notifications off. Whether the conversation is open is the caller's part.
pub fn notice(
    q: &mut impl Query,
    device: Uuid,
    stored: &Stored,
    locale: &str,
) -> Result<Option<Notice>> {
    if !stored.new || !stored.incoming || !settings::notifications(q, device)? {
        return Ok(None);
    }
    let thread_id = stored.thread_id.to_string();
    let thread: Option<(String, i64)> = q.query_row(
        "SELECT state, muted FROM chat_threads WHERE id = ?1",
        params![thread_id],
        |row| Ok((row.get(0)?, row.get(1)?)),
    )?;
    let Some((state, muted)) = thread else {
        return Ok(None);
    };
    if muted != 0 {
        return Ok(None);
    }
    if state == "request" {
        return Ok(Some(Notice {
            thread_id,
            title: text(locale, "chat.notify.request"),
            body: None,
        }));
    }
    let message: Option<Option<String>> = q.query_row(
        "SELECT text FROM chat_peer_messages WHERE id = ?1",
        params![stored.message_id.to_string()],
        |row| row.get(0),
    )?;
    Ok(Some(Notice {
        thread_id,
        title: sender_name(q, &stored.peer)?
            .unwrap_or_else(|| text(locale, "chat.peer.unknownName")),
        body: message.flatten().map(|text| preview(&text)),
    }))
}

/// The contact's name in the address book, else the name on the stored card.
fn sender_name(q: &mut impl Query, vault: &str) -> Result<Option<String>> {
    let Some(vault) = vault_bytes(vault) else {
        return Ok(None);
    };
    let Some((contact_id, card)) = received::stored(q, &vault)? else {
        return Ok(None);
    };
    let name = crate::contacts::store::load(q, &contact_id)
        .map(|contact| contact.name)
        .unwrap_or_default();
    Ok(Some(if name.trim().is_empty() {
        card.card.name
    } else {
        name
    }))
}

fn text(locale: &str, key: &str) -> String {
    crate::window_texts::text(locale, key, &[])
}

/// Shows the notices of the messages `arrivals` reports until `closing`: one per conversation, a
/// newer message replaces the conversation's notification.
pub async fn run<R: Runtime>(
    app: AppHandle<R>,
    db: VaultDb,
    registry: Arc<PeerRegistry>,
    device: Uuid,
    mut arrivals: broadcast::Receiver<Stored>,
    closing: CancellationToken,
) {
    let mut open: Vec<(String, Box<dyn ShownNotification>)> = Vec::new();
    loop {
        let stored = tokio::select! {
            () = closing.cancelled() => return,
            received = arrivals.recv() => match received {
                Ok(stored) => stored,
                // Missed ones are not shown late: the list's unread count holds them.
                Err(broadcast::error::RecvError::Lagged(_)) => continue,
                Err(broadcast::error::RecvError::Closed) => return,
            },
        };
        if registry.is_viewing(&stored.thread_id.to_string()) {
            continue;
        }
        let locale = app
            .try_state::<AppState>()
            .map(|state| state.extensions().context().locale)
            .unwrap_or_default();
        let found = db
            .read(move |r| Ok(notice(r, device, &stored, &locale)?))
            .await;
        match found {
            Ok(Some(notice)) => {
                let thread_id = notice.thread_id.clone();
                if let Some(at) = open.iter().position(|(id, _)| *id == thread_id) {
                    open.remove(at).1.close();
                }
                if let Some(shown) = show(&app, notice).await {
                    open.push((thread_id, shown));
                    if open.len() > MAX_OPEN {
                        open.remove(0).1.close();
                    }
                }
            }
            Ok(None) => {}
            Err(error) => log::warn!("chat: no notification of a new message: {error}"),
        }
    }
}

/// Shows `notice` as a system notification, else in holzi's window.
async fn show<R: Runtime>(
    app: &AppHandle<R>,
    notice: Notice,
) -> Option<Box<dyn ShownNotification>> {
    let in_app = serde_json::json!({
        "threadId": notice.thread_id,
        "title": notice.title,
        "body": notice.body,
    });
    let Some(notifier) = app.try_state::<Arc<SystemNotifier<R>>>() else {
        let _ = app.emit(CHAT_IN_APP, in_app);
        return None;
    };
    let notifier = Arc::clone(&notifier);
    // The system calls are blocking (Android asks for its permission the first time).
    let shown = tokio::task::spawn_blocking(move || {
        let spec = NotificationSpec {
            title: notice.title,
            body: notice.body,
            icon: None,
            buttons: Vec::new(),
        };
        let target = Target::Chat {
            thread_id: notice.thread_id,
        };
        notifier.allowed().then(|| notifier.show(&spec, target))
    })
    .await;
    match shown {
        Ok(Some(Ok(shown))) => return Some(shown),
        Ok(Some(Err(error))) => log::warn!("chat: the notification could not be shown: {error}"),
        Ok(None) | Err(_) => {}
    }
    let _ = app.emit(CHAT_IN_APP, in_app);
    None
}

#[cfg(test)]
#[path = "notify_tests.rs"]
mod tests;
