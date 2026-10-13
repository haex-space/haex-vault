//! The real effects of a close: the window, the event and the process (spec 013 T039).
//!
//! Every method logs a failure and returns; none can refuse the close (FR-002).

use tauri::{AppHandle, Manager, Url};

use crate::vault_gate::{ChildRegistry, CloseEffects, ClosePolicy};

use super::events::emit_instance_list_changed;

/// The static page that shows only a spinner (research R4). It sits at the root of the served
/// files, so an absolute path finds it from any route.
const CLOSING_PAGE: &str = "/closing.html";

/// Label of the one window the app opens (Tauri's default for a window without a label).
const MAIN_WINDOW: &str = "main";

/// Performs the close effects on a running app.
pub struct AppCloseEffects {
    app: AppHandle,
    children: ChildRegistry,
}

impl AppCloseEffects {
    pub fn new(app: AppHandle, children: ChildRegistry) -> Self {
        Self { app, children }
    }
}

impl CloseEffects for AppCloseEffects {
    fn show_closing_page(&self) {
        let Some(window) = self.app.get_webview_window(MAIN_WINDOW) else {
            log::warn!("no main window to show the closing page in");
            return;
        };
        let shown = window
            .url()
            .map_err(|error| error.to_string())
            .and_then(|current| {
                current
                    .join(CLOSING_PAGE)
                    .map_err(|error| error.to_string())
            })
            .and_then(|url| window.navigate(url).map_err(|error| error.to_string()));
        if let Err(error) = shown {
            // The page is only a spinner, so a blank one does the same job of clearing the screen.
            log::warn!("could not show the closing page, blanking the window: {error}");
            if let Ok(blank) = "about:blank".parse::<Url>() {
                if let Err(error) = window.navigate(blank) {
                    log::warn!("could not blank the window: {error}");
                }
            }
        }
    }

    fn announce_closed(&self, name: Option<String>) {
        emit_instance_list_changed(&self.app, "closed", name);
    }

    fn request_end(&self, policy: ClosePolicy) {
        match policy {
            ClosePolicy::Relaunch => self.app.request_restart(),
            ClosePolicy::Exit => self.app.exit(0),
        }
    }

    fn force_end(&self, policy: ClosePolicy) {
        // Ending the process skips every `Drop`, so the children have to be ended first.
        self.children.kill_all();
        match policy {
            ClosePolicy::Relaunch => tauri::process::restart(&self.app.env()),
            ClosePolicy::Exit => crate::platform::end_process(0),
        }
    }
}
