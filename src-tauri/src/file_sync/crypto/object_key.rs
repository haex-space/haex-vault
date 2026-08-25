//! Round C — opaque object-key generation and the local object-key cache.
//!
//! Cloud object keys are random, not derived from `relative_path` — a
//! path-derived key would leak the exact thing Phase 4 exists to hide (see
//! the plan's "Zielbild": "sonst lernt der Betreiber Dateinamen und
//! Ordnerstruktur"). That means every device needs a local
//! `relative_path -> object_key` mapping, cached in
//! `haex_sync_state_no_sync.object_key` (added by migration
//! `0019_file_sync_object_key`). [`bootstrap_object_key_cache`] rebuilds
//! that mapping on a fresh device by listing the bucket and decrypting each
//! object's metadata sidecar.
//!
//! ## Cache table ownership
//!
//! This module reads/writes `haex_sync_state_no_sync` directly via
//! `database::core::{select, execute}` — the same non-CRDT pattern
//! `file_sync::engine::state` already uses for that table (the `_no_sync`
//! suffix means it is local-only, never replicated). It does not call into
//! `engine::state` to avoid an `engine -> crypto -> engine` dependency
//! cycle; `engine::state::upsert_sync_state` (the regular write-path
//! upsert) and [`upsert_bootstrap_entry`] here are companions, not
//! duplicates — see the pitfall note on [`bootstrap_object_key_cache`].
//!
//! ## Round D pitfall: `upsert_sync_state` does not carry `object_key`
//!
//! `engine::state::upsert_sync_state` does `INSERT OR REPLACE` over the full
//! row without an `object_key` column in its statement. On SQLite that
//! means a regular sync-execute upsert *after* bootstrap has populated
//! `object_key` for a row will silently null it out again — REPLACE deletes
//! and reinserts the whole row, so unlisted columns revert to their
//! default. Round D's provider wiring must either extend
//! `upsert_sync_state` to accept and persist `object_key`, or upsert through
//! this module's helpers instead. Not fixed here because it requires
//! touching `engine::execute.rs` call sites, which is explicitly out of
//! scope for Round C.

use std::collections::HashSet;
use std::time::{SystemTime, UNIX_EPOCH};

use serde_json::Value as JsonValue;

use crate::database::DbConnection;
use crate::remote_storage::backend::StorageBackend;
use crate::remote_storage::error::StorageError;

use super::envelope::EnvelopeHeader;
use super::key_resolver::{resolve_key, KeyError};
use super::sidecar::{open_sidecar, SidecarError, SidecarPayload};

/// Suffix marking a bucket object as a metadata sidecar rather than file
/// content. `sidecar_key_for("o/…") == "o/….m"`.
pub const SIDECAR_SUFFIX: &str = ".m";

/// Bucket-root prefix for opaque content object keys. Keeps bucket roots
/// flat and visually distinct from any future non-file_sync object under
/// the same prefix.
const OBJECT_KEY_PREFIX: &str = "o/";

/// Random bytes backing a fresh object key (128 bits).
const OBJECT_KEY_RANDOM_BYTES: usize = 16;

/// Mint a fresh opaque object key: `o/` + 32 lowercase hex chars.
///
/// Deliberately not base32 despite the plan's original sketch — hex needs no
/// new dependency (`hex` is already in `Cargo.toml`) and is equally opaque;
/// nothing depends on the specific encoding beyond "safe S3 key characters,
/// no structure leak".
pub fn generate_object_key() -> String {
    let mut bytes = [0u8; OBJECT_KEY_RANDOM_BYTES];
    rand::fill(&mut bytes);
    format!("{OBJECT_KEY_PREFIX}{}", hex::encode(bytes))
}

/// The sidecar object key for a given content object key.
pub fn sidecar_key_for(object_key: &str) -> String {
    format!("{object_key}{SIDECAR_SUFFIX}")
}

