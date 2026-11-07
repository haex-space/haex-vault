//! `vault.db.vault-id` (spec 043, data-model.md): the SHA-256 of the vault identity's public key
//! as hex, beside the database of every vault of this installation. It holds no secret; it lets an imported vault
//! file be recognised as a second copy of a vault already on this device (contract
//! `import-instance.md`, step 5). Written on create, open and import.

use std::fs;
use std::path::{Path, PathBuf};

use haex_crdt::Database;
use sha2::{Digest, Sha256};

use super::vault_paths::DATABASE_FILE;

const SUFFIX: &str = ".vault-id";

/// The fingerprint file of the database `db_path`.
pub fn vault_id_path(db_path: &Path) -> PathBuf {
    let mut name = db_path.as_os_str().to_owned();
    name.push(SUFFIX);
    PathBuf::from(name)
}

/// The fingerprint of a vault identity's public key.
pub fn fingerprint_of(pubkey: &[u8; 32]) -> String {
    Sha256::digest(pubkey)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

/// The fingerprint of an open vault; `None` while it has no published vault identity.
pub fn fingerprint(db: &Database) -> Option<String> {
    match crate::storage::query::read(db, |q| crate::sync::keys::vault_pubkey(q)) {
        Ok(pubkey) => pubkey.as_ref().map(fingerprint_of),
        Err(error) => {
            log::warn!("vault id: reading the vault identity failed: {error}");
            None
        }
    }
}

/// Writes the fingerprint beside the vault. A missing sidecar would make duplicate-vault detection
/// unreliable, so callers must handle a write failure before publishing the vault.
pub fn write(db_path: &Path, db: &Database) -> std::io::Result<()> {
    let Some(fingerprint) = fingerprint(db) else {
        return Ok(());
    };
    let path = vault_id_path(db_path);
    if fs::read_to_string(&path).is_ok_and(|text| text.trim() == fingerprint) {
        return Ok(());
    }
    if let Err(error) = fs::write(&path, format!("{fingerprint}\n")) {
        log::warn!("vault id: writing {path:?} failed: {error}");
        return Err(error);
    }
    Ok(())
}

/// The vault in `vaults_dir`, other than `except`, whose fingerprint file says `fingerprint`.
pub fn owner_of(vaults_dir: &Path, fingerprint: &str, except: &str) -> Option<String> {
    let file = format!("{DATABASE_FILE}{SUFFIX}");
    fs::read_dir(vaults_dir).ok()?.flatten().find_map(|entry| {
        let folder = entry.file_name();
        let name = folder.to_str()?;
        if name == except || name.starts_with('.') {
            return None;
        }
        let text = fs::read_to_string(entry.path().join(&file)).ok()?;
        (text.trim() == fingerprint).then(|| name.to_string())
    })
}

#[cfg(test)]
#[path = "vault_id_tests.rs"]
mod tests;
