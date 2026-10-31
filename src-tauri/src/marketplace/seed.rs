//! The preset marketplace is always there (spec 069, FR-022, SC-006, research R10): one ordinary
//! row with a fixed id, added when a vault is created and whenever a vault that lacks it is opened
//! (a vault from before this feature). Only a main device adds it: a copy gets the row from the
//! sync, so a copy that opens before its first sync cannot switch the marketplace back on. The
//! row is written after the sync state exists, so it syncs like any other. A failure is logged
//! and does not stop the vault from opening.

use haex_crdt::Database;

use super::store;
use crate::passwords::clock::unix_millis;
use crate::sync::keys;

/// Adds the preset row on a main device if it is missing; a row that is there stays as it is,
/// even switched off.
pub fn ensure_preset(db: &Database) {
    let now = unix_millis(std::time::SystemTime::now());
    let written = db.write(|tx| {
        if keys::vault_secret(tx)?.is_some() {
            store::add_preset(tx, now)?;
        }
        Ok(())
    });
    if let Err(error) = written {
        log::warn!("marketplace: the preset could not be added: {error}");
    }
}