/// Errors from the object-key cache and bootstrap path.
#[derive(Debug, thiserror::Error)]
pub enum ObjectKeyError {
    #[error("storage error: {0}")]
    Storage(#[from] StorageError),
    #[error(transparent)]
    Crypto(#[from] super::chunk::CryptoError),
    #[error(transparent)]
    Sidecar(#[from] SidecarError),
    #[error(transparent)]
    Key(#[from] KeyError),
    #[error("database error: {0}")]
    Database(String),
}

/// Outcome of a bootstrap run. Per-object failures (a corrupt or
/// undecryptable sidecar) never abort the run — they land in
/// `failed_sidecars` — so one bad object cannot brick recovery of the rest
/// of the library. See the plan's "Orphans führen zu einer definierten
/// Aktion statt zu einem Abbruch".
#[derive(Debug, Clone, Default)]
pub struct BootstrapReport {
    /// Sidecars decrypted and newly written into the local cache.
    pub recovered: usize,
    /// Object keys already present in the cache — skipped without a
    /// download, per the plan's "für jeden Key ohne Cache-Eintrag".
    pub already_known: usize,
    /// Content objects with no matching `<key>.m` sidecar. Defined action
    /// (plan default): removed at the next sync cycle. That deletion is
    /// Round D's job (it needs a running provider/sync cycle) — this
    /// module only detects and reports.
    pub orphan_content: Vec<String>,
    /// Sidecar objects with no matching content object. Defined action:
    /// ignored — logged here, never acted on further.
    pub orphan_sidecar: Vec<String>,
    /// Sidecar key + error message, for paired objects whose sidecar could
    /// not be recovered (decrypt failure, unknown epoch, malformed JSON).
    pub failed_sidecars: Vec<(String, String)>,
}

/// Rebuild the local `relative_path -> object_key` mapping for `rule_id` by
/// listing `prefix` on `backend` and decrypting every sidecar not already
/// cached.
///
/// `space_id` resolves the AEAD key per sidecar's own envelope epoch (see
/// `key_resolver::resolve_key`) — the caller (Round D wiring) is
/// responsible for supplying the space a given cloud sync rule's content is
/// sealed under; this function does not infer it.
pub async fn bootstrap_object_key_cache(
    backend: &dyn StorageBackend,
    prefix: &str,
    space_id: &str,
    rule_id: &str,
    db: &DbConnection,
) -> Result<BootstrapReport, ObjectKeyError> {
    let objects = backend.list(Some(prefix)).await?;

    let mut content_keys: HashSet<String> = HashSet::new();
    let mut sidecar_owners: HashSet<String> = HashSet::new();
    for obj in &objects {
        match obj.key.strip_suffix(SIDECAR_SUFFIX) {
            Some(owner) => {
                sidecar_owners.insert(owner.to_string());
            }
            None => {
                content_keys.insert(obj.key.clone());
            }
        }
    }

    let mut report = BootstrapReport {
        orphan_content: content_keys
            .iter()
            .filter(|k| !sidecar_owners.contains(*k))
            .cloned()
            .collect(),
        orphan_sidecar: sidecar_owners
            .iter()
            .filter(|k| !content_keys.contains(*k))
            .map(|k| sidecar_key_for(k))
            .collect(),
        ..Default::default()
    };

    let paired = content_keys.intersection(&sidecar_owners);
    for object_key in paired {
        if object_key_known(db, rule_id, object_key).map_err(ObjectKeyError::Database)? {
            report.already_known += 1;
            continue;
        }
        let sidecar_key = sidecar_key_for(object_key);
        match recover_sidecar(backend, &sidecar_key, space_id, db).await {
            Ok(payload) => match upsert_bootstrap_entry(
                db,
                rule_id,
                &payload.relative_path,
                object_key,
                payload.size,
                payload.modified_at,
                &payload.blake3,
            ) {
                Ok(()) => report.recovered += 1,
                Err(e) => report.failed_sidecars.push((sidecar_key, e)),
            },
            Err(e) => report.failed_sidecars.push((sidecar_key, e.to_string())),
        }
    }

    Ok(report)
}

async fn recover_sidecar(
    backend: &dyn StorageBackend,
    sidecar_key: &str,
    space_id: &str,
    db: &DbConnection,
) -> Result<SidecarPayload, ObjectKeyError> {
    let bytes = backend.download(sidecar_key).await?;
    // Cheap pre-parse to learn the epoch before resolving the key;
    // `open_sidecar` re-parses the (37-byte) header internally rather than
    // taking it as a parameter, keeping its own API self-contained.
    let epoch = EnvelopeHeader::parse(&bytes)?.epoch;
    let key = resolve_key(space_id, epoch, db)?;
    let (_, payload) = open_sidecar(&key, &bytes)?;
    Ok(payload)
}

/// Look up the opaque object key a `(rule_id, relative_path)` pair is
/// synced under. Returns `None` if the row does not exist (fresh file) or
/// carries a NULL `object_key` (pre-Round-C row). The provider decorator
/// calls this to decide whether to reuse an existing key or mint a fresh
/// one on write — reusing keeps the storage-side history append-only, so
/// a random re-mint on every write would orphan the previous object.
pub fn lookup_object_key(
    db: &DbConnection,
    rule_id: &str,
    relative_path: &str,
) -> Result<Option<String>, ObjectKeyError> {
    let rows = crate::database::core::select(
        "SELECT object_key FROM haex_sync_state_no_sync \
         WHERE rule_id = ?1 AND relative_path = ?2 LIMIT 1"
            .to_string(),
        vec![
            JsonValue::String(rule_id.to_string()),
            JsonValue::String(relative_path.to_string()),
        ],
        db,
    )
    .map_err(|e| ObjectKeyError::Database(e.to_string()))?;
    Ok(rows
        .into_iter()
        .next()
        .and_then(|row| row.into_iter().next())
        .and_then(|v| v.as_str().map(str::to_string)))
}

/// Persist a freshly minted object key for `(rule_id, relative_path)`
/// without disturbing `file_size`, `modified_at`, `synced_at`, `deleted`,
/// or `hash` on an existing row. Called by the provider decorator right
/// after [`generate_object_key`], before the content upload — the
/// subsequent [`crate::file_sync::engine::state::upsert_sync_state`] call
/// then finds the row already carrying the object key and preserves it
/// via its COALESCE subquery.
///
/// The `INSERT ... ON CONFLICT DO UPDATE` upsert is deliberate here over
/// `INSERT OR REPLACE`: REPLACE would DELETE the existing row (nulling
/// the just-preserved `hash` on a churn cycle) and re-insert, which is
/// exactly the shape of the Round C pitfall that motivated the COALESCE
/// fix in `upsert_sync_state`. UPSERT touches only the named columns.
pub fn set_object_key(
    db: &DbConnection,
    rule_id: &str,
    relative_path: &str,
    object_key: &str,
) -> Result<(), ObjectKeyError> {
    let id = uuid::Uuid::new_v4().to_string();
    let now = unix_now().to_string();
    let sql = "INSERT INTO haex_sync_state_no_sync \
        (id, rule_id, relative_path, file_size, modified_at, synced_at, deleted, object_key) \
        VALUES (?1, ?2, ?3, 0, 0, ?4, 0, ?5) \
        ON CONFLICT (rule_id, relative_path) \
        DO UPDATE SET object_key = excluded.object_key"
        .to_string();
    let params = vec![
        JsonValue::String(id),
        JsonValue::String(rule_id.to_string()),
        JsonValue::String(relative_path.to_string()),
        JsonValue::String(now),
        JsonValue::String(object_key.to_string()),
    ];
    crate::database::core::execute(sql, params, db)
        .map_err(|e| ObjectKeyError::Database(e.to_string()))?;
    Ok(())
}

/// Mark the row for `(rule_id, relative_path)` deleted. Companion to
/// [`crate::file_sync::engine::state::mark_deleted`] but exposed here so
/// the provider decorator can call it without depending on
/// `engine::state`, keeping the crypto module free of an
/// `engine -> crypto -> engine` cycle. Semantically identical to
/// `mark_deleted`.
pub fn mark_object_deleted(
    db: &DbConnection,
    rule_id: &str,
    relative_path: &str,
) -> Result<(), ObjectKeyError> {
    let now = unix_now().to_string();
    let sql = "UPDATE haex_sync_state_no_sync SET deleted = 1, synced_at = ?1 \
        WHERE rule_id = ?2 AND relative_path = ?3"
        .to_string();
    let params = vec![
        JsonValue::String(now),
        JsonValue::String(rule_id.to_string()),
        JsonValue::String(relative_path.to_string()),
    ];
    crate::database::core::execute(sql, params, db)
        .map_err(|e| ObjectKeyError::Database(e.to_string()))?;
    Ok(())
}

/// `pub(super)` (not private) so `crypto::tests::object_key` — a sibling of
/// this module, not a descendant — can exercise the cache lookup directly in
/// tests, matching `key_resolver::derive_file_key`'s visibility rationale.
pub(super) fn object_key_known(
    db: &DbConnection,
    rule_id: &str,
    object_key: &str,
) -> Result<bool, String> {
    let rows = crate::database::core::select(
        "SELECT 1 FROM haex_sync_state_no_sync WHERE rule_id = ?1 AND object_key = ?2 LIMIT 1"
            .to_string(),
        vec![
            JsonValue::String(rule_id.to_string()),
            JsonValue::String(object_key.to_string()),
        ],
        db,
    )
    .map_err(|e| e.to_string())?;
    Ok(!rows.is_empty())
}

/// Insert or update the sync-state row recovered from a sidecar during
/// bootstrap. Companion to `engine::state::upsert_sync_state` — see the
/// module-level Round D pitfall note. `pub(super)` for the same
/// cross-sibling test-access reason as [`object_key_known`].
pub(super) fn upsert_bootstrap_entry(
    db: &DbConnection,
    rule_id: &str,
    relative_path: &str,
    object_key: &str,
    file_size: u64,
    modified_at: u64,
    blake3_hash: &str,
) -> Result<(), String> {
    let now = unix_now().to_string();
    let id = uuid::Uuid::new_v4().to_string();
    let sql = "INSERT OR REPLACE INTO haex_sync_state_no_sync \
        (id, rule_id, relative_path, file_size, modified_at, synced_at, deleted, hash, object_key) \
        VALUES (?1, ?2, ?3, ?4, ?5, ?6, 0, ?7, ?8)"
        .to_string();
    let params = vec![
        JsonValue::String(id),
        JsonValue::String(rule_id.to_string()),
        JsonValue::String(relative_path.to_string()),
        JsonValue::Number(serde_json::Number::from(file_size)),
        JsonValue::Number(serde_json::Number::from(modified_at)),
        JsonValue::String(now),
        JsonValue::String(blake3_hash.to_string()),
        JsonValue::String(object_key.to_string()),
    ];
    crate::database::core::execute(sql, params, db).map_err(|e| e.to_string())?;
    Ok(())
}

fn unix_now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}
