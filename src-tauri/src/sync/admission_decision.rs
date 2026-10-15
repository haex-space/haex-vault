//! A main device's answer to a request, and a copy of a main device enrolling
//! itself (spec 024, user story 7, FR-044, FR-045). Part of [`super`].

use haex_crdt::rusqlite::params;
use haex_crdt::CrdtTransaction;
use uuid::Uuid;

use super::store_impl::{delete_request, load_request};
use super::{AdmissionError, REJECTED};
use crate::sync::content_keys;
use crate::sync::device_list::{self, DeviceList, ListedDevice, Role, SignedList};
use crate::sync::envelopes;
use crate::sync::keys::{self, DeviceKeys};

/// What a main device decided about an open request.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Decided {
    /// The copy is on the new list, generation `list_generation`.
    Admitted {
        list_generation: u64,
        endpoint_id: [u8; 32],
    },
    Refused,
    /// The copy was on the list already: only the stale request went.
    AlreadyListed,
    /// The request named a removed device or did not verify: it went, nobody
    /// was admitted.
    Dropped,
}

/// Admits (`admit`) or refuses the open request of `target`, in one
/// transaction. Admitting puts the copy on a device list of the next
/// generation as a linked device and wraps every content key generation this
/// device holds for it (FR-045).
pub fn decide(
    replica: &crate::sync::replica::Replica,
    own: &DeviceKeys,
    target: &[u8; 32],
    admit: bool,
    now_ms: u64,
) -> Result<Decided, AdmissionError> {
    let (own, target) = (own.clone(), *target);
    replica
        .db()
        .write(|tx| {
            let fail = |error: AdmissionError| haex_crdt::Error::consumer(error);
            let vault_secret =
                keys::vault_secret(tx)?.ok_or_else(|| fail(AdmissionError::NotMainDevice))?;
            let vault =
                keys::vault_pubkey(tx)?.ok_or_else(|| fail(AdmissionError::NoDeviceList))?;
            let valid = device_list::valid_lists(&device_list::load_all(tx)?, &vault);
            let effective = device_list::effective(&valid)
                .ok_or_else(|| fail(AdmissionError::NoDeviceList))?
                .clone();
            if !effective.list.is_main(&own.device_pubkey) {
                return Err(fail(AdmissionError::NotMainDevice));
            }
            let request =
                load_request(tx, &target)?.ok_or_else(|| fail(AdmissionError::NoRequest))?;
            if !admit {
                tx.execute(
                    "UPDATE admission_requests SET state = ?2 WHERE device_pubkey = ?1",
                    params![target.as_slice(), REJECTED],
                )?;
                return Ok(Decided::Refused);
            }

            if effective.list.device(&target).is_some() {
                delete_request(tx, &target)?;
                return Ok(Decided::AlreadyListed);
            }
            if effective.list.removes(&target) || request.verify().is_err() {
                delete_request(tx, &target)?;
                return Ok(Decided::Dropped);
            }
            let held = envelopes::held_keys(tx)?;
            let newest = held
                .first()
                .ok_or_else(|| fail(AdmissionError::NoContentKey))?;
            let mut devices = effective.list.devices.clone();
            devices.push(ListedDevice {
                device_pubkey: request.device,
                endpoint_id: request.endpoint,
                role: Role::Linked,
                vault_device_uuid: request.vault_device_uuid,
                name_sealed: content_keys::seal_name(newest, &request.device, &request.name),
                added_at: now_ms,
            });
            let next = DeviceList {
                generation: effective.list.generation + 1,
                base_list_hash: Some(effective.hash),
                issued_by: own.device_pubkey,
                issued_at: now_ms,
                devices,
                ..effective.list.clone()
            };
            next.check_structure().map_err(haex_crdt::Error::consumer)?;
            let signed =
                device_list::sign_list(next, &vault_secret).map_err(haex_crdt::Error::consumer)?;
            device_list::insert(tx, &signed)?;
            crate::contacts::card::own_card::refresh_quietly(tx);
            let wrapped = envelopes::envelopes_for(&held, &signed, &own, &request.device)?;
            envelopes::insert_envelopes(tx, &wrapped)?;
            delete_request(tx, &target)?;
            Ok(Decided::Admitted {
                list_generation: signed.list.generation,
                endpoint_id: request.endpoint,
            })
        })
        .map_err(unwrap_error)
}

/// Lets a copy of a main device enroll itself as a main device: the next
/// device list names it with its own device key, and its own key generation
/// envelopes are stored (FR-044). `false` when the copy's keys are not there
/// (nothing to enroll) or no content key is held.
pub fn enroll_as_main(
    tx: &mut CrdtTransaction<'_>,
    own: &DeviceKeys,
    vault_secret: &[u8; 32],
    effective: &SignedList,
    vault_device_uuid: Uuid,
    name: &str,
    now_ms: u64,
) -> haex_crdt::Result<bool> {
    let held = envelopes::held_keys(tx)?;
    let Some(newest) = held.first() else {
        log::warn!("sync: a copy of a main device holds no content key, it cannot enroll");
        return Ok(false);
    };
    let mut devices = effective.list.devices.clone();
    devices.push(ListedDevice {
        device_pubkey: own.device_pubkey,
        endpoint_id: own.endpoint_id,
        role: Role::Main,
        vault_device_uuid,
        name_sealed: content_keys::seal_name(newest, &own.device_pubkey, name),
        added_at: now_ms,
    });
    let next = DeviceList {
        generation: effective.list.generation + 1,
        base_list_hash: Some(effective.hash),
        issued_by: own.device_pubkey,
        issued_at: now_ms,
        devices,
        ..effective.list.clone()
    };
    next.check_structure().map_err(haex_crdt::Error::consumer)?;
    let signed = device_list::sign_list(next, vault_secret).map_err(haex_crdt::Error::consumer)?;
    device_list::insert(tx, &signed)?;
    crate::contacts::card::own_card::refresh_quietly(tx);
    let wrapped = envelopes::envelopes_for(&held, &signed, own, &own.device_pubkey)?;
    envelopes::insert_envelopes(tx, &wrapped)?;
    Ok(true)
}

/// An [`AdmissionError`] that crossed the storage transaction as a consumer
/// error, or the storage error itself.
fn unwrap_error(error: haex_crdt::Error) -> AdmissionError {
    match error {
        haex_crdt::Error::Consumer(inner) => match inner.downcast::<AdmissionError>() {
            Ok(admission) => *admission,
            Err(inner) => AdmissionError::Crdt(haex_crdt::Error::Consumer(inner)),
        },
        other => AdmissionError::Crdt(other),
    }
}
