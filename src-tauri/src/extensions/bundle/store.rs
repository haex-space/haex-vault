//! Bundles as vault data (research R4): every file is a content-addressed BLOB in
//! `extension_blobs`, the bundle row keeps the exact manifest and signature bytes, and files are
//! served straight from the database, re-hashed on every read.
//!
//! Writes use check-then-write instead of `ON CONFLICT`, like the other CRDT tables
//! (`storage/preferences.rs`). Each BLOB is written in a write of its own, so one transaction group
//! stays small for the sync; the registry rows follow in one write that the caller opens.

use haex_crdt::rusqlite::params;
use haex_crdt::CrdtTransaction;
use sha2::{Digest, Sha256};
use uuid::Uuid;

use super::{signed_message_hex, BundleRejection, Manifest, VerifiedBundle};
use crate::error::{HolziError, Result};
use crate::extensions::default_limits;
use crate::extensions::ids::{bundle_file_id, bundle_id, extension_id, limits_id, migration_id};
use crate::storage::query::Query;
use crate::sync::keys::hex;
use crate::vault_gate::VaultDb;

fn sha256_hex(data: &[u8]) -> String {
    hex(&Sha256::digest(data))
}

fn exists(q: &mut impl Query, sql: &str, key: &str) -> Result<bool> {
    Ok(q.query_row(sql, &[&key], |r| r.get::<_, i64>(0))?
        .unwrap_or(0)
        > 0)
}

/// The ids of a stored bundle.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BundleIds {
    pub extension_id: Uuid,
    pub bundle_id: Uuid,
}

impl BundleIds {
    pub fn of(bundle: &VerifiedBundle, manifest: &Manifest) -> Self {
        Self {
            extension_id: extension_id(&manifest.public_key, &manifest.name),
            bundle_id: bundle_id(&signed_message_hex(bundle)),
        }
    }
}

/// Makes sure the BLOB of `data` exists; a known one loses its orphan mark.
pub fn ensure_blob(tx: &mut CrdtTransaction<'_>, data: &[u8]) -> Result<String> {
    let hash = sha256_hex(data);
    if exists(
        tx,
        "SELECT COUNT(*) FROM extension_blobs WHERE hash = ?1",
        &hash,
    )? {
        tx.execute(
            "UPDATE extension_blobs SET orphaned_at = NULL \
             WHERE hash = ?1 AND orphaned_at IS NOT NULL",
            params![hash],
        )?;
    } else {
        tx.execute(
            "INSERT INTO extension_blobs (hash, data, size) VALUES (?1, ?2, ?3)",
            params![hash, data, data.len() as i64],
        )?;
    }
    Ok(hash)
}

/// Writes every file of `bundle` as a BLOB, each in a write of its own. Blocking.
pub fn store_blobs(db: &VaultDb, bundle: &VerifiedBundle) -> Result<()> {
    for entry in &bundle.entries {
        let data = entry.data.clone();
        db.write_blocking(move |tx| ensure_blob(tx, &data).map(drop).map_err(Into::into))?;
    }
    Ok(())
}

