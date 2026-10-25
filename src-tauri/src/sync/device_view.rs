//! The vault's devices as the settings list them (spec 024, FR-033 to
//! FR-035, contracts/tauri-commands.md `VaultDevice`).
//!
//! The effective device list says who belongs to the vault and in which
//! role; `known_devices` says what each is called; the device-local
//! presence table says when it was last online and whether sync with it is
//! halted; the node says who is connected right now. [`build`] puts these
//! together without touching storage, so it is testable alone.

use std::collections::{HashMap, HashSet};

use serde::Serialize;
use ts_rs::TS;
use uuid::Uuid;

use crate::storage::known_devices::{self, KnownDevice};
use crate::storage::query::Query;
use crate::sync::content_keys::{self, ContentKey};
use crate::sync::device_list::{self, Role, SignedList};
use crate::sync::handshake::local_schema;
use crate::sync::keys::{self, hex};
use crate::sync::peer_schema;
use crate::sync::problems::Problem;
use crate::sync::wire::SchemaVersion;

/// A device's role in the vault.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, TS)]
#[ts(export, export_to = "../../src/types/bindings/")]
#[serde(rename_all = "snake_case")]
pub enum DeviceRole {
    Main,
    Linked,
}

/// What stops sync with a device (FR-029, FR-030).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, TS)]
#[ts(export, export_to = "../../src/types/bindings/")]
#[serde(rename_all = "snake_case")]
pub enum DeviceProblem {
    IncompatibleVersion,
    Duplicate,
}

impl From<Problem> for DeviceProblem {
    fn from(problem: Problem) -> Self {
        match problem {
            Problem::IncompatibleVersion => DeviceProblem::IncompatibleVersion,
            Problem::Duplicate => DeviceProblem::Duplicate,
        }
    }
}

/// The version state of a device: what two devices compare before they sync (spec 059, FR-001).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, TS)]
#[ts(export, export_to = "../../src/types/bindings/")]
#[serde(rename_all = "camelCase")]
pub struct DeviceSchema {
    /// The version of the sync protocol.
    pub protocol: u32,
    /// How many holzi migrations the device knows: how the vault's tables are built.
    pub holzi_migration: u32,
    /// The version of the rules that track changes for the sync.
    pub crdt_trigger: u32,
}

impl From<SchemaVersion> for DeviceSchema {
    fn from(schema: SchemaVersion) -> Self {
        Self {
            protocol: schema.protocol,
            holzi_migration: schema.holzi_migration,
            crdt_trigger: schema.crdt_trigger,
        }
    }
}

/// One device of the vault for the settings' device list.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(export, export_to = "../../src/types/bindings/")]
#[serde(rename_all = "camelCase")]
pub struct VaultDevice {
    #[ts(type = "string")]
    pub vault_device_uuid: Uuid,
    /// The device key as hex.
    pub device_pubkey: String,
    /// `None` for a device that has no name yet.
    pub alias: Option<String>,
    pub role: DeviceRole,
    pub is_current: bool,
    /// Whether a session with it is open right now.
    pub online: bool,
    /// Milliseconds since the epoch; `None` when this device knows of no time.
    #[ts(type = "number | null")]
    pub last_seen: Option<u64>,
    pub problem: Option<DeviceProblem>,
    /// This device's own state for the row of this device; for the others the state they showed in
    /// their last handshake with this device, `None` while it never saw one (spec 059, FR-007).
    pub schema: Option<DeviceSchema>,
}

/// What the device-local presence table knows of a device.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Facts {
    pub last_seen_ms: u64,
    pub problem: Option<Problem>,
    pub schema: Option<SchemaVersion>,
}

/// Everything [`build`] needs.
pub struct Inputs<'a> {
    /// The effective device list, `None` while the vault has none.
    pub list: Option<&'a SignedList>,
    pub known: &'a [KnownDevice],
    pub facts: &'a HashMap<[u8; 32], Facts>,
    pub connected: &'a HashSet<[u8; 32]>,
    pub own_device: [u8; 32],
    /// The state of this build, shown on the row of this device.
    pub own_schema: SchemaVersion,
    /// A content key to open sealed names with, for a device without an
    /// alias of its own here.
    pub key: Option<&'a ContentKey>,
}

