//! The handler of `haex-vault-files/1` on the session endpoint (spec 056 research R5, R6): every
//! connection runs on the session's tracker, proves itself (`handshake`), and then sends one request
//! per bi-stream. Every request is checked again (FR-005): the switch for an own device, the card
//! and the grant for a contact, the path and holzi's own places for both. A refusal looks the same
//! from outside whatever its reason (FR-042).

pub mod check;
pub mod ops;
pub mod roots;

use std::sync::Arc;
use std::time::Duration;

use iroh::endpoint::{Connection, RecvStream, SendStream};
use iroh::protocol::{AcceptError, ProtocolHandler};
use tokio::sync::Semaphore;

use self::roots::Located;
use crate::files::local::OwnPlaces;
use crate::files::peer::handshake;
use crate::files::peer::wire::{
    read_head, write_head, FailCode, Mode, Op, Reply, Request, MAX_LIST, VERSION,
};
use crate::files::share::store::{self, FileShareRow};
use crate::files::share::{device_access, FileShareLevel, FEATURE};
use crate::files::transfer::local::Removal;
use crate::remote_storage::local::LocalStore;
use crate::sync::endpoint::NodeTasks;
use crate::sync::foreign_auth::{Local, SeenGenerations};
use crate::sync::keys::DeviceKeys;
use crate::vault_gate::VaultDb;

/// Most requests one connection runs at once (research R6).
pub const STREAMS_PER_CONNECTION: usize = 16;
/// Most requests this device serves at once, over all connections (SC-006).
pub const STREAMS_PER_DEVICE: usize = 64;
/// How long a stream may take to send its request.
const REQUEST_LIMIT: Duration = Duration::from_secs(10);
/// How long a refused device gets to read the end of the handshake and close.
const REFUSAL_GRACE: Duration = Duration::from_secs(2);

/// What serving needs of this device.
pub struct ServeContext {
    pub keys: DeviceKeys,
    pub vault: [u8; 32],
    pub db: VaultDb,
    /// holzi's own places on this device (FR-007).
    pub own: OwnPlaces,
    /// This device's `vault_device_uuid`: the shares it serves are its own (FR-018).
    pub device: String,
    /// The highest device-list generations of other vaults this session saw (FR-003).
    pub seen: SeenGenerations,
    pub store: LocalStore,
    /// Whether deleting here goes to a trash (research R8).
    pub trash: bool,
    slots: Arc<Semaphore>,
}

impl ServeContext {
    pub fn new(
        keys: DeviceKeys,
        vault: [u8; 32],
        db: VaultDb,
        own: OwnPlaces,
        device: String,
    ) -> Self {
        Self {
            keys,
            vault,
            db,
            own,
            device,
            seen: SeenGenerations::default(),
            store: LocalStore::new(),
            trash: matches!(crate::files::transfer::platform_removal(), Removal::Trash),
            slots: Arc::new(Semaphore::new(STREAMS_PER_DEVICE)),
        }
    }

    /// This device as [`crate::sync::foreign_auth`] presents it.
    pub fn local(&self) -> Local<'_> {
        Local {
            keys: &self.keys,
            vault: self.vault,
            db: &self.db,
            feature: FEATURE,
            seen: &self.seen,
        }
    }
}

/// The handler the router calls for every connection on `haex-vault-files/1`.
#[derive(Clone)]
pub struct FilesProtocol {
    ctx: Arc<ServeContext>,
    tasks: NodeTasks,
}

impl std::fmt::Debug for FilesProtocol {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("FilesProtocol").finish_non_exhaustive()
    }
}

impl FilesProtocol {
    pub fn new(ctx: Arc<ServeContext>, tasks: NodeTasks) -> Self {
        Self { ctx, tasks }
    }
}

impl ProtocolHandler for FilesProtocol {
    async fn accept(&self, connection: Connection) -> Result<(), AcceptError> {
        // Tracked like the sync sessions: the router drops this future at its shutdown, the
        // node's shutdown waits for the tracker (FR-044).
        let (ctx, tasks) = (Arc::clone(&self.ctx), self.tasks.clone());
        self.tasks.tracker.spawn(serve(ctx, connection, tasks));
        Ok(())
    }
}

/// The other side of one connection, as the handshake found it.
struct Peer {
    mode: Mode,
    vault: [u8; 32],
}

async fn serve(ctx: Arc<ServeContext>, connection: Connection, tasks: NodeTasks) {
    let Some((verified, mode)) =
        handshake::accept(&ctx.local(), &ctx.db, &ctx.device, ctx.trash, &connection).await
    else {
        // What this side wrote last must reach the other side before the connection closes, or it
        // sees a lost connection instead of a refusal; it closes when it has read it.
        tokio::select! {
            () = tasks.cancel.cancelled() => {}
            _ = tokio::time::timeout(REFUSAL_GRACE, connection.closed()) => {}
        }
        connection.close(0u32.into(), b"");
        return;
    };
    let peer = Arc::new(Peer {
        mode,
        vault: verified.vault,
    });
    let streams = Arc::new(Semaphore::new(STREAMS_PER_CONNECTION));
    loop {
        let (send, recv) = tokio::select! {
            () = tasks.cancel.cancelled() => break,
            stream = connection.accept_bi() => match stream {
                Ok(stream) => stream,
                Err(_) => break,
            },
        };
        let permits = (
            Arc::clone(&streams).try_acquire_owned(),
            Arc::clone(&ctx.slots).try_acquire_owned(),
        );
        let (ctx, peer, cancel) = (Arc::clone(&ctx), Arc::clone(&peer), tasks.cancel.clone());
        tasks.tracker.spawn(async move {
            match permits {
                (Ok(_stream), Ok(_slot)) => {
                    tokio::select! {
                        () = cancel.cancelled() => {}
                        () = handle(&ctx, &peer, send, recv) => {}
                    }
                }
                _ => answer(send, Reply::Failed(FailCode::Busy)).await,
            }
        });
    }
    connection.close(0u32.into(), b"");
}

