//! The version state a device showed in its last handshake with this device (spec 059,
//! data-model.md `device_presence_no_sync.peer_*`).
//!
//! Device-local like the rest of that table: nothing here syncs. A handshake writes the state of
//! the other side whether it accepts or refuses, so a device that cannot sync with this one still
//! shows why. Only a complete state counts; a row with some of the three values missing reads as
//! unknown. Writing replaces the state and leaves `last_seen`, `endpoint_addr` and `problem` alone.

use haex_crdt::rusqlite::params;

use crate::storage::query::Query;
use crate::sync::replica::Replica;
use crate::sync::wire::SchemaVersion;

/// Records `schema` as the state of `device`; returns whether that changed anything.
pub fn record(
    replica: &Replica,
    device: &[u8; 32],
    schema: SchemaVersion,
) -> haex_crdt::Result<bool> {
    replica.db().write(|tx| {
        let known: Option<(Option<i64>, Option<i64>, Option<i64>)> = tx.query_row(
            "SELECT peer_protocol, peer_migration, peer_trigger \
             FROM device_presence_no_sync WHERE device_pubkey = ?1",
            params![device.as_slice()],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
        )?;
        let values = params![
            device.as_slice(),
            i64::from(schema.protocol),
            i64::from(schema.holzi_migration),
            i64::from(schema.crdt_trigger),
        ];
        if let Some((protocol, migration, trigger)) = known {
            if from_columns(protocol, migration, trigger) == Some(schema) {
                return Ok(false);
            }
            tx.execute(
                "UPDATE device_presence_no_sync \
                 SET peer_protocol = ?2, peer_migration = ?3, peer_trigger = ?4 \
                 WHERE device_pubkey = ?1",
                values,
            )?;
            return Ok(true);
        }
        // A device never seen keeps `last_seen = 0`; only a meeting or a handshake gives it a
        // real time (as in `problems::set`).
        tx.execute(
            "INSERT INTO device_presence_no_sync \
               (device_pubkey, last_seen, endpoint_addr, problem, \
                peer_protocol, peer_migration, peer_trigger) \
             VALUES (?1, 0, NULL, NULL, ?2, ?3, ?4)",
            values,
        )?;
        Ok(true)
    })
}

/// The state recorded for `device`, if all three values are there.
pub fn of(q: &mut impl Query, device: &[u8; 32]) -> haex_crdt::Result<Option<SchemaVersion>> {
    let row: Option<(Option<i64>, Option<i64>, Option<i64>)> = q.query_row(
        "SELECT peer_protocol, peer_migration, peer_trigger \
         FROM device_presence_no_sync WHERE device_pubkey = ?1",
        params![device.as_slice()],
        |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
    )?;
    Ok(row.and_then(|(protocol, migration, trigger)| from_columns(protocol, migration, trigger)))
}

/// The state the three stored values make; `None` unless all three are there and fit (the one rule
/// for what counts as known, shared with the device list's own query).
pub fn from_columns(
    protocol: Option<i64>,
    migration: Option<i64>,
    trigger: Option<i64>,
) -> Option<SchemaVersion> {
    Some(SchemaVersion {
        protocol: u32::try_from(protocol?).ok()?,
        holzi_migration: u32::try_from(migration?).ok()?,
        crdt_trigger: u32::try_from(trigger?).ok()?,
    })
}

#[cfg(test)]
#[path = "peer_schema_tests.rs"]
mod tests;
