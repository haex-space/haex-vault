//! Own-device sync (spec 024): vault identity, device keys, device list,
//! content keys, the exchange of changes by progress per origin, and the
//! per-session sync service. The transport follows with user story 1.

pub mod admission;
pub mod change;
pub mod commands;
pub mod content_keys;
pub mod device_list;
pub mod device_view;
pub mod endpoint;
pub mod envelopes;
pub mod events;
pub mod foreign_auth;
pub mod genesis;
pub mod handshake;
pub(crate) mod home_relay;
pub mod inbound;
pub mod keys;
pub mod link;
pub mod outbound;
pub mod peer_schema;
pub mod presence;
pub mod presence_loop;
pub mod problems;
pub mod progress;
pub mod registry;
pub mod removal;
pub mod replica;
pub mod resume;
pub mod resync;
pub mod seen;
pub mod servers;
mod service;
mod service_protocols;
pub mod session;
pub mod signing;
pub mod wire;

pub use service::{start_for_active_instance, SyncService};

/// Reconnects to devices presence has an address for but that this session
/// is not currently connected to (spec 024, FR-010): after a drop, a
/// network change, or simply having just learned of a device presence has
/// not been able to reach until now. Best-effort: a device still
/// unreachable is left for the next call, never treated as an error.
pub fn reconnect_missing(node: &endpoint::SyncNode, replica: &replica::Replica) {
    let rows = match crate::storage::query::read(replica.db(), |r| presence::load_all(r)) {
        Ok(rows) => rows,
        Err(error) => {
            log::warn!("sync: reconnect could not read presence: {error}");
            return;
        }
    };
    let known: std::collections::HashSet<[u8; 32]> =
        match crate::storage::query::read(replica.db(), |r| {
            let vault = keys::vault_pubkey(r)?.unwrap_or([0; 32]);
            let valid = device_list::valid_lists(&device_list::load_all(r)?, &vault);
            Ok(device_list::effective(&valid)
                .map(|e| e.list.devices.iter().map(|d| d.device_pubkey).collect())
                .unwrap_or_default())
        }) {
            Ok(known) => known,
            Err(error) => {
                log::warn!("sync: reconnect could not read the device list: {error}");
                return;
            }
        };
    let connected: std::collections::HashSet<[u8; 32]> = node.connected().into_iter().collect();
    let own = node.device_pubkey();
    for row in rows {
        if row.device_pubkey == own
            || !known.contains(&row.device_pubkey)
            || connected.contains(&row.device_pubkey)
        {
            continue;
        }
        if let Some(addr) = row.endpoint_addr {
            node.dial(addr);
        }
    }
    // A device the list does not name yet announced a newer list: one dial
    // fetches it, and the handshake then decides (FR-007).
    for addr in node.take_candidates() {
        node.dial(addr);
    }
}

#[cfg(test)]
pub(crate) mod test_support;

#[cfg(test)]
#[path = "mod_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "large_preference_tests.rs"]
mod large_preference_tests;

#[cfg(test)]
#[path = "icon_set_sync_tests.rs"]
mod icon_set_sync_tests;
