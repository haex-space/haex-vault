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
//! ## How `object_key` survives `upsert_sync_state`
//!
//! `engine::state::upsert_sync_state` still uses `INSERT OR REPLACE` and
//! takes no `object_key` argument — every non-encrypting caller is oblivious
//! to it. Preservation is done by the statement itself: the `VALUES` list
//! carries a correlated subquery that reads the pre-conflict row's
//! `object_key` (`SELECT object_key FROM haex_sync_state_no_sync WHERE
//! rule_id = ?2 AND relative_path = ?3`). SQLite evaluates that subquery
//! *before* firing REPLACE's delete-then-insert, so the value survives the
//! row's rewrite. See `engine::state::upsert_sync_state` for the shipped
//! SQL and `engine::tests::upsert_sync_state_preserves_object_key_across_replace`
//! for the regression test.

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
/// content. Applies to all grant scopes (`own/*.m`, `space-<id>/*.m`).
pub const SIDECAR_SUFFIX: &str = ".m";

/// Bucket-root prefix for opaque content object keys. Round F2 flattens
/// content into a single grant-agnostic prefix so a shared file lives as
/// one physical object regardless of how many spaces it is granted into.
pub const CONTENT_KEY_PREFIX: &str = "content/o/";

/// Bucket-root prefix for own-vault sidecars — one grant-carrier per
/// content object the owner holds under their vault_key.
pub const OWN_SIDECAR_PREFIX: &str = "own/";

/// Bucket-root prefix stem for space-scoped sidecars. The full prefix
/// for a specific space is `space-<space_id>/`; see
/// [`space_sidecar_prefix`]. One grant-carrier per space × content
/// object, sealed under the space's MLS epoch key so only current
/// members can unwrap the enclosed DEK.
pub const SPACE_SIDECAR_PREFIX_STEM: &str = "space-";

/// Random bytes backing a fresh object key (128 bits).
const OBJECT_KEY_RANDOM_BYTES: usize = 16;

/// Mint a fresh opaque content object key: `content/o/` + 32 lowercase
/// hex chars.
///
/// Deliberately not base32 despite the plan's original sketch — hex needs no
/// new dependency (`hex` is already in `Cargo.toml`) and is equally opaque;
/// nothing depends on the specific encoding beyond "safe S3 key characters,
/// no structure leak".
pub fn generate_object_key() -> String {
    let mut bytes = [0u8; OBJECT_KEY_RANDOM_BYTES];
    rand::fill(&mut bytes);
    format!("{CONTENT_KEY_PREFIX}{}", hex::encode(bytes))
}

/// Legacy sidecar-key helper — appends `.m` to whatever object key it is
/// handed. Retained so the Round C bootstrap tests continue to compile
/// alongside the Round F2 own-vault path; new call sites should use
/// [`own_sidecar_key_for`] (or the space-scoped variant in a later round)
/// so the sidecar prefix carries the grant scope.
pub fn sidecar_key_for(object_key: &str) -> String {
    format!("{object_key}{SIDECAR_SUFFIX}")
}

/// Own-vault sidecar key for a canonical content object key.
/// `own_sidecar_key_for("content/o/<hex>") == "own/<hex>.m"`.
///
/// The sidecar filename shares its hex part with the content object so a
/// grant-holder (own device today, space member in a later round) can
/// reconstruct the content GET path from the sidecar filename alone —
/// no need to serialise the full content key inside every grant scope.
pub fn own_sidecar_key_for(content_key: &str) -> String {
    let hex = content_key
        .strip_prefix(CONTENT_KEY_PREFIX)
        .unwrap_or(content_key);
    format!("{OWN_SIDECAR_PREFIX}{hex}{SIDECAR_SUFFIX}")
}

/// Space-scoped sidecar key for a canonical content object key.
/// `space_sidecar_key_for("s1", "content/o/<hex>") == "space-s1/<hex>.m"`.
///
/// Same hex-shared-with-content-key convention as [`own_sidecar_key_for`]
/// — the grant scope lives in the path prefix, the physical content
/// object stays one file for the same DEK across every scope that
/// wraps it.
pub fn space_sidecar_key_for(space_id: &str, content_key: &str) -> String {
    let hex = content_key
        .strip_prefix(CONTENT_KEY_PREFIX)
        .unwrap_or(content_key);
    format!("{SPACE_SIDECAR_PREFIX_STEM}{space_id}/{hex}{SIDECAR_SUFFIX}")
}

/// Bucket-prefix stem the space-scoped provider bootstraps against —
/// `space-<space_id>/`. Kept as a helper rather than inline `format!`
/// so callers (bootstrap + tests) share one canonical spelling.
pub fn space_sidecar_prefix(space_id: &str) -> String {
    format!("{SPACE_SIDECAR_PREFIX_STEM}{space_id}/")
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
/// via its preserving `SELECT object_key` subquery in the VALUES list.
///
/// The `INSERT ... ON CONFLICT DO UPDATE` upsert is deliberate here over
/// `INSERT OR REPLACE`: REPLACE would DELETE the existing row (nulling
/// the just-preserved `hash` on a churn cycle) and re-insert, which is
/// exactly the shape of the pitfall that motivated the preserving
/// subquery in `upsert_sync_state`. UPSERT touches only the named columns.
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
/// module-level note on how `object_key` survives that call.
/// `pub(super)` for the same cross-sibling test-access reason as
/// [`object_key_known`].
///
/// UPSERT (`INSERT ... ON CONFLICT DO UPDATE`) instead of `INSERT OR
/// REPLACE`: REPLACE would DELETE the existing row and reinsert with a
/// fresh id, so any column not named in VALUES silently reverts to its
/// default — the same failure mode the preserving subquery in
/// `upsert_sync_state` guards against. UPSERT touches only the named
/// columns, preserving the row's stable id.
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
    let sql = "INSERT INTO haex_sync_state_no_sync \
        (id, rule_id, relative_path, file_size, modified_at, synced_at, deleted, hash, object_key) \
        VALUES (?1, ?2, ?3, ?4, ?5, ?6, 0, ?7, ?8) \
        ON CONFLICT (rule_id, relative_path) DO UPDATE SET \
            file_size = excluded.file_size, \
            modified_at = excluded.modified_at, \
            synced_at = excluded.synced_at, \
            deleted = 0, \
            hash = excluded.hash, \
            object_key = excluded.object_key"
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
