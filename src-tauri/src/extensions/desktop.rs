//! holzi's [`Desktop`] for extensions in the running app (spec 017, US8): the system's browser
//! through `tauri-plugin-opener`, and system notifications through the shared
//! [`SystemNotifier`](crate::notifications::system::SystemNotifier), whose clicks go back to the
//! extension's respond callback.

use std::sync::Arc;

use tauri::{AppHandle, Runtime};
use tauri_plugin_opener::OpenerExt;

use super::host::Desktop;
use super::notifications::{NotificationSpec, Respond, ShownNotification};
use crate::notifications::system::SystemNotifier;
use crate::notifications::Target;

pub struct AppDesktop<R: Runtime> {
    app: AppHandle<R>,
    notifier: Arc<SystemNotifier<R>>,
}

impl<R: Runtime> AppDesktop<R> {
    pub fn new(app: AppHandle<R>, notifier: Arc<SystemNotifier<R>>) -> Self {
        Self { app, notifier }
    }
}

impl<R: Runtime> Desktop for AppDesktop<R> {
    #[cfg(mobile)]
    fn notifications_allowed(&self) -> bool {
        self.notifier.allowed()
    }

    fn open_url(&self, url: &str) -> Result<(), String> {
        self.app
            .opener()
            .open_url(url, None::<&str>)
            .map_err(|e| e.to_string())
    }

    fn show_notification(
        &self,
        spec: &NotificationSpec,
        respond: Respond,
    ) -> Result<Box<dyn ShownNotification>, String> {
        self.notifier.show(spec, Target::Respond(respond))
    }
}
