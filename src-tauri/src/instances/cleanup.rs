//! Taking back a vault that did not come into being (spec 043, data-model.md "Übernahme einer
//! Tresordatei"): a failed create or import, or an orphan found at startup. Every file of the vault
//! goes, the pending marker last, so a crash half way still leaves the marker that tells the next
//! startup to finish the job. Then its folder goes (spec 084, FR-007).

use std::fs;
use std::io::ErrorKind;
use std::path::Path;

use super::paths::get_pending_marker_path;

/// The files beside `<name>.db` that belong to it, by the suffix added to its file name.
pub const SIDECAR_SUFFIXES: [&str; 4] = [".lock", "-wal", "-shm", ".vault-id"];

fn remove(path: &Path) -> bool {
    match fs::remove_file(path) {
        Ok(()) => true,
        Err(error) if error.kind() == ErrorKind::NotFound => true,
        Err(error) => {
            log::warn!("could not remove {path:?}: {error}");
            false
        }
    }
}

/// Removes `<name>.db`, its sidecar files, temporary copies (`<name>.db.*.tmp`) and, when all of
/// them are gone, the pending marker. Returns whether everything went.
pub fn remove_vault_files(db_path: &Path) -> bool {
    let Some(file_name) = db_path.file_name().and_then(|name| name.to_str()) else {
        return false;
    };
    let mut all = remove(db_path);
    for suffix in SIDECAR_SUFFIXES {
        all &= remove(&db_path.with_file_name(format!("{file_name}{suffix}")));
    }
    if let Some(dir) = db_path.parent() {
        let prefix = format!("{file_name}.");
        if let Ok(entries) = fs::read_dir(dir) {
            for entry in entries.flatten() {
                let name = entry.file_name();
                let Some(name) = name.to_str() else { continue };
                if name.starts_with(&prefix) && name.ends_with(".tmp") {
                    all &= remove(&entry.path());
                }
            }
        }
    }
    if all {
        all = remove(&get_pending_marker_path(db_path));
    }
    all
}

/// Takes back the vault whose database is `db_path` (`VaultPaths::database`): its files as in
/// [`remove_vault_files`], then its folder with everything in it. The folder stays while a file
/// could not go, so the marker keeps telling the next start to finish.
pub fn take_back(db_path: &Path) -> bool {
    if !remove_vault_files(db_path) {
        return false;
    }
    let Some(folder) = db_path.parent() else {
        return false;
    };
    match fs::remove_dir_all(folder) {
        Ok(()) => true,
        Err(error) if error.kind() == ErrorKind::NotFound => true,
        Err(error) => {
            log::warn!("could not remove {folder:?}: {error}");
            false
        }
    }
}

#[cfg(test)]
#[path = "cleanup_tests.rs"]
mod tests;