/// Writes the registry rows of a verified bundle inside the caller's write: the `extensions` row
/// (new: `state = 'installed'`; known: back to `installed`, `enabled` and the purge fields
/// untouched, R11), the bundle row with the exact manifest and signature bytes (known: `retired`
/// back to 0, a re-upgrade after a downgrade), its files, its migrations in the order they apply
/// and the default limits if the extension has none. The BLOBs must already be stored.
pub fn write_registry_rows(
    tx: &mut CrdtTransaction<'_>,
    bundle: &VerifiedBundle,
    manifest: &Manifest,
    now_ms: i64,
) -> Result<BundleIds> {
    let ids = BundleIds::of(bundle, manifest);
    let ext = ids.extension_id.to_string();
    let bid = ids.bundle_id.to_string();

    if exists(tx, "SELECT COUNT(*) FROM extensions WHERE id = ?1", &ext)? {
        tx.execute(
            "UPDATE extensions SET state = 'installed', updated_at = ?2 WHERE id = ?1",
            params![ext, now_ms],
        )?;
    } else {
        tx.execute(
            "INSERT INTO extensions (id, public_key, name, display_name, enabled, state, \
             purge_data, installed_at, updated_at) \
             VALUES (?1, ?2, ?3, ?4, 1, 'installed', 0, ?5, ?5)",
            params![
                ext,
                manifest.public_key.as_str(),
                manifest.name.as_str(),
                manifest.display_name,
                now_ms
            ],
        )?;
    }

    if exists(
        tx,
        "SELECT COUNT(*) FROM extension_bundles WHERE id = ?1",
        &bid,
    )? {
        tx.execute(
            "UPDATE extension_bundles SET retired = 0 WHERE id = ?1 AND retired <> 0",
            params![bid],
        )?;
    } else {
        tx.execute(
            "INSERT INTO extension_bundles (id, extension_id, version, manifest_json, \
             signature_json, retired, added_at) VALUES (?1, ?2, ?3, ?4, ?5, 0, ?6)",
            params![
                bid,
                ext,
                manifest.version.to_string(),
                bundle.manifest_bytes,
                bundle.signature_bytes,
                now_ms
            ],
        )?;
        for file in &bundle.files {
            tx.execute(
                "INSERT INTO extension_bundle_files (id, bundle_id, path, size, sha256) \
                 VALUES (?1, ?2, ?3, ?4, ?5)",
                params![
                    bundle_file_id(ids.bundle_id, &file.path).to_string(),
                    bid,
                    file.path,
                    file.size as i64,
                    file.sha256
                ],
            )?;
        }
    }
    // A file that refers to a BLOB again keeps it from being collected (a reinstall, R11).
    for file in &bundle.files {
        tx.execute(
            "UPDATE extension_blobs SET orphaned_at = NULL \
             WHERE hash = ?1 AND orphaned_at IS NOT NULL",
            params![file.sha256],
        )?;
    }

    for (position, migration) in bundle.migrations.iter().enumerate() {
        let sql_sha256 = crate::extensions::sql::migrate::sql_sha256(&migration.sql);
        let id = migration_id(ids.extension_id, &migration.name, &sql_sha256).to_string();
        if !exists(
            tx,
            "SELECT COUNT(*) FROM extension_migrations WHERE id = ?1",
            &id,
        )? {
            tx.execute(
                "INSERT INTO extension_migrations (id, extension_id, name, position, sql, \
                 sql_sha256) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                params![
                    id,
                    ext,
                    migration.name,
                    position as i64,
                    migration.sql,
                    sql_sha256
                ],
            )?;
        }
    }

    let limits = limits_id(ids.extension_id).to_string();
    if !exists(
        tx,
        "SELECT COUNT(*) FROM extension_limits WHERE id = ?1",
        &limits,
    )? {
        tx.execute(
            "INSERT INTO extension_limits (id, extension_id, max_rows, max_concurrent, \
             max_sql_bytes, timeout_ms, max_response_bytes) \
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            params![
                limits,
                ext,
                default_limits::MAX_ROWS as i64,
                default_limits::MAX_CONCURRENT as i64,
                default_limits::MAX_SQL_BYTES as i64,
                default_limits::TIMEOUT_MS as i64,
                default_limits::MAX_RESPONSE_BYTES as i64
            ],
        )?;
    }
    Ok(ids)
}

/// What reading a stored bundle file with a size limit found.
#[derive(Debug, PartialEq, Eq)]
pub enum VerifiedFile {
    /// The bundle has no such file.
    Missing,
    /// The stored BLOB is longer than the limit; it was not loaded.
    TooLarge,
    Data(Vec<u8>),
}

