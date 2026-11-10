//! Taking a vault name (spec 084, research R3): the vault's folder is made with `create_dir`,
//! which fails when the name is taken, so two creations of one name cannot both succeed. The
//! pending marker follows inside it, before the database, so a crash leaves a folder that the
//! startup clean-up recognises (`startup`, `cleanup::take_back`).

use std::fs::{self, OpenOptions};
use std::io;
use std::path::Path;

use super::paths::get_pending_marker_path;

/// Reserves the folder of the vault whose database will be `db_path` (`VaultPaths::database`) and
/// puts the pending marker in it. `ErrorKind::AlreadyExists` means the name is taken; any other
/// error leaves nothing behind.
pub fn reserve(db_path: &Path) -> io::Result<()> {
    let Some(folder) = db_path.parent() else {
        return Err(io::Error::new(io::ErrorKind::InvalidInput, "no folder"));
    };
    if let Some(vaults) = folder.parent() {
        fs::create_dir_all(vaults)?;
    }
    fs::create_dir(folder)?;
    let marker = OpenOptions::new()
        .create_new(true)
        .write(true)
        .open(get_pending_marker_path(db_path));
    if let Err(error) = marker {
        let _ = fs::remove_dir(folder);
        return Err(error);
    }
    Ok(())
}

#[cfg(test)]
#[path = "reserve_tests.rs"]
mod tests;
