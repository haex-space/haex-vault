//! Generic OS-notification bridge for extensions.
//!
//! Extensions can't fire OS notifications themselves (no host privileges), so
//! they call `extension_notifications_show`. The host fires the notification
//! via `tauri-plugin-notification` and records it in an in-memory registry,
//! pinned to the calling extension's id (derived from its public key). A click
//! can therefore only ever route back to the SAME extension — there is no
//! cross-extension deep linking.
//!
//! ## Platform reality
//!
//! `tauri-plugin-notification` 2.x only renders title/body/icon on **desktop**
//! and does NOT report clicks or action taps back to Rust there (it ignores the
//! builder's `extra`/actions). So on desktop the notification is shown, but the
//! click → deep-link routing in [`route_notification_click`] has no trigger yet.
//! On mobile, action types and click events are supported by the plugin and can
//! drive [`route_notification_click`]. The routing half is implemented and
//! correct; only the desktop *click source* is missing (documented limitation).

pub mod commands;
pub mod types;

use std::collections::HashMap;
use std::sync::Mutex;

use types::{DeepLink, NotificationAction};

/// Tauri event delivered to an extension's webview when one of its
/// notifications is clicked.
pub const NOTIFICATION_CLICK_EVENT: &str = "haextension:notification:click";

/// One registered notification, pinned to the extension that created it.
#[derive(Debug, Clone)]
pub struct NotificationRecord {
    /// Owning extension id (resolved from the public key at `show()` time).
    pub extension_id: String,
    pub primary: Option<DeepLink>,
    pub actions: Vec<NotificationAction>,
    pub tag: Option<String>,
}

/// In-memory, session-lifetime registry of shown notifications, keyed by id.
///
/// Clicks on notifications from a previous session (the map is empty after a
/// restart) become no-ops — acceptable and even desirable across upgrades.
#[derive(Default)]
pub struct NotificationRegistry {
    inner: Mutex<HashMap<String, NotificationRecord>>,
}

impl NotificationRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    /// Insert a record. If `tag` is set, any existing record from the SAME
    /// extension with the SAME tag is dropped first (logical replace — the OS
    /// notification itself can't be replaced through the plugin on desktop).
    pub fn insert(&self, id: String, record: NotificationRecord) {
        let mut map = self.inner.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(tag) = record.tag.clone() {
            map.retain(|_, r| {
                !(r.extension_id == record.extension_id && r.tag.as_deref() == Some(tag.as_str()))
            });
        }
        map.insert(id, record);
    }

    pub fn get(&self, id: &str) -> Option<NotificationRecord> {
        let map = self.inner.lock().unwrap_or_else(|e| e.into_inner());
        map.get(id).cloned()
    }

    /// Remove a record only if it belongs to `extension_id`. Returns `true`
    /// when something was removed.
    pub fn remove_if_owned(&self, id: &str, extension_id: &str) -> bool {
        let mut map = self.inner.lock().unwrap_or_else(|e| e.into_inner());
        match map.get(id) {
            Some(r) if r.extension_id == extension_id => {
                map.remove(id);
                true
            }
            _ => false,
        }
    }
}

/// Resolve a notification click and deliver it to the owning extension's
/// webview as a [`NOTIFICATION_CLICK_EVENT`].
///
/// The notification is looked up by id; its pinned `extension_id` decides the
/// target webview, so a click can never route to a different extension. An
/// unknown id (e.g. from a previous session) is a no-op.
///
/// NOTE: nothing calls this on desktop yet — `tauri-plugin-notification` 2.x
/// does not report desktop clicks. This is the complete routing half, ready to
/// be wired to a click source (mobile action events, or a future backend).
#[allow(dead_code)]
pub fn route_notification_click(
    app_handle: &tauri::AppHandle,
    state: &crate::AppState,
    notification_id: &str,
    action_id: Option<String>,
) {
    let Some(record) = state.notifications.get(notification_id) else {
        return;
    };

    let path = match &action_id {
        Some(aid) => record
            .actions
            .iter()
            .find(|a| &a.id == aid)
            .map(|a| a.deep_link.path.clone()),
        None => record.primary.as_ref().map(|d| d.path.clone()),
    };

    let payload = types::NotificationClickPayload {
        notification_id: notification_id.to_string(),
        action_id,
        path,
    };

    #[cfg(not(any(target_os = "android", target_os = "ios")))]
    {
        let _ = state.extension_webview_manager.emit_to_extension_or_main(
            app_handle,
            &record.extension_id,
            NOTIFICATION_CLICK_EVENT,
            payload,
        );
    }
    #[cfg(any(target_os = "android", target_os = "ios"))]
    {
        use tauri::Emitter;
        let _ = app_handle.emit_to("main", NOTIFICATION_CLICK_EVENT, payload);
    }
}
