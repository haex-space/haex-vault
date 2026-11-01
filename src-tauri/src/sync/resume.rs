//! Brings the sync of the open vault up to date when holzi comes back to the foreground or the
//! network changes (spec 043, FR-018, FR-019, research R5).
//!
//! Android stops a backgrounded app's sockets without telling it, and iroh does not see Android's
//! network changes from native code (`Endpoint::network_change` says so). Nothing here syncs in the
//! background (FR-018): a [`Wake`] only shortcuts what the session would otherwise do on its next
//! tick, so a change made elsewhere arrives within seconds instead of a minute (SC-005). Both wakes
//! rebuild the Nostr relay connections, which also republishes this device's presence, and run the
//! reconnect pass of [`crate::sync::reconnect_missing`]; a network change also tells iroh.

use std::sync::Arc;

use tauri::{AppHandle, Manager, Runtime};
use tokio::sync::Notify;
use tokio_util::sync::CancellationToken;

use crate::sync::registry::SyncRegistry;

/// Why the session should catch up now.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Wake {
    /// The app came back to the foreground.
    Resumed,
    /// The device changed networks (Wi-Fi to mobile data, or back).
    NetworkChanged,
}

/// The loops of one sync session a wake reaches. Each [`Notify`] keeps at most one pending wake, so
/// wakes in quick succession collapse into one pass.
#[derive(Clone)]
pub struct Wakeups {
    /// The reconnect loop: dials devices presence knows but this session is not connected to.
    pub reconnect: Arc<Notify>,
    /// The presence loop: rebuilds its relay connections and publishes this device's presence.
    pub relays: Arc<Notify>,
    /// The network loop: tells iroh the network changed.
    pub network: Arc<Notify>,
    /// The session's own token; once the close has started a wake does nothing.
    pub cancel: CancellationToken,
}

impl Wakeups {
    pub fn new(cancel: CancellationToken) -> Self {
        Self {
            reconnect: Arc::new(Notify::new()),
            relays: Arc::new(Notify::new()),
            network: Arc::new(Notify::new()),
            cancel,
        }
    }

    /// Wakes the session's loops; `false` when the session is closing.
    pub fn wake(&self, wake: Wake) -> bool {
        if self.cancel.is_cancelled() {
            return false;
        }
        if wake == Wake::NetworkChanged {
            self.network.notify_one();
        }
        self.relays.notify_one();
        self.reconnect.notify_one();
        true
    }
}

/// Wakes the running session, if there is one; `false` without an open vault or while it closes.
pub fn wake_registry(registry: &SyncRegistry, wake: Wake) -> bool {
    registry
        .get()
        .is_some_and(|runtime| runtime.wakeups.wake(wake))
}

/// Wakes the sync of the open vault of `app`.
pub fn wake_app<R: Runtime>(app: &AppHandle<R>, wake: Wake) {
    let Some(registry) = app.try_state::<Arc<SyncRegistry>>() else {
        return;
    };
    if wake_registry(&registry, wake) {
        log::info!("sync: {wake:?}, catching up");
    }
    // Spec 072: the chat's own Nostr connections may be dead just the same.
    if let Some(chat) = app.try_state::<Arc<crate::chat::peer::session::PeerRegistry>>() {
        chat.wake();
    }
}

/// Wakes the sync on every network change the platform reports; on a desktop none is reported,
/// since iroh notices those by itself.
pub fn watch_network<R: Runtime>(app: &AppHandle<R>) {
    use tauri_plugin_holzi_android::HolziAndroidExt;

    let handle = app.clone();
    let watched = app
        .holzi_android()
        .watch_network(move || wake_app(&handle, Wake::NetworkChanged));
    if let Err(error) = watched {
        log::warn!("sync: network changes not watched: {error}");
    }
}

#[cfg(test)]
#[path = "resume_tests.rs"]
mod tests;
