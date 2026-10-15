//! The mutual handshake of `holzi-sync/1` (contracts/sync-protocol.md §1,
//! FR-009, FR-029, FR-030, research R6, R14).
//!
//! The accepting side A opens the first stream:
//!
//! ```text
//! A → D  Challenge { nonce_a, endpoint_a, list }
//! D → A  DeviceListPush*   (when D's effective list ranks before A's)
//! D → A  Response { device_d, vault, nonce_d, schema, list, sig_d }
//! A → D  DeviceListPush*   (when A's effective list ranks before D's)
//! A → D  Accept { device_a, schema, list, sig_a } | Reject { code }
//! ```
//!
//! Each side signs `lp(nonce_a) ‖ lp(nonce_d) ‖ lp(own endpoint) ‖ lp(other
//! endpoint) ‖ lp(vault)` with its device key. The other endpoint is the
//! connection's `remote_id()`, so a signature only verifies for the
//! endpoint the connection really comes from. A pushed list travels with
//! its base lists and is stored once it checks out; only then does each
//! side look the other up in its effective list: listed with exactly this
//! endpoint, never removed. Before `Accept` nothing but device lists flows.
//!
//! Maintainability exception (spaex 500-LoC rule): both sides of the handshake run the same
//! transcript, list pushes and admission, so they have stayed in one file. Concrete split plan,
//! if this grows further: move the device-list exchange (`read_pushes_until`, `push_if_better`,
//! `push_lists`, `list_ref`, `effective_ref`, `load_lists`, `store_pushed`) into
//! `handshake_lists.rs`, leaving `accept_with`, `dial_with` and the admission here.

use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

use tokio::io::{AsyncRead, AsyncWrite};
use uuid::Uuid;

use crate::identity::{holzi_migration_source, HOLZI_TRIGGER_VERSION};
use crate::storage::query;
use crate::sync::device_list::{self, Role, SignedList};
use crate::sync::keys::{self, DeviceKeys};
use crate::sync::problems::Problem;
use crate::sync::replica::Replica;
use crate::sync::signing::{self, lp, Domain};
use crate::sync::wire::{
    expect_frame, write_frame, ListRef, Message, RejectCode, SchemaVersion, Signature,
    HANDSHAKE_FRAME_LIMIT, PROTOCOL_VERSION,
};

/// Most device lists a side may push in one handshake.
const MAX_PUSHED_LISTS: usize = 64;

/// This device as it presents itself.
pub struct Local<'a> {
    pub keys: &'a DeviceKeys,
    pub vault: [u8; 32],
    pub schema: SchemaVersion,
}

/// The device on the other end, as the effective list names it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Peer {
    pub device_pubkey: [u8; 32],
    pub endpoint_id: [u8; 32],
    pub vault_device_uuid: Uuid,
    pub role: Role,
    /// The effective list both sides hold after the handshake.
    pub list: ListRef,
}

