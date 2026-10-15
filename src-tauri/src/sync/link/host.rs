//! The main device's side of a link (spec 024, FR-023 to FR-025, FR-038,
//! research R11, contracts/sync-protocol.md `holzi-link/1`).
//!
//! [`run`] is the message exchange on one open stream, so it is testable
//! without a network; [`crate::sync::link::transport`] finds the new
//! installation and opens the stream.
//!
//! Nothing but the proofs crosses before the user decides. After a yes, the
//! new device list is signed but only stored in `pending_links_no_sync`;
//! the snapshot, the wrapped content keys and (for a main device role) the
//! vault secret go over this stream, and only after the new device says it
//! stored everything does the list become part of the vault (FR-025).

use std::future::Future;
use std::sync::Arc;

use tokio::io::{AsyncRead, AsyncWrite};

use crate::sync::device_list::{self, DeviceList, ListedDevice, Role as ListRole};
use crate::sync::envelopes;
use crate::sync::keys::{self, DeviceKeys};
use crate::sync::link::code::{LinkCode, Role, Transcript};
use crate::sync::link::error::LinkError;
use crate::sync::link::pending::{self, Pending, State};
use crate::sync::link::wire::{AbortReason, LinkMessage, SecretBytes};
use crate::sync::outbound::serve_pull;
use crate::sync::progress::Vector;
use crate::sync::replica::Replica;
use crate::sync::wire::{expect_value, write_value, SchemaVersion, Signature};
use crate::sync::wire::{FRAME_LIMIT, HANDSHAKE_FRAME_LIMIT};

/// What the user decided about the new device.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Decision {
    Reject,
    Accept {
        /// Whether the new device also becomes a main device (FR-024).
        as_main: bool,
    },
}

/// What the host reports while it waits for the user.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NewDevice {
    pub name: String,
}

/// How a link ended.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Outcome {
    Linked { device: [u8; 32], name: String },
    Rejected,
}

/// What the host session needs.
pub struct Host<'a> {
    pub replica: &'a Arc<Replica>,
    pub keys: &'a DeviceKeys,
    pub vault: [u8; 32],
    pub code: &'a LinkCode,
    pub schema: SchemaVersion,
    pub now_ms: u64,
}

/// Runs the host side on `send`/`recv`, the stream to the endpoint
/// `remote_endpoint`. `on_new_device` learns the new device's name once its
/// proof checked out; `decision` resolves when the user answered.
pub async fn run<W, R, F>(
    send: &mut W,
    recv: &mut R,
    remote_endpoint: [u8; 32],
    host: &Host<'_>,
    on_new_device: impl FnOnce(NewDevice),
    decision: F,
) -> Result<Outcome, LinkError>
where
    W: AsyncWrite + Unpin + Send,
    R: AsyncRead + Unpin + Send,
    F: Future<Output = Decision>,
{
    let nonce_h = *keys::random_bytes::<32>();
    let hello = LinkMessage::Hello {
        nonce_h,
        endpoint_h: host.keys.endpoint_id,
        device_h: host.keys.device_pubkey,
    };
    write_value(send, &hello, HANDSHAKE_FRAME_LIMIT).await?;

    let LinkMessage::Proof {
        nonce_n,
        endpoint_n,
        device_n,
        vault_device_uuid,
        name,
        schema,
        mac_n,
    } = expect_value(recv, HANDSHAKE_FRAME_LIMIT).await?
    else {
        return Err(LinkError::Protocol("expected a Proof"));
    };
    let transcript = Transcript {
        nonce_h,
        nonce_n,
        endpoint_h: host.keys.endpoint_id,
        endpoint_n,
        device_h: host.keys.device_pubkey,
        device_n,
    };
    if endpoint_n != remote_endpoint || !host.code.verify_proof(Role::Joiner, &transcript, &mac_n) {
        return Err(LinkError::BadProof);
    }
    let mac_h = host.code.proof(Role::Host, &transcript);
    write_value(
        send,
        &LinkMessage::HostProof { mac_h },
        HANDSHAKE_FRAME_LIMIT,
    )
    .await?;
    if schema != host.schema {
        write_value(
            send,
            &LinkMessage::Abort {
                reason: AbortReason::Incompatible,
            },
            HANDSHAKE_FRAME_LIMIT,
        )
        .await?;
        return Err(LinkError::Incompatible);
    }

    on_new_device(NewDevice { name: name.clone() });
    let as_main = match decision.await {
        Decision::Reject => {
            write_value(
                send,
                &LinkMessage::Decision { accepted: false },
                FRAME_LIMIT,
            )
            .await?;
            return Ok(Outcome::Rejected);
        }
        Decision::Accept { as_main } => as_main,
    };

    let joiner = Joiner {
        device: device_n,
        endpoint: endpoint_n,
        vault_device_uuid,
        name: name.clone(),
    };
    let secret = host.code.session_secret(&nonce_h, &nonce_n);
    let prepared = prepare(host, joiner, as_main, *secret).await?;
    write_value(send, &LinkMessage::Decision { accepted: true }, FRAME_LIMIT).await?;

    let replica = Arc::clone(host.replica);
    let mut outbox =
        tokio::task::spawn_blocking(move || serve_pull(&replica, &Vector::new())).await??;
    while let Some(page) = outbox.next_page() {
        write_value(send, &LinkMessage::Page(page), FRAME_LIMIT).await?;
    }
    let transfer = LinkMessage::Transfer {
        link_id: prepared.link_id,
        envelopes: prepared.envelopes,
        list_payload: prepared.list_payload,
        list_signature: Signature(prepared.list_signature),
        vault_secret: prepared.vault_secret,
    };
    write_value(send, &transfer, FRAME_LIMIT).await?;

    match expect_value(recv, FRAME_LIMIT).await? {
        LinkMessage::Done { link_id } if link_id == prepared.link_id => {}
        LinkMessage::Abort { reason } => return Err(LinkError::Aborted(reason)),
        _ => return Err(LinkError::Protocol("expected Done for this link")),
    }
    publish(host.replica, host.keys, host.vault, &prepared.link_id).await?;
    Ok(Outcome::Linked {
        device: device_n,
        name,
    })
}

