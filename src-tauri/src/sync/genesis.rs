//! The sync state a vault needs before the first connection (spec 024,
//! FR-001 to FR-006, FR-015, research R2/R3).
//!
//! [`ensure_sync_state`] runs after every open, in one CRDT write and
//! idempotently:
//! 1. publish the vault identity, derived from its seed (a legacy
//!    placeholder, or a fresh seed for a new vault);
//! 2. create this installation's device and endpoint keys;
//! 3. on a main device of a vault without any device list, issue the first
//!    device list (generation 1, this device as main device) and the first
//!    content key generation, wrapped for this device;
//! 4. unwrap every content key addressed to this device;
//! 5. in a copy of a main device's vault file (keys created just now, a list
//!    that does not name this device, the vault secret in the file), enroll
//!    this device as a main device and leave a notice for the user (FR-044).
//!    A copy of a linked device has no secret: it asks for admission
//!    ([`crate::sync::admission`]) once presence runs.
//!
//! Each copy of a legacy vault derives the same identity and issues its own
//! first list; when two copies meet, the device list rules merge them
//! (research R3, FR-043).

use haex_crdt::rusqlite::params;
use haex_crdt::Database;
use uuid::Uuid;

use crate::storage::query::Query;
use crate::sync::admission;
use crate::sync::content_keys::{self, ContentKey};
use crate::sync::device_list::{self, DeviceList, ListedDevice, Role};
use crate::sync::keys;

/// Name used in the device list when this installation has no alias yet.
pub(crate) const FALLBACK_DEVICE_NAME: &str = "holzi";

/// What [`ensure_sync_state`] found or created.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SyncState {
    /// `None` when the vault has no identity yet and genesis was not allowed.
    pub vault_pubkey: Option<[u8; 32]>,
    pub device_pubkey: Option<[u8; 32]>,
    /// Whether this device holds the vault secret.
    pub is_main: bool,
    /// Whether this run issued the first device list.
    pub issued_first_list: bool,
    /// Whether this run enrolled this device as a main device because the
    /// vault file is a copy of a main device's.
    pub enrolled_as_copy: bool,
}

/// Brings the vault's sync state up to date; see the module docs.
/// `allow_genesis` lets a vault without identity and seed mint a new one.
pub fn ensure_sync_state(
    db: &Database,
    installation_uuid: Uuid,
    allow_genesis: bool,
) -> haex_crdt::Result<SyncState> {
    let vault_device_uuid = db.device_id();
    let now = now_ms();
    db.write(|tx| {
        let Some(vault_pubkey) = keys::ensure_vault_identity(tx, allow_genesis)? else {
            return Ok(SyncState {
                vault_pubkey: None,
                device_pubkey: None,
                is_main: false,
                issued_first_list: false,
                enrolled_as_copy: false,
            });
        };
        let had_keys = keys::load_device_keys(tx, installation_uuid)?.is_some();
        let device = keys::ensure_device_keys(tx, installation_uuid, now_i64(now))?;
        let vault_secret = keys::vault_secret(tx)?;

        let mut issued_first_list = false;
        if let Some(vault_secret) = vault_secret.as_ref() {
            if device_list::load_all(tx)?.is_empty() {
                let name = device_name(tx, installation_uuid)?;
                let key = ContentKey::generate(1);
                let list = DeviceList {
                    vault: vault_pubkey,
                    generation: 1,
                    devices: vec![ListedDevice {
                        device_pubkey: device.device_pubkey,
                        endpoint_id: device.endpoint_id,
                        role: Role::Main,
                        vault_device_uuid,
                        name_sealed: content_keys::seal_name(&key, &device.device_pubkey, &name),
                        added_at: now,
                    }],
                    removed: Vec::new(),
                    issued_by: device.device_pubkey,
                    issued_at: now,
                    base_list_hash: None,
                };
                let signed = device_list::sign_list(list, vault_secret)
                    .map_err(haex_crdt::Error::consumer)?;
                device_list::insert(tx, &signed)?;
                crate::contacts::card::own_card::refresh_quietly(tx);
                content_keys::issue_generation(tx, &key, &signed, &device, now)?;
                issued_first_list = true;
            }
        }

        let enrolled_as_copy = if !had_keys && !issued_first_list {
            open_copy(
                tx,
                &device,
                vault_secret.as_deref(),
                vault_pubkey,
                vault_device_uuid,
                now,
            )?
        } else {
            false
        };

        let valid = device_list::valid_lists(&device_list::load_all(tx)?, &vault_pubkey);
        content_keys::unwrap_own_envelopes(tx, &device, &valid)?;
        Ok(SyncState {
            vault_pubkey: Some(vault_pubkey),
            device_pubkey: Some(device.device_pubkey),
            is_main: vault_secret.is_some(),
            issued_first_list,
            enrolled_as_copy,
        })
    })
}

