//! The new installation's side of a link (spec 024, FR-023 to FR-025,
//! research R11, contracts/sync-protocol.md `holzi-link/1`).
//!
//! [`run`] is the message exchange on one open stream. The vault it fills
//! was created by the caller with the user's own passphrase, which never
//! leaves this device. Everything is applied here as it arrives, and nothing
//! of it counts until the caller keeps the vault: on any error before the
//! `Done` this side sent, the caller deletes it (FR-025).

use std::sync::Arc;

use tokio::io::{AsyncRead, AsyncWrite};
use uuid::Uuid;

use crate::storage::query;
use crate::sync::content_keys;
use crate::sync::device_list;
use crate::sync::envelopes;
use crate::sync::inbound::Inbox;
use crate::sync::keys::{self, DeviceKeys};
use crate::sync::link::code::{LinkCode, Role, Transcript};
use crate::sync::link::error::LinkError;
use crate::sync::link::pending::{self, Pending, State};
use crate::sync::link::wire::LinkMessage;
use crate::sync::replica::Replica;
use crate::sync::signing;
use crate::sync::wire::{expect_value, write_value, SchemaVersion};
use crate::sync::wire::{FRAME_LIMIT, HANDSHAKE_FRAME_LIMIT};

/// What the new installation needs.
pub struct Join<'a> {
    pub replica: &'a Arc<Replica>,
    pub keys: &'a DeviceKeys,
    /// The device id of the new vault, which the host's list names this
    /// installation by.
    pub vault_device_uuid: Uuid,
    pub code: &'a LinkCode,
    /// The name the user gave this device.
    pub name: &'a str,
    pub schema: SchemaVersion,
    pub now_ms: u64,
}

/// Where the link stands, for the progress the user sees.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Stage {
    WaitingForConfirmation,
    /// The snapshot arrives; `pages` counts the pages applied so far.
    Transferring {
        pages: usize,
    },
}

/// A finished link.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Joined {
    pub link_id: [u8; 32],
    /// Whether the host made this device a main device.
    pub as_main: bool,
}

/// Runs the joining side on `send`/`recv`, the stream the host opened on the
/// connection from the endpoint `remote_endpoint`.
pub async fn run<W, R>(
    send: &mut W,
    recv: &mut R,
    remote_endpoint: [u8; 32],
    join: &Join<'_>,
    on_stage: impl Fn(Stage),
) -> Result<Joined, LinkError>
where
    W: AsyncWrite + Unpin + Send,
    R: AsyncRead + Unpin + Send,
{
    let LinkMessage::Hello {
        nonce_h,
        endpoint_h,
        device_h,
    } = expect_value(recv, HANDSHAKE_FRAME_LIMIT).await?
    else {
        return Err(LinkError::Protocol("expected a Hello"));
    };
    if endpoint_h != remote_endpoint {
        return Err(LinkError::BadProof);
    }
    let nonce_n = *keys::random_bytes::<32>();
    let transcript = Transcript {
        nonce_h,
        nonce_n,
        endpoint_h,
        endpoint_n: join.keys.endpoint_id,
        device_h,
        device_n: join.keys.device_pubkey,
    };
    let proof = LinkMessage::Proof {
        nonce_n,
        endpoint_n: join.keys.endpoint_id,
        device_n: join.keys.device_pubkey,
        vault_device_uuid: join.vault_device_uuid,
        name: join.name.to_string(),
        schema: join.schema,
        mac_n: join.code.proof(Role::Joiner, &transcript),
    };
    write_value(send, &proof, HANDSHAKE_FRAME_LIMIT).await?;

    // A host that does not accept the proof just closes the connection.
    let LinkMessage::HostProof { mac_h } = expect_value(recv, HANDSHAKE_FRAME_LIMIT)
        .await
        .map_err(|_| LinkError::BadProof)?
    else {
        return Err(LinkError::Protocol("expected the host's proof"));
    };
    if !join.code.verify_proof(Role::Host, &transcript, &mac_h) {
        return Err(LinkError::BadProof);
    }

    on_stage(Stage::WaitingForConfirmation);
    match expect_value(recv, FRAME_LIMIT).await? {
        LinkMessage::Decision { accepted: true } => {}
        LinkMessage::Decision { accepted: false } => return Err(LinkError::Declined),
        LinkMessage::Abort { reason } => return Err(LinkError::Aborted(reason)),
        _ => return Err(LinkError::Protocol("expected the host's decision")),
    }

    let mut inbox = Inbox::new();
    let mut pages = 0;
    on_stage(Stage::Transferring { pages });
    let (link_id, envelopes, list_payload, list_signature, vault_secret) = loop {
        match expect_value(recv, FRAME_LIMIT).await? {
            LinkMessage::Page(page) => {
                let replica = Arc::clone(join.replica);
                let (back, received) = tokio::task::spawn_blocking(move || {
                    let received = inbox.receive(&replica, page);
                    (inbox, received)
                })
                .await?;
                inbox = back;
                received?;
                pages += 1;
                on_stage(Stage::Transferring { pages });
            }
            LinkMessage::Transfer {
                link_id,
                envelopes,
                list_payload,
                list_signature,
                vault_secret,
            } => {
                break (
                    link_id,
                    envelopes,
                    list_payload,
                    list_signature.0,
                    vault_secret,
                )
            }
            LinkMessage::Abort { reason } => return Err(LinkError::Aborted(reason)),
            _ => return Err(LinkError::Protocol("expected a page or the transfer")),
        }
    };

    let record = Received {
        link_id,
        envelopes,
        list_payload,
        list_signature,
        secret: vault_secret.map(|s| s.0),
        resume_secret: *join.code.session_secret(&nonce_h, &nonce_n),
        peer_device: device_h,
    };
    let (replica, own, now) = (Arc::clone(join.replica), join.keys.clone(), join.now_ms);
    let as_main = tokio::task::spawn_blocking(move || store(&replica, &own, record, now)).await??;
    // Everything is stored. A `Done` that does not get through does not undo that: the host keeps
    // its record, and the two devices meet through this device's newer list as soon as both are
    // online (research R20), so the link ends the same way.
    if let Err(error) = write_value(send, &LinkMessage::Done { link_id }, FRAME_LIMIT).await {
        log::info!("sync: the link's Done did not reach the main device: {error}");
    }
    Ok(Joined { link_id, as_main })
}

