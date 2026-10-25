//! The handler of `haex-vault-files/1` on the session endpoint (spec 056 research R5, R6): every
//! connection runs on the session's tracker, proves itself (`handshake`), and then sends one request
//! per bi-stream. Every request is checked again (FR-005): the switch for an own device, the card
//! and the grant for a contact, the path and holzi's own places for both. A refusal looks the same
//! from outside whatever its reason (FR-042).

pub mod change;
pub mod check;
pub mod ops;
mod permit;
pub mod roots;
pub mod streams;
pub mod upload;
pub mod watch;

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use iroh::endpoint::{Connection, RecvStream, SendStream};
use iroh::protocol::{AcceptError, ProtocolHandler};
use tokio::io::AsyncReadExt;
use tokio::sync::Semaphore;
use tokio_util::sync::CancellationToken;

use self::permit::{again, grant_of, locate, permit};
use self::streams::{StreamKey, Streams};
use self::upload::Uploads;
use crate::files::local::{OwnPlaces, Place};
use crate::files::peer::handshake;
use crate::files::peer::wire::{
    read_head, write_head, FailCode, Mode, Op, Reply, Request, MAX_LIST, MAX_PART,
    STREAM_CANCELLED, VERSION,
};
use crate::files::share::FEATURE;
use crate::files::transfer::local::Removal;
use crate::remote_storage::local::LocalStore;
use crate::sync::endpoint::NodeTasks;
use crate::sync::foreign_auth::{Local, SeenGenerations};
use crate::sync::keys::DeviceKeys;
use crate::vault_events::{changes_channel, VaultChanges};
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
    /// The known places an own device starts in (home, documents, …).
    pub known: Vec<Place>,
    /// This device's `vault_device_uuid`: the shares it serves are its own (FR-018).
    pub device: String,
    /// The highest device-list generations of other vaults this session saw (FR-003).
    pub seen: SeenGenerations,
    pub store: LocalStore,
    /// How deleting removes here: into the trash on desktops (research R8).
    pub removal: Removal,
    /// The open vault's changes, which [`crate::files::share::served`] follows.
    pub changes: VaultChanges,
    /// The running requests of contacts, cancelled by a revoke (FR-024).
    pub streams: Streams,
    /// Locks peer writes to one target while checking and replacing it.
    write_locks: Mutex<HashMap<PathBuf, Arc<tokio::sync::Mutex<()>>>>,
    slots: Arc<Semaphore>,
}

impl ServeContext {
    pub fn new(
        keys: DeviceKeys,
        vault: [u8; 32],
        db: VaultDb,
        own: OwnPlaces,
        known: Vec<Place>,
        device: String,
    ) -> Self {
        Self {
            keys,
            vault,
            db,
            own,
            known,
            device,
            seen: SeenGenerations::default(),
            store: LocalStore::new(),
            removal: crate::files::transfer::platform_removal(),
            changes: changes_channel(),
            streams: Streams::default(),
            write_locks: Mutex::new(HashMap::new()),
            slots: Arc::new(Semaphore::new(STREAMS_PER_DEVICE)),
        }
    }

    fn write_lock(&self, path: &Path) -> Arc<tokio::sync::Mutex<()>> {
        let mut locks = self.write_locks.lock().unwrap_or_else(|e| e.into_inner());
        Arc::clone(
            locks
                .entry(path.to_owned())
                .or_insert_with(|| Arc::new(tokio::sync::Mutex::new(()))),
        )
    }

