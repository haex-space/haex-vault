//! The settings of the MCP access (spec 021 FR-001, data-model.md): on or off per device, in the
//! preferences of this device. Off until the user turns it on; a guardrail no agent can change.

use haex_crdt::{CrdtTransaction, Result};
use uuid::Uuid;

use crate::storage::preferences::{self, PrefScope};
use crate::storage::query::Query;

pub const KEY_ENABLED: &str = "mcp.enabled";

/// Whether the access is on for `device`; a missing or unreadable value is off.
pub fn read_enabled(q: &mut impl Query, device: Uuid) -> Result<bool> {
    Ok(preferences::parse_bool(
        preferences::get(q, PrefScope::Device(device), KEY_ENABLED)?.as_deref(),
    )
    .unwrap_or(false))
}

pub fn write_enabled(tx: &mut CrdtTransaction<'_>, device: Uuid, enabled: bool) -> Result<()> {
    preferences::insert_or_update(
        tx,
        PrefScope::Device(device),
        KEY_ENABLED,
        if enabled { "true" } else { "false" },
    )
    .map(|_| ())
}
