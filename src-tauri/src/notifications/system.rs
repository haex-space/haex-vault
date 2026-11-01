//! System notifications through `tauri-plugin-notification` on every platform. Clicks come back
//! through the plugin's `on_action` (spec 017 research R20, T099); the plugin is pinned to an
//! upstream commit that reports them on desktop too, until a release after 2.5.1 contains it
//! (Cargo.toml `[patch.crates-io]`). One [`SystemNotifier`] per process (managed state) holds the
//! shown notifications, so every click finds its [`Target`].

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicI32, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};

use sha2::{Digest, Sha256};
use tauri::{AppHandle, Emitter, Manager, Runtime};
#[cfg(mobile)]
use tauri_plugin_notification::PermissionState;
use tauri_plugin_notification::{Action, ActionType, NotificationExt};

use super::{NotificationResponse, NotificationSpec, ShownNotification, Target, CHAT_OPEN};
use crate::sync::keys::hex;

/// The plugin's action id of a click on the notification itself.
const TAP: &str = "tap";
/// Buttons get their own key space, so a button named `tap` stays a button.
const BUTTON: &str = "button:";

/// A shown notification that waits for its response, and its buttons.
struct Waiting {
    target: Target,
    buttons: Option<ActionType>,
}

#[derive(Default)]
struct Shown {
    waiting: Mutex<HashMap<i32, Waiting>>,
}

impl Shown {
    fn lock(&self) -> MutexGuard<'_, HashMap<i32, Waiting>> {
        self.waiting.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// The action types of every waiting notification: iOS replaces the whole set on each
    /// registration, so it is always registered complete.
    fn action_types(&self) -> Vec<ActionType> {
        self.lock()
            .values()
            .filter_map(|w| w.buttons.clone())
            .collect()
    }
}

pub struct SystemNotifier<R: Runtime> {
    app: AppHandle<R>,
    shown: Arc<Shown>,
    next_id: AtomicI32,
}

impl<R: Runtime> SystemNotifier<R> {
    /// Also starts listening to the plugin's actions; call it once, after the plugin was added.
    pub fn new(app: AppHandle<R>) -> Self {
        let shown = Arc::new(Shown::default());
        let waiting = Arc::clone(&shown);
        let window = app.clone();
        let listening = app.notification().on_action(move |performed| {
            let Some(id) = performed.notification().map(|n| n.id()) else {
                return;
            };
            let Some(entry) = waiting.lock().remove(&id) else {
                return;
            };
            if let Some(thread_id) = entry.target.answer(response(performed.action_id())) {
                let payload = serde_json::json!({ "threadId": thread_id });
                if let Err(error) = window.emit(CHAT_OPEN, payload) {
                    log::warn!("notifications: the chat was not opened: {error}");
                }
            }
        });
        if let Err(error) = listening {
            log::warn!("notifications: clicks are not reported: {error}");
        }
        Self {
            app,
            shown,
            next_id: AtomicI32::new(1),
        }
    }

    /// Android 13 and later ask the person once (spec 043 FR-023, research R12); a refusal holds
    /// until they change it in the system's settings. Notifications at the same time wait for
    /// the one question, since Android drops a second request while one is open. Blocking.
    #[cfg(mobile)]
    pub fn allowed(&self) -> bool {
        static ASKING: Mutex<()> = Mutex::new(());
        let _asking = ASKING.lock().unwrap_or_else(PoisonError::into_inner);
        let notifications = self.app.notification();
        // `PromptWithRationale` means the person refused once: that answer holds.
        let state = match notifications.permission_state() {
            Ok(PermissionState::Prompt) => notifications.request_permission(),
            other => other,
        };
        match state {
            Ok(state) => state == PermissionState::Granted,
            Err(error) => {
                log::warn!("notifications: the permission is unknown: {error}");
                false
            }
        }
    }

    /// Desktop systems need no permission of holzi's own.
    #[cfg(not(mobile))]
    pub fn allowed(&self) -> bool {
        true
    }

    /// Shows `spec`; a click or close goes to `target`, if the system reports it.
    pub fn show(
        &self,
        spec: &NotificationSpec,
        target: Target,
    ) -> Result<Box<dyn ShownNotification>, String> {
        let id = self.next_id.fetch_add(1, Ordering::Relaxed);
        let notifications = self.app.notification();
        let mut builder = notifications.builder().id(id).title(&spec.title);
        if let Some(body) = &spec.body {
            builder = builder.body(body);
        }
        let icons = self.app.path().app_cache_dir().ok();
        if let Some(path) = spec.icon.as_ref().and_then(|(bytes, extension)| {
            icon_file(&icons?.join("notification-icons"), bytes, extension)
        }) {
            builder = builder.icon(path.to_string_lossy());
        }
        let buttons = (!spec.buttons.is_empty()).then(|| {
            ActionType::builder(format!("holzi-{id}"))
                .actions(
                    spec.buttons
                        .iter()
                        .map(|b| Action::builder(format!("{BUTTON}{}", b.id), &b.label).build())
                        .collect(),
                )
                .build()
        });
        if let Some(buttons) = &buttons {
            builder = builder.action_type_id(buttons.id());
        }
        self.shown.lock().insert(id, Waiting { target, buttons });
        let shown = notifications
            .register_action_types(self.shown.action_types())
            .and_then(|()| builder.show());
        if let Err(error) = shown {
            self.shown.lock().remove(&id);
            return Err(error.to_string());
        }
        Ok(Box::new(PluginNotification {
            app: self.app.clone(),
            shown: Arc::clone(&self.shown),
            id,
        }))
    }
}

/// What an action of the plugin means. Besides `tap` and the buttons, Android and iOS report
/// `dismiss` when the user swipes the notification away: that is no click.
fn response(action_id: &str) -> NotificationResponse {
    match action_id {
        TAP => NotificationResponse::Body,
        action => match action.strip_prefix(BUTTON) {
            Some(button) => NotificationResponse::Button(button.to_owned()),
            None => NotificationResponse::Closed,
        },
    }
}

/// The icon as a file the notification system can read, named by its content, in holzi's own
/// cache (not the shared temp directory, where another user could place the folder first). It is
/// written under a name of its own and renamed, so a notification showing the same icon at the
/// same time never reads half a file and a crash leaves no truncated one behind.
fn icon_file(dir: &Path, bytes: &[u8], extension: &str) -> Option<PathBuf> {
    std::fs::create_dir_all(dir).ok()?;
    let path = dir.join(format!("{}.{extension}", hex(&Sha256::digest(bytes))));
    if !path.exists() {
        let partial = dir.join(format!("{}.partial", uuid::Uuid::new_v4()));
        std::fs::write(&partial, bytes).ok()?;
        if std::fs::rename(&partial, &path).is_err() {
            let _ = std::fs::remove_file(&partial);
            return None;
        }
    }
    Some(path)
}

/// A notification of the plugin; removing it also forgets its response.
struct PluginNotification<R: Runtime> {
    app: AppHandle<R>,
    shown: Arc<Shown>,
    id: i32,
}

impl<R: Runtime> ShownNotification for PluginNotification<R> {
    fn close(self: Box<Self>) {
        self.shown.lock().remove(&self.id);
        if let Err(error) = self.app.notification().remove_active(vec![self.id]) {
            log::warn!("notifications: one could not be removed: {error}");
        }
    }
}

#[cfg(test)]
#[path = "system_tests.rs"]
mod tests;