/// The devices to show: this device first, then the others by name
/// ignoring case, devices without a name last. A device removed from the
/// list is gone (FR-034).
pub fn build(inputs: &Inputs<'_>) -> Vec<VaultDevice> {
    let by_uuid: HashMap<Uuid, &KnownDevice> = inputs
        .known
        .iter()
        .map(|known| (known.vault_device_uuid, known))
        .collect();
    let mut devices: Vec<VaultDevice> = match inputs.list {
        Some(signed) => signed
            .list
            .devices
            .iter()
            .map(|listed| {
                let facts = inputs
                    .facts
                    .get(&listed.device_pubkey)
                    .copied()
                    .unwrap_or_default();
                let alias = by_uuid
                    .get(&listed.vault_device_uuid)
                    .and_then(|known| known.alias.clone())
                    .filter(|alias| !alias.trim().is_empty())
                    .or_else(|| {
                        inputs.key.and_then(|key| {
                            content_keys::open_name(key, &listed.device_pubkey, &listed.name_sealed)
                                .ok()
                        })
                    });
                let is_current = listed.device_pubkey == inputs.own_device;
                VaultDevice {
                    vault_device_uuid: listed.vault_device_uuid,
                    device_pubkey: hex(&listed.device_pubkey),
                    alias,
                    role: match listed.role {
                        Role::Main => DeviceRole::Main,
                        Role::Linked => DeviceRole::Linked,
                    },
                    is_current,
                    online: is_current || inputs.connected.contains(&listed.device_pubkey),
                    last_seen: (facts.last_seen_ms > 0).then_some(facts.last_seen_ms),
                    problem: facts.problem.map(Into::into),
                    schema: if is_current {
                        Some(inputs.own_schema.into())
                    } else {
                        facts.schema.map(Into::into)
                    },
                }
            })
            .collect(),
        None => Vec::new(),
    };
    devices.sort_by_cached_key(|device| {
        (
            !device.is_current,
            device.alias.is_none(),
            device.alias.as_deref().map(str::to_lowercase),
        )
    });
    devices
}

/// Reads what [`build`] needs and builds the list. `connected` is who has a
/// session right now (the node's view), empty when the service is not up.
pub fn load(
    q: &mut impl Query,
    installation_uuid: Uuid,
    connected: &HashSet<[u8; 32]>,
) -> haex_crdt::Result<Vec<VaultDevice>> {
    let Some(own) = keys::load_device_keys(q, installation_uuid)? else {
        return Ok(Vec::new());
    };
    let vault = keys::vault_pubkey(q)?.unwrap_or([0; 32]);
    let valid = device_list::valid_lists(&device_list::load_all(q)?, &vault);
    let known = known_devices::list_devices(q)?;
    let key = content_keys::current_key(q, &[])?;
    type Row = (
        Vec<u8>,
        i64,
        Option<String>,
        Option<i64>,
        Option<i64>,
        Option<i64>,
    );
    let rows: Vec<Row> = q.query_map(
        "SELECT device_pubkey, last_seen, problem, peer_protocol, peer_migration, peer_trigger \
         FROM device_presence_no_sync",
        &[],
        |r| {
            Ok((
                r.get(0)?,
                r.get(1)?,
                r.get(2)?,
                r.get(3)?,
                r.get(4)?,
                r.get(5)?,
            ))
        },
    )?;
    let facts: HashMap<[u8; 32], Facts> = rows
        .into_iter()
        .filter_map(|(device, seen, problem, protocol, migration, trigger)| {
            Some((
                device.try_into().ok()?,
                Facts {
                    last_seen_ms: u64::try_from(seen).unwrap_or(0),
                    problem: problem.as_deref().and_then(Problem::parse),
                    schema: peer_schema::from_columns(protocol, migration, trigger),
                },
            ))
        })
        .collect();
    Ok(build(&Inputs {
        list: device_list::effective(&valid),
        known: &known,
        facts: &facts,
        connected,
        own_device: own.device_pubkey,
        own_schema: local_schema(),
        key: key.as_ref(),
    }))
}

#[cfg(test)]
#[path = "device_view_tests.rs"]
mod tests;