/// What the transfer carried besides the snapshot.
struct Received {
    link_id: [u8; 32],
    envelopes: Vec<content_keys::StoredEnvelope>,
    list_payload: Vec<u8>,
    list_signature: [u8; 64],
    secret: Option<[u8; 32]>,
    resume_secret: [u8; 32],
    peer_device: [u8; 32],
}

/// Checks and stores the new list, the envelopes and the vault secret in one
/// transaction, then records the link as awaiting publication. Returns
/// whether this device is a main device now.
fn store(
    replica: &Replica,
    own: &DeviceKeys,
    received: Received,
    now_ms: u64,
) -> Result<bool, LinkError> {
    replica
        .db()
        .write(|tx| {
            let list_error =
                |reason: &str| haex_crdt::Error::consumer(LinkError::List(reason.to_string()));
            let vault = keys::vault_pubkey(tx)?
                .ok_or_else(|| list_error("the snapshot named no vault identity"))?;
            let signed =
                device_list::check_pushed(&received.list_payload, &received.list_signature, &vault)
                    .map_err(|e| list_error(&e.to_string()))?;
            let entry = signed
                .list
                .device(&own.device_pubkey)
                .ok_or_else(|| list_error("the new list does not name this device"))?;
            if entry.endpoint_id != own.endpoint_id
                || entry.vault_device_uuid != replica.db().device_id()
            {
                return Err(list_error("the new list names this device differently"));
            }
            let as_main = entry.role == device_list::Role::Main;
            if as_main != received.secret.is_some() {
                return Err(list_error("the vault secret does not fit the role"));
            }
            if let Some(secret) = &received.secret {
                if signing::xonly_public_key(secret).ok() != Some(vault) {
                    return Err(list_error("the vault secret is not this vault's"));
                }
                keys::store_vault_secret(tx, secret)?;
            }
            device_list::insert(tx, &signed)?;
            crate::contacts::card::own_card::refresh_quietly(tx);
            envelopes::insert_envelopes(tx, &received.envelopes)?;
            let valid = device_list::valid_lists(&device_list::load_all(tx)?, &vault);
            if !valid.contains_key(&signed.hash) {
                return Err(list_error(
                    "the new list is not valid on top of the snapshot",
                ));
            }
            if content_keys::unwrap_own_envelopes(tx, own, &valid)? == 0 {
                return Err(list_error("no content key could be opened"));
            }
            pending::store(
                tx,
                &Pending {
                    link_id: received.link_id,
                    peer_device: received.peer_device,
                    list_hash: signed.hash,
                    list_payload: received.list_payload.clone(),
                    list_signature: received.list_signature,
                    as_main,
                    resume_secret: received.resume_secret,
                    state: State::AwaitingPublication,
                    created_at: i64::try_from(now_ms).unwrap_or(i64::MAX),
                },
            )?;
            Ok(as_main)
        })
        .map_err(|e| match e {
            haex_crdt::Error::Consumer(inner) => match inner.downcast::<LinkError>() {
                Ok(link) => *link,
                Err(inner) => LinkError::Crdt(haex_crdt::Error::Consumer(inner)),
            },
            other => LinkError::Crdt(other),
        })
}

/// Drops only stale link records on the joining side.
///
/// A locally received transfer already inserts the new device list before the
/// joiner sends `Done`, so the effective list is not publication evidence.
/// Cleanup after a host publication is handled by
/// [`finish_pending_after_host_publication`] from the remote-pull path.
pub fn finish_pending(replica: &Replica, now_ms: i64) -> haex_crdt::Result<usize> {
    replica.db().write(|tx| pending::delete_stale(tx, now_ms))
}

/// Drops join records after a device-list update arrived through sync.
///
/// The caller supplies this evidence by choosing this function only from the
/// remote-pull callback. That keeps a list inserted by the link transfer
/// itself from being mistaken for the host's later publication.
pub fn finish_pending_after_host_publication(
    replica: &Replica,
    own: &DeviceKeys,
    vault: [u8; 32],
    now_ms: i64,
) -> haex_crdt::Result<usize> {
    replica.db().write(|tx| {
        let valid = device_list::valid_lists(&device_list::load_all(tx)?, &vault);
        let listed = device_list::effective(&valid)
            .is_some_and(|signed| signed.list.device(&own.device_pubkey).is_some());
        let mut dropped = pending::delete_stale(tx, now_ms)?;
        if listed {
            for record in pending::load_all(tx)? {
                if record.state == State::AwaitingPublication {
                    pending::delete(tx, &record.link_id)?;
                    dropped += 1;
                }
            }
        }
        Ok(dropped)
    })
}

/// Whether this vault has finished a link and waits for the host's list.
pub fn awaiting_publication(replica: &Replica) -> haex_crdt::Result<bool> {
    query::read(replica.db(), |r| {
        Ok(pending::load_all(r)?
            .iter()
            .any(|p| p.state == State::AwaitingPublication))
    })
}