/// The new device as it introduced itself.
struct Joiner {
    device: [u8; 32],
    endpoint: [u8; 32],
    vault_device_uuid: uuid::Uuid,
    name: String,
}

/// What goes to the new device besides the snapshot.
struct Prepared {
    link_id: [u8; 32],
    envelopes: Vec<crate::sync::content_keys::StoredEnvelope>,
    list_payload: Vec<u8>,
    list_signature: [u8; 64],
    vault_secret: Option<SecretBytes>,
}

/// Signs the new device list, wraps every content key for the new device
/// and stores the pending record, all before anything is sent (FR-025).
async fn prepare(
    host: &Host<'_>,
    joiner: Joiner,
    as_main: bool,
    resume_secret: [u8; 32],
) -> Result<Prepared, LinkError> {
    let replica = Arc::clone(host.replica);
    let (own, vault, now) = (host.keys.clone(), host.vault, host.now_ms);
    tokio::task::spawn_blocking(move || {
        replica.db().write(|tx| {
            let vault_secret = keys::vault_secret(tx)?
                .ok_or_else(|| haex_crdt::Error::consumer(LinkError::NotMainDevice))?;
            let valid = device_list::valid_lists(&device_list::load_all(tx)?, &vault);
            let effective = device_list::effective(&valid)
                .ok_or_else(|| haex_crdt::Error::consumer(LinkError::List("none".into())))?
                .clone();
            let held = envelopes::held_keys(tx)?;
            let newest = held.first().ok_or_else(|| {
                haex_crdt::Error::consumer(LinkError::List("no content key".into()))
            })?;
            let entry = ListedDevice {
                device_pubkey: joiner.device,
                endpoint_id: joiner.endpoint,
                role: if as_main {
                    ListRole::Main
                } else {
                    ListRole::Linked
                },
                vault_device_uuid: joiner.vault_device_uuid,
                name_sealed: crate::sync::content_keys::seal_name(
                    newest,
                    &joiner.device,
                    &joiner.name,
                ),
                added_at: now,
            };
            let mut devices = effective.list.devices.clone();
            devices.push(entry);
            let next = DeviceList {
                generation: effective.list.generation + 1,
                base_list_hash: Some(effective.hash),
                issued_by: own.device_pubkey,
                issued_at: now,
                devices,
                ..effective.list.clone()
            };
            next.check_structure()
                .map_err(|e| haex_crdt::Error::consumer(LinkError::List(e.to_string())))?;
            let signed =
                device_list::sign_list(next, &vault_secret).map_err(haex_crdt::Error::consumer)?;
            let wrapped = envelopes::envelopes_for(&held, &signed, &own, &joiner.device)?;
            let link_id = *keys::random_bytes::<32>();
            pending::store(
                tx,
                &Pending {
                    link_id,
                    peer_device: joiner.device,
                    list_hash: signed.hash,
                    list_payload: signed.payload.clone(),
                    list_signature: signed.signature,
                    as_main,
                    resume_secret,
                    state: State::Transferring,
                    created_at: i64::try_from(now).unwrap_or(i64::MAX),
                },
            )?;
            Ok(Prepared {
                link_id,
                envelopes: wrapped,
                list_payload: signed.payload,
                list_signature: signed.signature,
                vault_secret: as_main.then(|| SecretBytes(*vault_secret)),
            })
        })
    })
    .await?
    .map_err(unwrap_link_error)
}

