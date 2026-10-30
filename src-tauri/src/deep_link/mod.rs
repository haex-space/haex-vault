//! Links of the form `haex-vault://contact/…` and `mailto:…` opened outside holzi (spec 060
//! FR-018a, research R9; spec 063 research R6).
//! The plugin hands the link to the process: on Android and macOS as an event, on Linux and
//! Windows as the argument of a **new** process. [`inbox`] gets such a link to the process that
//! has the vault open. The window then shows a preview only, nothing is saved before the user
//! confirms (FR-018a).

pub mod inbox;

use std::path::PathBuf;
use std::time::Duration;

use tauri::{AppHandle, Emitter, Manager};

use crate::error::Result;
use crate::instances::paths::get_app_local_data;
use crate::openers::mail;
use crate::state::AppState;

/// Sent to the window with the link as payload when another process passed one on.
pub const RECEIVED_EVENT: &str = "deep-link-received";

/// What a link of a share starts with; the host `contact` is the only one holzi answers to.
pub const LINK_PREFIX: &str = "haex-vault://contact/";

/// More than a share can be (spec 060 FR-018: 16,384 characters of text) is not a share.
const MAX_LINK_LEN: usize = 16 * 1024;

/// How often a process with an open vault looks into the inbox.
const WATCH_INTERVAL: Duration = Duration::from_millis(500);

/// `url` itself when it is a link of a share or a mail link, else nothing: other schemes, other
/// hosts and absurdly long texts are ignored. Each kind keeps its own limit; the window reads the
/// link further (`mailto:` by [`mail::parse`]).
pub fn known_link(url: &str) -> Option<&str> {
    let share =
        url.starts_with(LINK_PREFIX) && url.len() > LINK_PREFIX.len() && url.len() <= MAX_LINK_LEN;
    let mail = url
        .get(..mail::SCHEME.len())
        .is_some_and(|scheme| scheme.eq_ignore_ascii_case(mail::SCHEME))
        && url.len() <= mail::MAX_LINK_BYTES;
    (share || mail).then_some(url)
}

/// The first known link among `urls` (the arguments or events a start brings).
pub fn first_known_link(urls: impl IntoIterator<Item = String>) -> Option<String> {
    urls.into_iter().find(|url| known_link(url).is_some())
}

fn inbox_dir(app: &AppHandle) -> Result<PathBuf> {
    Ok(get_app_local_data(app)?.join(inbox::INBOX_DIRECTORY))
}

/// Registers the scheme with the system where holzi is started from a build directory, so a
/// link opens the binary being developed. Installed builds register through their package. Only
/// `haex-vault`: `register_all` would also make holzi the program for `mailto` on a developer's
/// machine, and holzi never makes itself the default (spec 063 FR-016).
#[cfg(any(target_os = "linux", windows))]
pub fn register_for_development(app: &AppHandle) {
    use tauri_plugin_deep_link::DeepLinkExt;
    if cfg!(debug_assertions) {
        if let Err(error) = app.deep_link().register("haex-vault") {
            log::warn!("deep link: could not register the scheme: {error}");
        }
    }
}

/// A process the system started with a link as argument (Linux, Windows): when other holzi
/// processes run, the link goes to their inbox, and a process that another took it from has
/// nothing left to do. Returns `true` then, and the caller ends the process. Otherwise the
/// plugin keeps the link for the window (`getCurrent`).
#[cfg(desktop)]
pub fn handed_over_at_start(app: &AppHandle, was_alone: bool) -> bool {
    use tauri_plugin_deep_link::DeepLinkExt;
    if was_alone {
        return false;
    }
    let current = match app.deep_link().get_current() {
        Ok(current) => current.unwrap_or_default(),
        Err(error) => {
            log::warn!("deep link: could not read the link of the start: {error}");
            return false;
        }
    };
    let Some(url) = first_known_link(current.into_iter().map(String::from)) else {
        return false;
    };
    let Ok(dir) = inbox_dir(app) else {
        return false;
    };
    matches!(inbox::offer(&dir, &url), Ok(inbox::Offered::Taken))
}

/// macOS delivers a link to the process the system reaches, whatever vault it has open. One
/// without an open vault offers it to the others too; the window of this process still keeps it
/// until the user opens a vault here, so it is never lost (research R9).
pub fn watch_opened_links(app: &AppHandle) {
    use tauri_plugin_deep_link::DeepLinkExt;
    if !cfg!(target_os = "macos") {
        return;
    }
    let app_for_event = app.clone();
    app.deep_link().on_open_url(move |event| {
        let Some(url) = first_known_link(event.urls().into_iter().map(String::from)) else {
            return;
        };
        if app_for_event.state::<AppState>().database().is_ok() {
            return;
        }
        let Ok(dir) = inbox_dir(&app_for_event) else {
            return;
        };
        std::thread::spawn(move || {
            if let Err(error) = inbox::offer(&dir, &url) {
                log::warn!("deep link: could not offer the link: {error}");
            }
        });
    });
}

/// Looks into the inbox while the vault is open and passes what another process left to the
/// window, which comes to the front. Started wherever a vault session starts (`instances/open.rs`
/// and `instances/import.rs`).
pub fn start_for_active_instance(app: &AppHandle) {
    let Ok(dir) = inbox_dir(app) else {
        return;
    };
    let state = app.state::<AppState>();
    let gate = state.gate().clone();
    let token = gate.token();
    let app = app.clone();
    let follow = async move {
        loop {
            tokio::select! {
                () = token.cancelled() => return,
                () = tokio::time::sleep(WATCH_INTERVAL) => {}
            }
            let dir = dir.clone();
            let focused = window_is_focused(&app);
            let claimed =
                tokio::task::spawn_blocking(move || inbox::claim_next(&dir, focused)).await;
            match claimed {
                Ok(Ok(Some(url))) => pass_on(&app, &url),
                Ok(Ok(None)) => {}
                Ok(Err(error)) => log::warn!("deep link: could not read the inbox: {error}"),
                Err(error) => log::warn!("deep link: the inbox check failed: {error}"),
            }
        }
    };
    if let Err(error) = gate.spawn(follow) {
        log::warn!("deep link: links of other processes will not arrive: {error}");
    }
}

/// Whether the window of this process has the focus: the process the person last used takes a
/// link from outside first (spec 063 research R6).
fn window_is_focused(app: &AppHandle) -> bool {
    app.webview_windows()
        .into_values()
        .any(|window| window.is_focused().unwrap_or(false))
}

fn pass_on(app: &AppHandle, url: &str) {
    let Some(url) = known_link(url) else {
        return;
    };
    if let Err(error) = app.emit(RECEIVED_EVENT, url) {
        log::warn!("deep link: could not tell the window: {error}");
    }
    if let Some(window) = app.webview_windows().into_values().next() {
        #[cfg(desktop)]
        let _ = window.unminimize();
        let _ = window.show();
        let _ = window.set_focus();
    }
}

#[cfg(test)]
#[path = "mod_tests.rs"]
mod mod_tests;
