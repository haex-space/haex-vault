//! The preset marketplace of a new vault (spec 069, FR-022, SC-006, research R10): one ordinary
//! row, written once when the vault is created, after the sync state exists so the row syncs like
//! any other. A failure is logged and does not stop the vault from being created: the empty
//! state of the catalog offers the preset again.

use haex_crdt::Database;

use super::store;
use crate::passwords::clock::unix_millis;

/// Adds the preset row to a vault that was just created.
pub fn seed_preset_after_creation(db: &Database) {
    let now = unix_millis(std::time::SystemTime::now());
    let written = db.write(|tx| {
        store::add_preset(tx, now)?;
        Ok(())
    });
    if let Err(error) = written {
        log::warn!("marketplace: the preset could not be added to the new vault: {error}");
    }
}