async fn handle(ctx: &ServeContext, peer: &Peer, mut send: SendStream, mut recv: RecvStream) {
    let request: Request = match tokio::time::timeout(REQUEST_LIMIT, read_head(&mut recv)).await {
        Ok(Ok(request)) => request,
        _ => return,
    };
    if request.v != VERSION {
        return answer(send, Reply::Failed(FailCode::Unsupported)).await;
    }
    if let Op::Read {
        share,
        path,
        start,
        len,
    } = &request.op
    {
        let opened = async {
            let at = locate(ctx, peer, &request, share.as_deref(), path).await?;
            ops::open_read(ctx, &at, *start, *len).await
        };
        match opened.await {
            Ok((size, len, mut reader)) => {
                if write_head(&mut send, &Reply::Reading { size, len })
                    .await
                    .is_ok()
                    && tokio::io::copy(&mut reader, &mut send).await.is_ok()
                {
                    let _ = send.finish();
                }
            }
            Err(code) => answer(send, Reply::Failed(code)).await,
        }
        return;
    }
    let reply = match reply_to(ctx, peer, &request).await {
        Ok(reply) => reply,
        Err(code) => Reply::Failed(code),
    };
    answer(send, reply).await;
}

async fn reply_to(ctx: &ServeContext, peer: &Peer, request: &Request) -> Result<Reply, FailCode> {
    match &request.op {
        Op::Shares => {
            permit(ctx, peer, request, None).await?;
            if peer.mode == Mode::Contact {
                Ok(Reply::Shares(ops::shares(ctx, peer.vault).await?))
            } else {
                // The roots of an own device come with the delivery for own devices.
                Err(FailCode::Unsupported)
            }
        }
        Op::ListDir { share, path, max } => {
            let at = locate(ctx, peer, request, share.as_deref(), path).await?;
            let max = (*max).min(MAX_LIST) as usize;
            let hide_own = check::hides_own(peer.mode, request.caller);
            Ok(Reply::Entries(ops::list(ctx, &at, max, hide_own).await?))
        }
        Op::Stat { share, path } => {
            let at = locate(ctx, peer, request, share.as_deref(), path).await?;
            Ok(Reply::Entry(ops::stat(ctx, &at).await?))
        }
        Op::Read { .. } => Err(FailCode::Internal),
    }
}

/// Checks `request` against what holds now (FR-005) and returns the share it names with the level
/// of the asking vault.
async fn permit(
    ctx: &ServeContext,
    peer: &Peer,
    request: &Request,
    share: Option<&str>,
) -> Result<Option<(FileShareRow, FileShareLevel)>, FailCode> {
    let grant = match peer.mode {
        Mode::OwnDevice => {
            let on = ctx
                .db
                .read(|q| Ok(device_access::enabled(q)?))
                .await
                .map_err(|_| FailCode::Internal)?;
            if !on {
                return Err(FailCode::Refused);
            }
            None
        }
        Mode::AccessOff => None,
        Mode::Contact => {
            if !handshake::still_contact(&ctx.db, peer.vault).await {
                return Err(FailCode::Refused);
            }
            match share {
                Some(id) => grant_of(ctx, peer.vault, id).await?,
                None => None,
            }
        }
    };
    check::allowed(
        peer.mode,
        &request.op,
        grant.as_ref().map(|(row, level)| (row, *level)),
    )?;
    Ok(grant)
}

/// The share `id` if this device serves it and `vault` holds a grant in it.
async fn grant_of(
    ctx: &ServeContext,
    vault: [u8; 32],
    id: &str,
) -> Result<Option<(FileShareRow, FileShareLevel)>, FailCode> {
    let (id, device, vault) = (
        id.to_owned(),
        ctx.device.clone(),
        crate::sync::keys::hex(&vault),
    );
    ctx.db
        .read(move |q| {
            let Some(row) = store::get_share(q, &id)? else {
                return Ok(None);
            };
            if row.vault_device_uuid != device {
                return Ok(None);
            }
            Ok(store::grant_for(q, &id, &vault)?.map(|level| (row, level)))
        })
        .await
        .map_err(|_| FailCode::Internal)
}

/// The target of `path` after every check: the request may run, the path stays in its root, and
/// holzi's own places are kept as FR-007 says.
async fn locate(
    ctx: &ServeContext,
    peer: &Peer,
    request: &Request,
    share: Option<&str>,
    path: &str,
) -> Result<Located, FailCode> {
    let grant = permit(ctx, peer, request, share).await?;
    let at = match (peer.mode, &grant) {
        (Mode::OwnDevice, None) => roots::on_device(path)?,
        (Mode::Contact, Some((row, _))) => roots::in_share(row, path)?,
        _ => return Err(FailCode::Refused),
    };
    let real = roots::real(&at.path())?;
    let own = ctx.own.contains(&real) || ctx.own.contains_resolved(&at.path());
    check::own_data(peer.mode, request.caller, request.op.writes(), own)?;
    Ok(at)
}

async fn answer(mut send: SendStream, reply: Reply) {
    if write_head(&mut send, &reply).await.is_ok() {
        let _ = send.finish();
    }
}

#[cfg(test)]
#[path = "serve_tests.rs"]
mod tests;