/// A file of a stored bundle, read by its expected hash and hashed again (R4). `None` when the
/// bundle has no such file; an error when its BLOB is missing (still transferring) or does not
/// match.
pub fn read_verified_file(
    q: &mut impl Query,
    bundle_id: Uuid,
    path: &str,
) -> Result<Option<Vec<u8>>> {
    Ok(
        match read_verified_file_within(q, bundle_id, path, usize::MAX)? {
            VerifiedFile::Data(data) => Some(data),
            VerifiedFile::Missing | VerifiedFile::TooLarge => None,
        },
    )
}

/// [`read_verified_file`] for a caller with a size limit: a BLOB longer than `max_bytes` is
/// refused by its stored length, before its bytes are loaded.
pub fn read_verified_file_within(
    q: &mut impl Query,
    bundle_id: Uuid,
    path: &str,
    max_bytes: usize,
) -> Result<VerifiedFile> {
    let expected = q.query_row(
        "SELECT sha256 FROM extension_bundle_files WHERE bundle_id = ?1 AND path = ?2",
        &[&bundle_id.to_string(), &path],
        |r| r.get::<_, String>(0),
    )?;
    let Some(expected) = expected else {
        return Ok(VerifiedFile::Missing);
    };
    let not_ready = || HolziError::ExtensionNotReady {
        status: "transferring".into(),
    };
    let length = q
        .query_row(
            "SELECT length(data) FROM extension_blobs WHERE hash = ?1",
            &[&expected],
            |r| r.get::<_, i64>(0),
        )?
        .ok_or_else(not_ready)?;
    if usize::try_from(length).map_or(true, |length| length > max_bytes) {
        return Ok(VerifiedFile::TooLarge);
    }
    let data = q
        .query_row(
            "SELECT data FROM extension_blobs WHERE hash = ?1",
            &[&expected],
            |r| r.get::<_, Vec<u8>>(0),
        )?
        .ok_or_else(not_ready)?;
    if sha256_hex(&data) != expected {
        return Err(HolziError::ExtensionNotReady {
            status: "signature_failed".into(),
        });
    }
    Ok(VerifiedFile::Data(data))
}

/// Why a stored bundle cannot start (FR-003).
#[derive(Debug, Clone)]
pub enum StoredBundleState {
    /// The bundle verifies with every file.
    Ready(Box<VerifiedBundle>),
    /// A BLOB has not arrived yet.
    Transferring,
    /// The stored bytes no longer verify.
    SignatureFailed(BundleRejection),
}

/// Checks a stored bundle again before it starts: its files and the stored `signature.json` are put
/// together and verified with every rule of the format (FR-003).
pub fn verify_stored_bundle(q: &mut impl Query, bundle_id: Uuid) -> Result<StoredBundleState> {
    let bid = bundle_id.to_string();
    let signature = q
        .query_row(
            "SELECT signature_json FROM extension_bundles WHERE id = ?1",
            &[&bid],
            |r| r.get::<_, Vec<u8>>(0),
        )?
        .ok_or(HolziError::ExtensionNotFound)?;
    let files = q.query_map(
        "SELECT f.path, b.data FROM extension_bundle_files f \
         LEFT JOIN extension_blobs b ON b.hash = f.sha256 \
         WHERE f.bundle_id = ?1 ORDER BY f.path",
        &[&bid],
        |r| Ok((r.get::<_, String>(0)?, r.get::<_, Option<Vec<u8>>>(1)?)),
    )?;
    let mut entries = Vec::with_capacity(files.len() + 1);
    for (path, data) in files {
        let Some(data) = data else {
            return Ok(StoredBundleState::Transferring);
        };
        entries.push(haex_bundle::Entry { path, data });
    }
    entries.push(haex_bundle::Entry {
        path: haex_bundle::format::SIGNATURE_PATH.to_owned(),
        data: signature,
    });
    Ok(match haex_bundle::verify_entries(entries) {
        Ok(bundle) => StoredBundleState::Ready(Box::new(bundle)),
        Err(error) => StoredBundleState::SignatureFailed(error.into()),
    })
}

#[cfg(test)]
#[path = "store_tests.rs"]
mod store_tests;
