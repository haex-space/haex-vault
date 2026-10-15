//! Removing a device from the vault (spec 024, user story 6, FR-026 to
//! FR-028, research R5, R8, R9).
//!
//! A main device publishes, in one transaction, a device list of the next
//! generation without the device, carrying the device's limit, and a new
//! content key generation wrapped only for the devices that stay. The limit
//! is the progress this device holds for the removed one: everything the
//! removed device wrote up to it stays, everything beyond it is refused by
//! every device that knows the list, also when another device forwards it
//! ([`crate::sync::inbound`]). The other devices take list and key over the
//! next time they sync. The vault identity and every secret stay as they
//! are: the device list is what locks a device out (D26, D27).
//!
//! It helps against an honest device only: what is on a removed device stays
//! there, and protected by the passphrase alone.

use haex_crdt::rusqlite::params;

use crate::sync::content_keys::{self, ContentKey};
use crate::sync::device_list::{self, DeviceList, RemovedDevice};
use crate::sync::keys::{self, DeviceKeys};
use crate::sync::replica::Replica;

/// Why a removal did not happen.
#[derive(Debug, thiserror::Error)]
pub enum RemovalError {
    /// Only a main device, which holds the vault secret, changes the list.
    #[error("this device is not a main device")]
    NotMainDevice,
    /// A device never removes itself (FR-035).
    #[error("a device cannot remove itself")]
    SelfRemoval,
    /// The device is not on the effective list.
    #[error("the device is not on the device list")]
    UnknownDevice,
    #[error("the vault has no valid device list")]
    NoDeviceList,
    #[error(transparent)]
    Crdt(#[from] haex_crdt::Error),
    #[error("a database task failed: {0}")]
    Join(#[from] tokio::task::JoinError),
}

/// What a removal published.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Removal {
    /// The generation of the new device list.
    pub list_generation: u64,
    pub list_hash: [u8; 32],
    /// The generation of the new content key.
    pub key_generation: u64,
    /// The last change of the removed device that still counts.
    pub limit_hlc: String,
    /// The removed device's endpoint, to forget its address.
    pub endpoint_id: [u8; 32],
}

/// Removes `target` from the vault. `own` is this device, which must be a
/// main device.
pub fn remove_device(
    replica: &Replica,
    own: &DeviceKeys,
    target: &[u8; 32],
    now_ms: u64,
) -> Result<Removal, RemovalError> {
    if target == &own.device_pubkey {
        return Err(RemovalError::SelfRemoval);
    }
    // Applying a pull and this removal exclude each other, so the limit is
    // exactly what this device holds of the removed device when it publishes.
    let _exchange = replica.exchange();
    let progress = replica.progress()?;
    let (own, target) = (own.clone(), *target);
    replica
        .db()
        .write(|tx| {
            let fail = |error: RemovalError| haex_crdt::Error::consumer(error);
            let vault_secret =
                keys::vault_secret(tx)?.ok_or_else(|| fail(RemovalError::NotMainDevice))?;
            let vault = keys::vault_pubkey(tx)?.ok_or_else(|| fail(RemovalError::NoDeviceList))?;
            let valid = device_list::valid_lists(&device_list::load_all(tx)?, &vault);
            let effective = device_list::effective(&valid)
                .ok_or_else(|| fail(RemovalError::NoDeviceList))?
                .clone();
            if !effective.list.is_main(&own.device_pubkey) {
                return Err(fail(RemovalError::NotMainDevice));
            }
            let entry = effective
                .list
                .device(&target)
                .cloned()
                .ok_or_else(|| fail(RemovalError::UnknownDevice))?;

            let limit_hlc = progress
                .get(&entry.vault_device_uuid)
                .cloned()
                .unwrap_or_else(|| "0/0".to_string());
            let mut removed = effective.list.removed.clone();
            removed.push(RemovedDevice {
                device_pubkey: target,
                vault_device_uuid: entry.vault_device_uuid,
                limit_hlc: limit_hlc.clone(),
                removed_at: now_ms,
            });
            let next = DeviceList {
                generation: effective.list.generation + 1,
                base_list_hash: Some(effective.hash),
                issued_by: own.device_pubkey,
                issued_at: now_ms,
                devices: effective
                    .list
                    .devices
                    .iter()
                    .filter(|d| d.device_pubkey != target)
                    .cloned()
                    .collect(),
                removed,
                ..effective.list.clone()
            };
            next.check_structure().map_err(haex_crdt::Error::consumer)?;
            let signed =
                device_list::sign_list(next, &vault_secret).map_err(haex_crdt::Error::consumer)?;
            device_list::insert(tx, &signed)?;
            crate::contacts::card::own_card::refresh_quietly(tx);

            // One generation above every one the vault ever had, wrapped for
            // the devices of the new list only.
            let key_generation = content_keys::load_generations(tx)?
                .iter()
                .map(|g| u64::try_from(g.generation).unwrap_or(0))
                .max()
                .unwrap_or(0)
                + 1;
            let key = ContentKey::generate(key_generation);
            content_keys::issue_generation(tx, &key, &signed, &own, now_ms)?;

            tx.execute(
                "DELETE FROM device_presence_no_sync WHERE device_pubkey = ?1",
                params![target.as_slice()],
            )?;
            Ok(Removal {
                list_generation: signed.list.generation,
                list_hash: signed.hash,
                key_generation,
                limit_hlc,
                endpoint_id: entry.endpoint_id,
            })
        })
        .map_err(unwrap_error)
}

impl From<RemovalError> for crate::error::HolziError {
    fn from(error: RemovalError) -> Self {
        use crate::error::HolziError;
        match error {
            RemovalError::NotMainDevice => HolziError::NotMainDevice,
            RemovalError::Crdt(error) => error.into(),
            RemovalError::Join(error) => HolziError::CrdtInit {
                reason: format!("removal task: {error}"),
            },
            other => HolziError::InvalidInput {
                reason: other.to_string(),
            },
        }
    }
}

/// A [`RemovalError`] that crossed the storage transaction as a consumer
/// error, or the storage error itself.
fn unwrap_error(error: haex_crdt::Error) -> RemovalError {
    match error {
        haex_crdt::Error::Consumer(inner) => match inner.downcast::<RemovalError>() {
            Ok(removal) => *removal,
            Err(inner) => RemovalError::Crdt(haex_crdt::Error::Consumer(inner)),
        },
        other => RemovalError::Crdt(other),
    }
}

#[cfg(test)]
#[path = "removal_tests.rs"]
mod tests;