    /// Whether deleting here goes to a trash.
    pub fn trash(&self) -> bool {
        self.removal == Removal::Trash
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
    uploads: Uploads,
}

async fn serve(ctx: Arc<ServeContext>, connection: Connection, tasks: NodeTasks) {
    let Some((verified, mode)) =
        handshake::accept(&ctx.local(), &ctx.db, &ctx.device, ctx.trash(), &connection).await
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
        uploads: Uploads::default(),
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
    // Uploads belong to the connection: nothing half stays behind it (FR-044).
    peer.uploads.abort_all(&ctx).await;
}

async fn handle(ctx: &ServeContext, peer: &Peer, mut send: SendStream, mut recv: RecvStream) {
    let request: Request = match tokio::time::timeout(REQUEST_LIMIT, read_head(&mut recv)).await {
        Ok(Ok(request)) => request,
        _ => return,
    };
    if request.v != VERSION {
        return answer(send, Reply::Failed(FailCode::Unsupported)).await;
    }
    let cancel = stream_token(ctx, peer, &request.op).await;
    match &request.op {
        Op::Read {
            share,
            path,
            start,
            len,
        } => {
            let opened = async {
                let at = locate(ctx, peer, &request, share.as_deref(), path).await?;
                ops::open_read(ctx, &at, *start, *len).await
            };
            match opened.await {
                Ok((size, len, mut reader)) => {
                    if write_head(&mut send, &Reply::Reading { size, len })
                        .await
                        .is_err()
                    {
                        return;
                    }
                    tokio::select! {
                        copied = tokio::io::copy(&mut reader, &mut send) => if copied.is_ok() {
                            let _ = send.finish();
                        },
                        () = cancel.cancelled() => {
                            let _ = send.reset(STREAM_CANCELLED.into());
                        }
                    }
                }
                Err(code) => answer(send, Reply::Failed(code)).await,
            }
        }
        Op::Watch { share, path } => {
            let at = match locate(ctx, peer, &request, share.as_deref(), path).await {
                Ok(at) => at,
                Err(code) => return answer(send, Reply::Failed(code)).await,
            };
            let still = || async {
                locate(ctx, peer, &request, share.as_deref(), path)
                    .await
                    .is_ok()
            };
            watch::run(&at, send, still, &cancel).await;
        }
        Op::UploadPart {
            upload_id,
            number,
            len,
        } => {
            let reply = tokio::select! {
                reply = part(ctx, peer, &request, upload_id, *number, *len, &mut recv) => reply,
                () = cancel.cancelled() => {
                    // Nothing half stays behind a revoke (FR-024).
                    let _ = peer.uploads.abort(ctx, upload_id).await;
                    Err(FailCode::Refused)
                }
            };
            answer(send, reply.unwrap_or_else(Reply::Failed)).await;
        }
        _ => {
            let reply = reply_to(ctx, peer, &request).await;
            answer(send, reply.unwrap_or_else(Reply::Failed)).await;
        }
    }
}

async fn reply_to(ctx: &ServeContext, peer: &Peer, request: &Request) -> Result<Reply, FailCode> {
    match &request.op {
        Op::Shares => {
            permit(ctx, peer, request, None).await?;
            if peer.mode == Mode::Contact {
                Ok(Reply::Shares(ops::shares(ctx, peer.vault).await?))
            } else {
                ops::roots(ctx).await
            }
        }
        Op::ListDir { share, path, max } => {
            if peer.mode == Mode::OwnDevice && cfg!(windows) && roots::parts(path)?.is_empty() {
                permit(ctx, peer, request, None).await?;
                return Ok(Reply::Entries(ops::drive_entries().await?));
            }
            let at = locate(ctx, peer, request, share.as_deref(), path).await?;
            let max = (*max).min(MAX_LIST) as usize;
            let hide_own = check::hides_own(peer.mode, request.caller);
            Ok(Reply::Entries(ops::list(ctx, &at, max, hide_own).await?))
        }
        Op::Stat { share, path } => {
            let at = locate(ctx, peer, request, share.as_deref(), path).await?;
            Ok(Reply::Entry(ops::stat(ctx, &at).await?))
        }
        Op::BeginUpload {
            share,
            path,
            expected,
        } => {
            let at = locate(ctx, peer, request, share.as_deref(), path).await?;
            let upload_id = peer
                .uploads
                .begin(ctx, at, share.clone(), path.clone(), expected.clone())
                .await?;
            Ok(Reply::Upload { upload_id })
        }
        Op::FinishUpload { upload_id } => {
            again(ctx, peer, request, upload_id).await?;
            peer.uploads.finish(ctx, upload_id).await?;
            Ok(Reply::Done)
        }
        Op::AbortUpload { upload_id } => {
            peer.uploads.abort(ctx, upload_id).await?;
            Ok(Reply::Done)
        }
        Op::CreateFolder { share, path } => {
            let at = locate(ctx, peer, request, share.as_deref(), path).await?;
            change::create_folder(&at).await?;
            Ok(Reply::Done)
        }
        Op::Move { share, from, to } => {
            let from = locate(ctx, peer, request, share.as_deref(), from).await?;
            let to = locate(ctx, peer, request, share.as_deref(), to).await?;
            change::move_entry(&from, &to).await?;
            Ok(Reply::Done)
        }
        Op::Delete { share, path } => {
            let at = locate(ctx, peer, request, share.as_deref(), path).await?;
            let trashed = change::delete(ctx, &at).await?;
            Ok(Reply::Deleted { trashed })
        }
        Op::Read { .. } | Op::Watch { .. } | Op::UploadPart { .. } => Err(FailCode::Internal),
    }
}

/// Part `number` of an upload, whose `len` bytes follow the head on `recv`.
async fn part(
    ctx: &ServeContext,
    peer: &Peer,
    request: &Request,
    upload_id: &str,
    number: u16,
    len: u64,
    recv: &mut RecvStream,
) -> Result<Reply, FailCode> {
    if len > MAX_PART {
        return Err(FailCode::TooLarge);
    }
    again(ctx, peer, request, upload_id).await?;
    let mut body = Vec::with_capacity((len as usize).min(1 << 20));
    let read = tokio::time::timeout(
        upload::PART_LIMIT,
        (&mut *recv).take(len).read_to_end(&mut body),
    )
    .await;
    if !matches!(read, Ok(Ok(n)) if n as u64 == len) {
        return Err(FailCode::Internal);
    }
    Ok(Reply::Mark(
        peer.uploads.part(ctx, upload_id, number, body).await?,
    ))
}

/// The token a contact's request runs under, for the share and level it is allowed as now; a request
/// that is not allowed gets one nobody cancels, as it is refused anyway.
async fn stream_token(ctx: &ServeContext, peer: &Peer, op: &Op) -> CancellationToken {
    if peer.mode != Mode::Contact {
        return CancellationToken::new();
    }
    let share = match op {
        Op::UploadPart { upload_id, .. }
        | Op::FinishUpload { upload_id }
        | Op::AbortUpload { upload_id } => {
            peer.uploads.get(upload_id).and_then(|upload| upload.share)
        }
        op => op.share().map(str::to_owned),
    };
    let Some(share) = share else {
        return CancellationToken::new();
    };
    match grant_of(ctx, peer.vault, &share).await {
        Ok(Some((_, level))) => ctx.streams.token(StreamKey {
            share,
            vault: peer.vault,
            level,
        }),
        _ => CancellationToken::new(),
    }
}

pub(crate) async fn answer(mut send: SendStream, reply: Reply) {
    if write_head(&mut send, &reply).await.is_ok() {
        let _ = send.finish();
    }
}

#[cfg(test)]
#[path = "serve_tests.rs"]
mod tests;
