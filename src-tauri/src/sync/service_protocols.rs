//! The protocols the session's endpoint serves beside `holzi-sync/1`: the channels between
//! extensions (spec 057) and file access between devices (spec 056).

use std::sync::Arc;

use tauri::{AppHandle, Runtime};

use crate::error::Result;
use crate::extensions::host::ExtensionHost;
use crate::files::local::OwnPlaces;
use crate::files::peer::serve::ServeContext;
use crate::state::AppState;
use crate::sync::endpoint::Protocols;
use crate::sync::keys::DeviceKeys;
use crate::vault_gate::VaultDb;

/// The extension host whose channels the endpoint serves, and a handle on the open vault for
/// their lookups.
pub struct PeerDeps {
    pub host: Arc<ExtensionHost>,
    pub db: VaultDb,
}

/// A handle on the open vault, holzi's own places and this device's `vault_device_uuid`, for
/// `haex-vault-files/1`.
pub struct FilesDeps {
    pub db: VaultDb,
    pub own: OwnPlaces,
    pub device: String,
}

/// What serving files needs, when this device is known to its vault as `device`.
pub fn files_deps<R: Runtime>(
    app: &AppHandle<R>,
    state: &AppState,
    device: Option<uuid::Uuid>,
) -> Result<Option<FilesDeps>> {
    let Some(device) = device else {
        log::warn!("files: this device is not known to its vault, it serves no files");
        return Ok(None);
    };
    Ok(Some(FilesDeps {
        db: state.database()?,
        own: OwnPlaces::from_app(app),
        device: device.to_string(),
    }))
}

/// The handler of `haex-vault-ext-peer/1` for the endpoint (spec 057 research R1), when the session has
/// an extension host.
pub fn peer_protocols<R: Runtime>(
    peers: Option<PeerDeps>,
    keys: &DeviceKeys,
    vault: [u8; 32],
    app: &AppHandle<R>,
) -> (Protocols, Option<Arc<crate::extensions::peers::admission::PeerContext>>) {
    use crate::extensions::commands::frames::WindowEmitter;
    use crate::extensions::peers::admission::{PeerContext, PeerProtocol};
    use crate::extensions::peers::channel::Frames;

    let Some(PeerDeps { host, db }) = peers else {
        return (Protocols::default(), None);
    };
    let ctx = Arc::new(PeerContext {
        keys: keys.clone(),
        vault,
        db,
        frames: Frames::new(Arc::new(WindowEmitter(app.clone())), &host),
    });
    let handler = Arc::clone(&ctx);
    let protocols = Protocols::default().with(crate::sync::wire::EXT_PEER_ALPN, move |tasks| {
        PeerProtocol::new(handler, tasks)
    });
    (protocols, Some(ctx))
}

/// `protocols` with the handler of `haex-vault-files/1` (spec 056 research R5), when the session
/// serves files, and what its own links to other devices prove themselves with.
pub fn file_protocols(
    protocols: Protocols,
    files: Option<FilesDeps>,
    keys: &DeviceKeys,
    vault: [u8; 32],
) -> (Protocols, Option<Arc<ServeContext>>) {
    use crate::files::peer::serve::FilesProtocol;

    let Some(FilesDeps { db, own, device }) = files else {
        return (protocols, None);
    };
    let ctx = Arc::new(ServeContext::new(keys.clone(), vault, db, own, device));
    let handler = Arc::clone(&ctx);
    let protocols = protocols.with(crate::sync::wire::FILES_ALPN, move |tasks| {
        FilesProtocol::new(handler, tasks)
    });
    (protocols, Some(ctx))
}
