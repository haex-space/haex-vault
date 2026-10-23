//! What commands can reach of the running sync service (spec 024).
//!
//! The service owns the bound [`SyncNode`]; a command that needs it (linking
//! a device, later the device list and the status) finds it here while the
//! vault session lasts. The registry is app state, set when the service has
//! bound and cleared when it ends.

use std::sync::{Arc, Mutex};

use tokio_util::sync::CancellationToken;

use crate::sync::endpoint::SyncNode;
use crate::sync::keys::DeviceKeys;
use crate::sync::link::host_task::LinkHost;
use crate::sync::replica::Replica;

/// The running service of the open vault.
pub struct SyncRuntime {
    pub node: Arc<SyncNode>,
    pub replica: Arc<Replica>,
    pub keys: DeviceKeys,
    pub vault: [u8; 32],
    /// The Nostr relays presence uses; a link's rendezvous uses them too.
    pub nostr_relays: Vec<String>,
    /// Ends with the vault session.
    pub cancel: CancellationToken,
    pub link: LinkHost,
    /// Wakes the session and presence as a local commit does, for changes
    /// that bypass `VaultDb` (a device list the link just published).
    pub wake: Arc<dyn Fn() + Send + Sync>,
    /// Catching up after the app comes back or the network changes (spec 043).
    pub wakeups: crate::sync::resume::Wakeups,
    /// This session's links to other devices for their files (spec 056), when it serves files.
    pub files: Option<Arc<crate::files::peer::connect::Connections>>,
}

/// The slot the service publishes its runtime in.
#[derive(Default)]
pub struct SyncRegistry {
    runtime: Mutex<Option<Arc<SyncRuntime>>>,
}

impl SyncRegistry {
    pub fn set(&self, runtime: Arc<SyncRuntime>) {
        *self.runtime.lock().unwrap_or_else(|e| e.into_inner()) = Some(runtime);
    }

    pub fn clear(&self) {
        *self.runtime.lock().unwrap_or_else(|e| e.into_inner()) = None;
    }

    /// The runtime of the open vault, if its service is up.
    pub fn get(&self) -> Option<Arc<SyncRuntime>> {
        self.runtime
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .clone()
    }
}