/// Why a handshake ended without a peer.
#[derive(Debug, thiserror::Error)]
pub enum HandshakeError {
    #[error("this device refused the peer: {0:?}")]
    Refused(RejectCode),
    #[error("the peer refused this device: {0:?}")]
    RefusedByPeer(RejectCode),
    /// A device that proved its key and belongs to this vault, with which
    /// this device must not sync for now (FR-029, FR-030).
    #[error("sync with a device is halted: {}", .problem.as_str())]
    Halted { problem: Problem, device: [u8; 32] },
    #[error("handshake protocol violation: {0}")]
    Protocol(&'static str),
    #[error("this device holds no valid device list")]
    NoDeviceList,
    #[error(transparent)]
    Wire(#[from] crate::sync::wire::WireError),
    #[error(transparent)]
    Crdt(#[from] haex_crdt::Error),
    #[error("a database task failed: {0}")]
    Join(#[from] tokio::task::JoinError),
    #[error(transparent)]
    Signing(#[from] signing::SigningError),
}

/// The schema this build syncs with: all three must match (FR-029).
pub fn local_schema() -> SchemaVersion {
    let migrations = haex_crdt::MigrationSource::list_migrations(&*holzi_migration_source())
        .map(|names| names.len())
        .unwrap_or(0);
    SchemaVersion {
        protocol: u32::from(PROTOCOL_VERSION),
        holzi_migration: u32::try_from(migrations).unwrap_or(u32::MAX),
        crdt_trigger: u32::try_from(HOLZI_TRIGGER_VERSION).unwrap_or(0),
    }
}

/// The accepting side.
pub async fn accept<W, R>(
    send: &mut W,
    recv: &mut R,
    replica: &Arc<Replica>,
    local: &Local<'_>,
    remote_endpoint: [u8; 32],
) -> Result<Peer, HandshakeError>
where
    W: AsyncWrite + Unpin + Send,
    R: AsyncRead + Unpin + Send,
{
    let noop = |_tables: BTreeSet<String>| {};
    accept_with(send, recv, replica, local, remote_endpoint, &noop).await
}

/// The accepting side with a callback for device lists learned in the handshake.
pub(crate) async fn accept_with<W, R>(
    send: &mut W,
    recv: &mut R,
    replica: &Arc<Replica>,
    local: &Local<'_>,
    remote_endpoint: [u8; 32],
    on_applied: &(dyn Fn(BTreeSet<String>) + Send + Sync),
) -> Result<Peer, HandshakeError>
where
    W: AsyncWrite + Unpin + Send,
    R: AsyncRead + Unpin + Send,
{
    let lists = load_lists(replica, local.vault).await?;
    let nonce_a = *keys::random_bytes::<32>();
    let challenge = Message::Challenge {
        v: PROTOCOL_VERSION,
        nonce_a,
        endpoint_a: local.keys.endpoint_id,
        list: effective_ref(&lists)?,
    };
    write_frame(send, &challenge, HANDSHAKE_FRAME_LIMIT).await?;

    let (pushed, response) =
        read_pushes_until(recv, |m| matches!(m, Message::Response { .. })).await?;
    let Message::Response {
        v,
        device_d,
        vault,
        nonce_d,
        schema,
        list: their_list,
        sig_d,
    } = response
    else {
        return Err(HandshakeError::Protocol("expected a Response"));
    };
    let their_transcript = transcript(
        &nonce_a,
        &nonce_d,
        &remote_endpoint,
        &local.keys.endpoint_id,
        &vault,
    );
    let checked = if vault != local.vault {
        Err(RejectCode::ForeignVault)
    } else if signing::verify(Domain::DeviceAuth, &their_transcript, &sig_d.0, &device_d).is_err() {
        Err(RejectCode::BadSignature)
    } else if v != PROTOCOL_VERSION || schema != local.schema {
        Err(RejectCode::Incompatible)
    } else {
        Ok(())
    };
    if let Err(code) = checked {
        return refuse(send, code, &device_d).await;
    }

    let (lists, changed) = store_pushed(replica, local.vault, pushed, lists).await?;
    notify_applied(on_applied, changed);
    let own = effective_ref(&lists)?;
    push_if_better(send, &lists, &own, &their_list).await?;
    let peer = match admit(&lists, &device_d, &remote_endpoint, local) {
        Ok(peer) => peer,
        Err(code) => return refuse(send, code, &device_d).await,
    };

    let own_transcript = transcript(
        &nonce_a,
        &nonce_d,
        &local.keys.endpoint_id,
        &remote_endpoint,
        &local.vault,
    );
    let accept = Message::Accept {
        device_a: local.keys.device_pubkey,
        schema: local.schema,
        list: own,
        sig_a: Signature(sign(local, &own_transcript)?),
    };
    write_frame(send, &accept, HANDSHAKE_FRAME_LIMIT).await?;
    Ok(peer)
}

/// The dialing side.
pub async fn dial<W, R>(
    send: &mut W,
    recv: &mut R,
    replica: &Arc<Replica>,
    local: &Local<'_>,
    remote_endpoint: [u8; 32],
) -> Result<Peer, HandshakeError>
where
    W: AsyncWrite + Unpin + Send,
    R: AsyncRead + Unpin + Send,
{
    let noop = |_tables: BTreeSet<String>| {};
    dial_with(send, recv, replica, local, remote_endpoint, &noop).await
}

/// The dialing side with a callback for device lists learned in the handshake.
pub(crate) async fn dial_with<W, R>(
    send: &mut W,
    recv: &mut R,
    replica: &Arc<Replica>,
    local: &Local<'_>,
    remote_endpoint: [u8; 32],
    on_applied: &(dyn Fn(BTreeSet<String>) + Send + Sync),
) -> Result<Peer, HandshakeError>
where
    W: AsyncWrite + Unpin + Send,
    R: AsyncRead + Unpin + Send,
{
    let Message::Challenge {
        v,
        nonce_a,
        endpoint_a,
        list: their_list,
    } = expect_frame(recv, HANDSHAKE_FRAME_LIMIT).await?
    else {
        return Err(HandshakeError::Protocol("expected a Challenge"));
    };
    if v != PROTOCOL_VERSION {
        return Err(HandshakeError::Refused(RejectCode::Incompatible));
    }
    if endpoint_a != remote_endpoint {
        return Err(HandshakeError::Refused(RejectCode::BadSignature));
    }

    let lists = load_lists(replica, local.vault).await?;
    let own = effective_ref(&lists)?;
    push_if_better(send, &lists, &own, &their_list).await?;
    let nonce_d = *keys::random_bytes::<32>();
    let own_transcript = transcript(
        &nonce_a,
        &nonce_d,
        &local.keys.endpoint_id,
        &remote_endpoint,
        &local.vault,
    );
    let response = Message::Response {
        v: PROTOCOL_VERSION,
        device_d: local.keys.device_pubkey,
        vault: local.vault,
        nonce_d,
        schema: local.schema,
        list: own,
        sig_d: Signature(sign(local, &own_transcript)?),
    };
    write_frame(send, &response, HANDSHAKE_FRAME_LIMIT).await?;

    let (pushed, answer) = read_pushes_until(recv, |m| {
        matches!(m, Message::Accept { .. } | Message::Reject { .. })
    })
    .await?;
    let (device_a, schema, sig_a) = match answer {
        Message::Accept {
            device_a,
            schema,
            sig_a,
            ..
        } => (device_a, schema, sig_a),
        Message::Reject { code } => {
            // What the peer pushed before refusing is still true: a list is signed by the vault
            // identity and checked on its own. It is how a removed device hears that it was
            // removed (FR-034), so it is kept although the peer refuses this device.
            if !pushed.is_empty() {
                match store_pushed(replica, local.vault, pushed, lists).await {
                    Ok((_, changed)) => notify_applied(on_applied, changed),
                    Err(error) => {
                        log::debug!("sync: a list pushed before a refusal was not stored: {error}")
                    }
                }
            }
            return Err(HandshakeError::RefusedByPeer(code));
        }
        _ => return Err(HandshakeError::Protocol("expected Accept or Reject")),
    };
    let their_transcript = transcript(
        &nonce_a,
        &nonce_d,
        &remote_endpoint,
        &local.keys.endpoint_id,
        &local.vault,
    );
    if signing::verify(Domain::DeviceAuth, &their_transcript, &sig_a.0, &device_a).is_err() {
        return Err(HandshakeError::Refused(RejectCode::BadSignature));
    }
    if schema != local.schema {
        return Err(halted(RejectCode::Incompatible, device_a));
    }
    let (lists, changed) = store_pushed(replica, local.vault, pushed, lists).await?;
    notify_applied(on_applied, changed);
    admit(&lists, &device_a, &remote_endpoint, local).map_err(|code| halted(code, device_a))
}

/// The error for a refusal of `code` toward a peer whose key is verified:
/// version and duplicate refusals halt sync with that device, the rest are
/// plain refusals.
fn halted(code: RejectCode, device: [u8; 32]) -> HandshakeError {
    match code {
        RejectCode::Incompatible => HandshakeError::Halted {
            problem: Problem::IncompatibleVersion,
            device,
        },
        RejectCode::Duplicate => HandshakeError::Halted {
            problem: Problem::Duplicate,
            device,
        },
        code => HandshakeError::Refused(code),
    }
}

/// The signed bytes, `own` being the signer's endpoint.
fn transcript(
    nonce_a: &[u8; 32],
    nonce_d: &[u8; 32],
    own: &[u8; 32],
    other: &[u8; 32],
    vault: &[u8; 32],
) -> Vec<u8> {
    [lp(nonce_a), lp(nonce_d), lp(own), lp(other), lp(vault)].concat()
}

fn sign(local: &Local<'_>, transcript: &[u8]) -> Result<[u8; 64], HandshakeError> {
    Ok(signing::sign(
        Domain::DeviceAuth,
        transcript,
        &local.keys.device_secret,
    )?)
}

/// Looks the peer up in the effective list. The peer proved its device key
/// before this runs, so a key the list names with another endpoint (or one
/// that shares its device id with another key) is a duplicate, not a
/// stranger.
fn admit(
    lists: &BTreeMap<[u8; 32], SignedList>,
    device: &[u8; 32],
    endpoint: &[u8; 32],
    local: &Local<'_>,
) -> Result<Peer, RejectCode> {
    if device == &local.keys.device_pubkey || endpoint == &local.keys.endpoint_id {
        return Err(RejectCode::Duplicate);
    }
    let effective = device_list::effective(lists).ok_or(RejectCode::NotOnList)?;
    // Only the effective list's own (carried-forward) removals are final; a
    // same-generation fork that lost the tie-break (FR-005, FR-043) does not
    // get a say in who stays removed.
    if effective.list.removes(device) {
        return Err(RejectCode::Removed);
    }
    let entry = effective.list.device(device).ok_or(RejectCode::NotOnList)?;
    if &entry.endpoint_id != endpoint || device_list::uuid_conflicts(lists).contains(device) {
        return Err(RejectCode::Duplicate);
    }
    Ok(Peer {
        device_pubkey: *device,
        endpoint_id: *endpoint,
        vault_device_uuid: entry.vault_device_uuid,
        role: entry.role,
        list: list_ref(effective),
    })
}

/// Sends `Reject` and fails with the matching error for `device`, whose
/// signature this side already checked unless `code` says it did not.
async fn refuse<W: AsyncWrite + Unpin>(
    send: &mut W,
    code: RejectCode,
    device: &[u8; 32],
) -> Result<Peer, HandshakeError> {
    write_frame(send, &Message::Reject { code }, HANDSHAKE_FRAME_LIMIT).await?;
    Err(halted(code, *device))
}

/// Reads pushed lists until a frame `is_end` accepts.
async fn read_pushes_until<R: AsyncRead + Unpin>(
    recv: &mut R,
    is_end: impl Fn(&Message) -> bool,
) -> Result<(Vec<(Vec<u8>, [u8; 64])>, Message), HandshakeError> {
    let mut pushed = Vec::new();
    loop {
        match expect_frame(recv, HANDSHAKE_FRAME_LIMIT).await? {
            Message::DeviceListPush { payload, signature } if pushed.len() < MAX_PUSHED_LISTS => {
                pushed.push((payload, signature.0));
            }
            message if is_end(&message) => return Ok((pushed, message)),
            _ => {
                return Err(HandshakeError::Protocol(
                    "an unexpected message in the handshake",
                ))
            }
        }
    }
}

/// Pushes `own` with its base lists when it ranks before `their_list`.
async fn push_if_better<W: AsyncWrite + Unpin>(
    send: &mut W,
    lists: &BTreeMap<[u8; 32], SignedList>,
    own: &ListRef,
    their_list: &ListRef,
) -> Result<(), HandshakeError> {
    if device_list::ranks_before(
        (own.generation, &own.list_hash),
        (their_list.generation, &their_list.list_hash),
    ) {
        push_lists(send, lists, own, their_list).await?;
    }
    Ok(())
}

/// Sends `own` with its base lists, oldest first, leaving out `theirs` and
/// everything before it: the peer already holds those. Fails locally,
/// without writing anything, when what is left still exceeds
/// [`MAX_PUSHED_LISTS`] (the receiving side hard-rejects that many pushes in
/// one handshake).
async fn push_lists<W: AsyncWrite + Unpin>(
    send: &mut W,
    lists: &BTreeMap<[u8; 32], SignedList>,
    own: &ListRef,
    theirs: &ListRef,
) -> Result<(), HandshakeError> {
    let chain = device_list::ancestry(lists, &own.list_hash);
    let start = chain
        .iter()
        .position(|signed| signed.hash == theirs.list_hash)
        .map_or(0, |i| i + 1);
    let chain = &chain[start..];
    if chain.len() > MAX_PUSHED_LISTS {
        return Err(HandshakeError::Protocol("too many device lists to push"));
    }
    for signed in chain {
        let push = Message::DeviceListPush {
            payload: signed.payload.clone(),
            signature: Signature(signed.signature),
        };
        write_frame(send, &push, HANDSHAKE_FRAME_LIMIT).await?;
    }
    Ok(())
}

fn list_ref(signed: &SignedList) -> ListRef {
    ListRef {
        generation: signed.list.generation,
        list_hash: signed.hash,
    }
}

fn effective_ref(lists: &BTreeMap<[u8; 32], SignedList>) -> Result<ListRef, HandshakeError> {
    device_list::effective(lists)
        .map(list_ref)
        .ok_or(HandshakeError::NoDeviceList)
}

async fn load_lists(
    replica: &Arc<Replica>,
    vault: [u8; 32],
) -> Result<BTreeMap<[u8; 32], SignedList>, HandshakeError> {
    let replica = Arc::clone(replica);
    let rows = tokio::task::spawn_blocking(move || {
        query::read(replica.db(), |r| device_list::load_all(r))
    })
    .await??;
    Ok(device_list::valid_lists(&rows, &vault))
}

/// Stores every pushed list that checks out on its own, then re-reads the
/// valid lists; a pushed list without its base stays invalid.
async fn store_pushed(
    replica: &Arc<Replica>,
    vault: [u8; 32],
    pushed: Vec<(Vec<u8>, [u8; 64])>,
    lists: BTreeMap<[u8; 32], SignedList>,
) -> Result<(BTreeMap<[u8; 32], SignedList>, bool), HandshakeError> {
    let fresh: Vec<SignedList> = pushed
        .iter()
        .filter_map(|(payload, signature)| {
            match device_list::check_pushed(payload, signature, &vault) {
                Ok(signed) => Some(signed),
                Err(error) => {
                    log::warn!("sync: a pushed device list did not check out: {error}");
                    None
                }
            }
        })
        .filter(|signed| !lists.contains_key(&signed.hash))
        .collect();
    if fresh.is_empty() {
        return Ok((lists, false));
    }
    let writer = Arc::clone(replica);
    tokio::task::spawn_blocking(move || {
        writer.db().write(|tx| {
            for signed in &fresh {
                device_list::insert(tx, signed)?;
            }
            // Spec 051 FR-010: a main device renews its business card with the new list.
            if !fresh.is_empty() {
                crate::contacts::card::own_card::refresh_quietly(tx);
            }
            Ok(())
        })
    })
    .await??;
    Ok((load_lists(replica, vault).await?, true))
}

fn notify_applied(on_applied: &(dyn Fn(BTreeSet<String>) + Send + Sync), changed: bool) {
    if changed {
        on_applied(BTreeSet::from(["device_lists".to_owned()]));
    }
}

#[cfg(test)]
#[path = "handshake_refusal_tests.rs"]
mod refusal_tests;
#[cfg(test)]
#[path = "handshake_tests.rs"]
mod tests;