/// A vault file opened by an installation that has no keys in it yet while
/// its list does not name this device: a copy. The copy takes the computer's
/// name while its own is still the default, so the devices tell apart. A copy
/// of a main device's file (`secret` is there) enrolls itself as a main device
/// and leaves the notice for the user; a copy of a linked device's file has
/// nothing more to do here: it asks for admission once presence runs.
fn open_copy(
    tx: &mut haex_crdt::CrdtTransaction<'_>,
    device: &keys::DeviceKeys,
    secret: Option<&[u8; 32]>,
    vault_pubkey: [u8; 32],
    vault_device_uuid: Uuid,
    now: u64,
) -> haex_crdt::Result<bool> {
    let valid = device_list::valid_lists(&device_list::load_all(tx)?, &vault_pubkey);
    let Some(effective) = device_list::effective(&valid).cloned() else {
        return Ok(false);
    };
    if !admission::needs_request(&effective, &device.device_pubkey) {
        return Ok(false);
    }
    admission::adopt_computer_name(tx, vault_device_uuid)?;
    let Some(secret) = secret else {
        return Ok(false);
    };
    let name = admission::own_name(tx, vault_device_uuid)?;
    let enrolled = admission::enroll_as_main(
        tx,
        device,
        secret,
        &effective,
        vault_device_uuid,
        &name,
        now,
    )?;
    if enrolled {
        crate::storage::preferences::insert_or_update(
            tx,
            crate::storage::preferences::PrefScope::Device(vault_device_uuid),
            admission::PREF_ENROLLED_AS_MAIN,
            "1",
        )?;
    }
    Ok(enrolled)
}

/// Runs [`ensure_sync_state`] after a vault opened, before the frontend sees
/// it. A failure is logged and does not stop the vault from opening; the next
/// open retries, since every step is idempotent.
pub fn run_after_open(db: &Database, installation_id_file: &std::path::Path, allow_genesis: bool) {
    let installation_uuid =
        match crate::identity::read_or_mint_installation_uuid(installation_id_file) {
            Ok(uuid) => uuid,
            Err(e) => {
                log::warn!("sync: no installation id, sync state not checked: {e}");
                return;
            }
        };
    if let Err(e) = ensure_sync_state(db, installation_uuid, allow_genesis) {
        log::warn!("sync: could not bring the sync state up to date: {e}");
    }
}

/// This installation's alias, the name the device list shows.
fn device_name(q: &mut impl Query, installation_uuid: Uuid) -> haex_crdt::Result<String> {
    let alias: Option<Option<String>> = q.query_row(
        "SELECT alias FROM known_devices WHERE installation_uuid = ?1",
        params![installation_uuid.to_string()],
        |r| r.get(0),
    )?;
    Ok(alias
        .flatten()
        .filter(|alias| !alias.trim().is_empty())
        .unwrap_or_else(|| FALLBACK_DEVICE_NAME.to_string()))
}

fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

fn now_i64(now: u64) -> i64 {
    i64::try_from(now).unwrap_or(i64::MAX)
}

#[cfg(test)]
#[path = "genesis_tests.rs"]
mod tests;