/// A [`LinkError`] that crossed a storage transaction as a consumer error,
/// or the storage error itself.
fn unwrap_link_error(error: haex_crdt::Error) -> LinkError {
    match error {
        haex_crdt::Error::Consumer(inner) => match inner.downcast::<LinkError>() {
            Ok(link) => *link,
            Err(inner) => LinkError::Crdt(haex_crdt::Error::Consumer(inner)),
        },
        other => LinkError::Crdt(other),
    }
}

/// Makes the new list part of the vault, in two committed steps so that a
/// crash between them is finished by [`finish_pending`]: the record moves to
/// `awaiting_publication`, then list and envelopes are stored and the record
/// goes. Idempotent (contracts/sync-protocol.md).
pub async fn publish(
    replica: &Arc<Replica>,
    keys: &DeviceKeys,
    vault: [u8; 32],
    link_id: &[u8; 32],
) -> Result<bool, LinkError> {
    let (replica, keys, link_id) = (Arc::clone(replica), keys.clone(), *link_id);
    Ok(tokio::task::spawn_blocking(move || {
        replica
            .db()
            .write(|tx| pending::set_state(tx, &link_id, State::AwaitingPublication))?;
        publish_now(&replica, &keys, vault, &link_id)
    })
    .await??)
}

/// Stores the list and envelopes of a record in `awaiting_publication`;
/// `false` when there is no such record (already published).
fn publish_now(
    replica: &Replica,
    keys: &DeviceKeys,
    vault: [u8; 32],
    link_id: &[u8; 32],
) -> haex_crdt::Result<bool> {
    replica.db().write(|tx| {
        let Some(record) = pending::load_all(tx)?
            .into_iter()
            .find(|p| &p.link_id == link_id && p.state == State::AwaitingPublication)
        else {
            return Ok(false);
        };
        let signed =
            device_list::check_pushed(&record.list_payload, &record.list_signature, &vault)
                .map_err(haex_crdt::Error::consumer)?;
        device_list::insert(tx, &signed)?;
        crate::contacts::card::own_card::refresh_quietly(tx);
        let held = envelopes::held_keys(tx)?;
        let wrapped = envelopes::envelopes_for(&held, &signed, keys, &record.peer_device)?;
        envelopes::insert_envelopes(tx, &wrapped)?;
        pending::delete(tx, link_id)?;
        Ok(true)
    })
}

/// Drops the records of links whose new device is on the effective list
/// already: the device's `Done` never reached this one, and the list came
/// through the device itself when they met, so there is nothing to publish.
/// Only records still `transferring` are dropped; one in `awaiting_publication`
/// is published by [`finish_pending`], which also wraps the keys for the device.
/// Returns how many records it dropped.
pub fn drop_listed(replica: &Replica, vault: [u8; 32]) -> haex_crdt::Result<usize> {
    replica.db().write(|tx| {
        let valid = device_list::valid_lists(&device_list::load_all(tx)?, &vault);
        let Some(effective) = device_list::effective(&valid) else {
            return Ok(0);
        };
        let mut dropped = 0;
        for record in pending::load_all(tx)? {
            if record.state == State::Transferring
                && effective.list.device(&record.peer_device).is_some()
            {
                pending::delete(tx, &record.link_id)?;
                dropped += 1;
            }
        }
        Ok(dropped)
    })
}

/// Finishes every link that reached `awaiting_publication` before this
/// device stopped, and drops records too old to matter. Run when a vault
/// opens; returns how many lists it published.
pub fn finish_pending(
    replica: &Replica,
    keys: &DeviceKeys,
    vault: [u8; 32],
    now_ms: i64,
) -> haex_crdt::Result<usize> {
    replica.db().write(|tx| pending::delete_stale(tx, now_ms))?;
    drop_listed(replica, vault)?;
    let open: Vec<[u8; 32]> = crate::storage::query::read(replica.db(), |r| {
        Ok(pending::load_all(r)?
            .into_iter()
            .filter(|p| p.state == State::AwaitingPublication)
            .map(|p| p.link_id)
            .collect())
    })?;
    let mut published = 0;
    for link_id in open {
        if publish_now(replica, keys, vault, &link_id)? {
            published += 1;
        }
    }
    Ok(published)
}

#[cfg(test)]
#[path = "host_cut_tests.rs"]
mod cut_tests;
#[cfg(test)]
#[path = "host_tests.rs"]
mod tests;
