//! The settings of the bridge (spec 049, FR-001, FR-002, data-model): on/off and the port, both
//! per device, in the preferences of this device.

use haex_crdt::{CrdtTransaction, Result};
use uuid::Uuid;

use crate::storage::preferences::{self, PrefScope};
use crate::storage::query::Query;

pub const KEY_ENABLED: &str = "browserBridge.enabled";
pub const KEY_PORT: &str = "browserBridge.port";
/// The port of haex-vault 2.x (Legacy), so the extension connects without changing its setting.
pub const DEFAULT_PORT: u16 = 19455;
const LOWEST_PORT: u16 = 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BridgeSettings {
    pub enabled: bool,
    pub port: u16,
}

impl Default for BridgeSettings {
    /// Off until the user turns it on (FR-001).
    fn default() -> Self {
        Self {
            enabled: false,
            port: DEFAULT_PORT,
        }
    }
}

/// A port outside 1024–65535; the low ports need privileges.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
#[error("the port must be between 1024 and 65535")]
pub struct InvalidPort;

pub fn validate_port(port: u16) -> std::result::Result<u16, InvalidPort> {
    if port >= LOWEST_PORT {
        Ok(port)
    } else {
        Err(InvalidPort)
    }
}

/// The settings of `device`; a missing or unreadable value counts as the default.
pub fn read(q: &mut impl Query, device: Uuid) -> Result<BridgeSettings> {
    let scope = PrefScope::Device(device);
    let defaults = BridgeSettings::default();
    let enabled = preferences::parse_bool(preferences::get(q, scope, KEY_ENABLED)?.as_deref())
        .unwrap_or(defaults.enabled);
    let port = preferences::get(q, scope, KEY_PORT)?
        .and_then(|value| value.parse::<u16>().ok())
        .and_then(|port| validate_port(port).ok())
        .unwrap_or(defaults.port);
    Ok(BridgeSettings { enabled, port })
}

/// Saves both values for `device`. The caller has validated the port.
pub fn write(tx: &mut CrdtTransaction<'_>, device: Uuid, settings: BridgeSettings) -> Result<()> {
    let scope = PrefScope::Device(device);
    preferences::insert_or_update(
        tx,
        scope,
        KEY_ENABLED,
        if settings.enabled { "true" } else { "false" },
    )?;
    preferences::insert_or_update(tx, scope, KEY_PORT, &settings.port.to_string())?;
    Ok(())
}
