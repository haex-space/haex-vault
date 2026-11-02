//! The settings of the MCP access (spec 021 FR-001, data-model.md): on or off per device, in the
//! preferences of this device. Off until the user turns it on; a guardrail no agent can change.

use haex_crdt::{CrdtTransaction, Result};
use uuid::Uuid;

use crate::storage::preferences::{self, PrefScope};
use crate::storage::query::Query;

pub const KEY_ENABLED: &str = "mcp.enabled";
/// The port of the HTTP way: chosen on its first start, kept per device (data-model.md, R14).
pub const KEY_HTTP_PORT: &str = "mcp.http_port";

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

/// The kept port of the HTTP way; `None` before its first start or when unreadable.
pub fn read_http_port(q: &mut impl Query, device: Uuid) -> Result<Option<u16>> {
    Ok(
        preferences::get(q, PrefScope::Device(device), KEY_HTTP_PORT)?
            .and_then(|value| value.parse::<u16>().ok())
            .filter(|port| *port != 0),
    )
}

pub fn write_http_port(tx: &mut CrdtTransaction<'_>, device: Uuid, port: u16) -> Result<()> {
    preferences::insert_or_update(
        tx,
        PrefScope::Device(device),
        KEY_HTTP_PORT,
        &port.to_string(),
    )
    .map(|_| ())
}

/// Whether this device has the MCP access at all: the program way or the HTTP way.
pub fn available() -> bool {
    let capabilities = crate::platform::capabilities();
    capabilities.mcp_helper || capabilities.mcp_http
}
