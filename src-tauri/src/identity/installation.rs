//! Installation UUID — one random UUID per haex-vault installation on a host,
//! stored in `<AppLocalData>/installation-id`. Never leaves the device.
//!
//! Contract: `tauri-commands.md` §"Vault identity and device model" — the
//! second bullet ("Installation UUID"). Kept as a file outside any vault so
//! that a copied `.db` reopened by the same installation can look up its
//! existing `known_devices` row.

use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

use uuid::Uuid;

/// The bare filename haex-vault writes under `<AppLocalData>/`.
pub const INSTALLATION_ID_FILENAME: &str = "installation-id";

/// Resolves the installation-id path from an `AppLocalData` root. Kept
/// separate from the Tauri path resolver so tests can point at a temp dir.
pub fn installation_id_path(app_local_data: &Path) -> PathBuf {
    app_local_data.join(INSTALLATION_ID_FILENAME)
}

/// Reads the existing installation UUID from `path`, or mints and fsyncs a
/// fresh one if the file does not exist. Idempotent — repeated calls always
/// return the same UUID.
pub fn read_or_mint_installation_uuid(path: &Path) -> std::io::Result<Uuid> {
    match fs::read_to_string(path) {
        Ok(s) => Uuid::parse_str(s.trim())
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e)),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => mint_and_fsync(path),
        Err(e) => Err(e),
    }
}

/// Creates and durably writes a new installation UUID at `path`.
///
/// The UUID is written to a temporary file beside `path` and published only when complete, never
/// over an existing file: a second process minting at the same time keeps the first one's UUID.
/// `persist_noclobber` publishes with `renameat2(RENAME_NOREPLACE)` on Linux and Android; Android
/// forbids apps hard links (spec 043, found on the emulator), which the old way used.
fn mint_and_fsync(path: &Path) -> std::io::Result<Uuid> {
    let parent = match path.parent() {
        Some(parent) if !parent.as_os_str().is_empty() => parent,
        _ => Path::new("."),
    };
    fs::create_dir_all(parent)?;
    let fresh = Uuid::new_v4();

    let mut tmp = tempfile::Builder::new()
        .prefix(&format!("{INSTALLATION_ID_FILENAME}."))
        .suffix(".tmp")
        .tempfile_in(parent)?;
    tmp.write_all(fresh.to_string().as_bytes())?;
    tmp.as_file().sync_all()?;

    match tmp.persist_noclobber(path) {
        Ok(_) => Ok(fresh),
        Err(error) if error.error.kind() == std::io::ErrorKind::AlreadyExists => {
            let existing = fs::read_to_string(path)?;
            Uuid::parse_str(existing.trim()).map_err(|parse_error| {
                std::io::Error::new(std::io::ErrorKind::InvalidData, parse_error)
            })
        }
        Err(error) => Err(error.error),
    }
}
