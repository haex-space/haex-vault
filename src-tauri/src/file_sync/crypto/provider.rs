//! Round D — encrypting `SyncProvider` decorator.
//!
//! Wraps an inner `SyncProvider` (in production always a
//! [`CloudProvider`](crate::file_sync::cloud_provider::CloudProvider)) and
//! presents the same trait to the sync engine while transparently:
//!
//! - Discovering the local `relative_path -> object_key` mapping on first
//!   `manifest()` for a rule (the plan's "Bootstrap einhängen") by
//!   listing the bucket via the inner provider and decrypting each
//!   sidecar not already cached in `haex_sync_state_no_sync`.
//! - Answering `manifest()` from that local cache with **plaintext**
//!   sizes and modification times (the Fallstrick 2 fix — reporting
//!   ciphertext sizes here would turn every unchanged file into a silent
//!   re-upload).
//! - Sealing a fresh file into an envelope + AEAD chunks and a sidecar
//!   record before uploading, using an opaque `o/<random-hex>` key so the
//!   storage operator learns neither the filename nor the directory
//!   layout.
//! - Reusing an existing object key on rewrite — a fresh mint per change
//!   would orphan the previous object under the same relative_path.
//! - Streaming through disk on the `read_file_to_path` /
//!   `write_file_from_path` paths so a multi-gigabyte plaintext or
//!   ciphertext never lives in RAM.
//!
//! The `CloudProvider` layer stays dumb: opaque object keys are just
//! relative paths from its perspective, and prefixing is its concern
//! alone. This module never touches `cloud_provider.rs` directly.
//!
//! ## Key source
//!
//! Two variants are wired:
//!
//! - [`FileKeySource::SpaceEpoch`] — shared-space content, sealed with a
//!   domain-separated derivative of the current MLS epoch's
//!   `haex_mls_sync_keys` row. Resolved via [`key_resolver::resolve_latest`]
//!   on write and [`key_resolver::resolve_key`] on read.
//! - [`FileKeySource::VaultKey`] — own-vault (personal) content, sealed
//!   with a per-vault key derived once from the default identity's
//!   Ed25519 seed and cached in [`AppState::vault_key`]
//!   (`Arc<Mutex<Option<Zeroizing<[u8; 32]>>>>`). The provider reads it
//!   just-in-time from the slot on each seal/open, so it holds no copy
//!   at rest. The key has no rotation concept — epoch is fixed to `0`
//!   on write and ignored on read.
//!
//! The two variants land in disjoint key spaces (their IKMs and HKDF
//! salts never overlap), so a mistyped rule that mixes them still fails
//! at AEAD-tag verification instead of silently opening under the wrong
//! key.
//!
//! [`AppState::vault_key`]: crate::AppState::vault_key

use std::path::Path;
use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use tokio::io::AsyncWriteExt;
use zeroize::Zeroizing;

use crate::database::DbConnection;

use super::super::engine::{load_sync_state, mark_deleted};
use super::super::hashing::ChunkedHash;
use super::super::provider::{
    validate_relative_path, ReadFileResult, SyncProvider, SyncProviderError,
};
use super::super::types::FileState;
use super::chunk::CHUNK_PLAINTEXT_SIZE;
use super::content::{open_bytes, open_stream, seal_bytes, seal_stream, StreamCryptoError};
use super::envelope::{EnvelopeHeader, NONCE_SIZE};
use super::key_resolver::{resolve_key, resolve_latest, KeyError};
use super::object_key::{
    generate_object_key, lookup_object_key, mark_object_deleted, object_key_known, set_object_key,
    sidecar_key_for, upsert_bootstrap_entry, ObjectKeyError, SIDECAR_SUFFIX,
};
use super::sidecar::{open_sidecar, seal_sidecar, SidecarError, SidecarPayload};

/// Which key material the decorator uses to seal file content.
#[derive(Debug, Clone)]
pub enum FileKeySource {
    /// Shared-space MLS epoch key — the current epoch is looked up per
    /// write via `key_resolver::resolve_latest`, and per read via
    /// `key_resolver::resolve_key` with the sealed envelope's epoch.
    SpaceEpoch {
        /// The space this rule syncs content for.
        space_id: String,
    },
    /// Own-vault (personal) key, derived from the default-identity
    /// Ed25519 seed via HKDF and cached in `AppState::vault_key`. The
    /// key is stable per vault (no rotation concept), so writes use a
    /// synthetic `epoch = 0` and reads ignore the envelope epoch.
    VaultKey,
}

/// Just-in-time handle to the own-vault key slot. Cloned from
/// `AppState::vault_key` at provider-construction time so the decorator
/// picks up any later population without needing to be rebuilt.
type VaultKeySlot = Arc<Mutex<Option<Zeroizing<[u8; 32]>>>>;

/// Errors surfaced by the encrypting provider. Wraps the underlying
/// provider/DB/crypto errors so the caller sees exactly which layer
/// failed — a network drop reads differently from a corrupt ciphertext.
#[derive(Debug, thiserror::Error)]
pub enum ProviderCryptoError {
    #[error(transparent)]
    Provider(#[from] SyncProviderError),
    #[error(transparent)]
    Key(#[from] KeyError),
    #[error(transparent)]
    Sidecar(#[from] SidecarError),
    #[error(transparent)]
    Crypto(#[from] super::chunk::CryptoError),
    #[error(transparent)]
    Stream(#[from] StreamCryptoError),
    #[error(transparent)]
    ObjectKey(#[from] ObjectKeyError),
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
    #[error("engine error: {0}")]
    Engine(String),
    #[error("no object_key cached for relative_path {path} — bootstrap missing?")]
    MissingObjectKey { path: String },
    #[error("vault not open — cannot access own-vault encryption key")]
    OwnVaultNotWired,
}

impl From<ProviderCryptoError> for SyncProviderError {
    fn from(err: ProviderCryptoError) -> Self {
        SyncProviderError::Other {
            reason: err.to_string(),
        }
    }
}

pub struct EncryptingSyncProvider {
    inner: Arc<dyn SyncProvider>,
    key_source: FileKeySource,
    rule_id: String,
    db: DbConnection,
    /// Handle to the own-vault key slot. Only consulted for the
    /// `VaultKey` variant — a shared-space rule can pass a slot that
    /// is never populated and it never blocks the seal path.
    vault_key_slot: VaultKeySlot,
}

/// Synthetic epoch stamped into `VaultKey`-sealed envelopes. The own-
/// vault key has no rotation concept, so the value carried on the wire
/// is a fixed sentinel — the open path ignores it and reads the key
/// straight from the slot.
const VAULT_KEY_EPOCH: u64 = 0;

impl EncryptingSyncProvider {
    /// Wrap an inner provider with envelope + sidecar encryption. The
    /// inner provider is expected to treat its `relative_path` argument as
    /// an opaque object key (`o/<hex>` or `o/<hex>.m`) — production
    /// `CloudProvider` does exactly that; other providers likely do not
    /// and are not supported.
    ///
    /// `vault_key_slot` is the `AppState::vault_key` handle (or a clone
    /// of it). Passing an unpopulated slot for a `SpaceEpoch` rule is
    /// fine — the slot is only consulted for `VaultKey`.
    pub fn new(
        inner: Arc<dyn SyncProvider>,
        key_source: FileKeySource,
        rule_id: impl Into<String>,
        db: DbConnection,
        vault_key_slot: VaultKeySlot,
    ) -> Self {
        Self {
            inner,
            key_source,
            rule_id: rule_id.into(),
            db,
            vault_key_slot,
        }
    }

    /// Read the own-vault key from the slot into a fresh `Zeroizing`
    /// buffer, then release the mutex. Callers get a scoped copy that
    /// scrubs on drop; the provider never keeps a copy at rest.
    fn load_vault_key(&self) -> Result<Zeroizing<[u8; 32]>, ProviderCryptoError> {
        let guard = self
            .vault_key_slot
            .lock()
            .map_err(|_| ProviderCryptoError::OwnVaultNotWired)?;
        let key_ref = guard
            .as_ref()
            .ok_or(ProviderCryptoError::OwnVaultNotWired)?;
        let mut copy = Zeroizing::new([0u8; 32]);
        copy.copy_from_slice(key_ref.as_ref());
        Ok(copy)
    }

    fn seal_key(&self) -> Result<(u64, Zeroizing<[u8; 32]>), ProviderCryptoError> {
        match &self.key_source {
            FileKeySource::SpaceEpoch { space_id } => {
                let (epoch, key) = resolve_latest(space_id.as_str(), &self.db)?;
                Ok((epoch, Zeroizing::new(key)))
            }
            FileKeySource::VaultKey => Ok((VAULT_KEY_EPOCH, self.load_vault_key()?)),
        }
    }

    fn open_key(&self, epoch: u64) -> Result<Zeroizing<[u8; 32]>, ProviderCryptoError> {
        match &self.key_source {
            FileKeySource::SpaceEpoch { space_id } => {
                let key = resolve_key(space_id.as_str(), epoch, &self.db)?;
                Ok(Zeroizing::new(key))
            }
            FileKeySource::VaultKey => {
                // Envelope epoch is informational for VaultKey — no
                // rotation, no lookup. Load the current slot value.
                self.load_vault_key()
            }
        }
    }

    fn random_nonce() -> [u8; NONCE_SIZE] {
        let mut n = [0u8; NONCE_SIZE];
        rand::fill(&mut n);
        n
    }

    /// Bootstrap the object-key cache from the remote bucket contents.
    ///
    /// Mirrors [`super::object_key::bootstrap_object_key_cache`] but
    /// consumes `SyncProvider` primitives instead of `StorageBackend` so
    /// the decorator does not need to hold a second handle to the same
    /// backend. Idempotent — already-cached object keys are skipped
    /// without a re-download.
    ///
    /// Per-object failures (a corrupt or undecryptable sidecar) never
    /// abort the run: each is logged via `eprintln!` (matching the rest
    /// of `file_sync`'s logging convention) and skipped so a single
    /// bad object cannot brick recovery of the rest of the library.
    /// Returns `Ok(())` on completion regardless of per-object
    /// outcomes; the outer `Result` covers only manifest-listing and
    /// DB-lookup failures that block the whole pass.
    async fn bootstrap(&self) -> Result<(), ProviderCryptoError> {
        let manifest = self.inner.manifest().await?;
        let mut content_keys: std::collections::HashSet<String> = std::collections::HashSet::new();
        let mut sidecar_owners: std::collections::HashSet<String> =
            std::collections::HashSet::new();
        for entry in &manifest {
            if entry.is_directory {
                continue;
            }
            match entry.relative_path.strip_suffix(SIDECAR_SUFFIX) {
                Some(owner) => {
                    sidecar_owners.insert(owner.to_string());
                }
                None => {
                    content_keys.insert(entry.relative_path.clone());
                }
            }
        }
        for object_key in content_keys.intersection(&sidecar_owners) {
            if object_key_known(&self.db, &self.rule_id, object_key)
                .map_err(ProviderCryptoError::Engine)?
            {
                continue;
            }
            let sidecar_key = sidecar_key_for(object_key);
            if let Err(e) = self.recover_and_store(&sidecar_key, object_key).await {
                // Per-object failure: log-and-continue is the plan's
                // "Fehlerisolation" policy. The decorator surface has no
                // notion of a report today; if we need one later, thread
                // it through here.
                eprintln!(
                    "[EncryptingSyncProvider] bootstrap: failed to recover sidecar {sidecar_key} (rule {}): {e}",
                    self.rule_id,
                );
                continue;
            }
        }
        Ok(())
    }

    async fn recover_and_store(
        &self,
        sidecar_key: &str,
        object_key: &str,
    ) -> Result<(), ProviderCryptoError> {
        let bytes = self.inner.read_file(sidecar_key).await?;
        let epoch = EnvelopeHeader::parse(&bytes)
            .map_err(ProviderCryptoError::Crypto)?
            .epoch;
        let key = self.open_key(epoch)?;
        let (_, payload) = open_sidecar(&key, &bytes)?;
        upsert_bootstrap_entry(
            &self.db,
            &self.rule_id,
            &payload.relative_path,
            object_key,
            payload.size,
            payload.modified_at,
            &payload.blake3,
        )
        .map_err(ProviderCryptoError::Engine)?;
        Ok(())
    }

    /// Resolve the object key for `relative_path`, minting a fresh one
    /// if none exists yet — but do **not** persist a fresh key here.
    /// The caller commits the mapping via `set_object_key` only after
    /// both content and sidecar uploads succeed.
    ///
    /// Deferring the write closes the phantom-manifest-entry hole: a
    /// row inserted here before the upload finishes would carry
    /// `deleted=0`, `object_key=<fresh>`, `size=0`, `modified_at=0`, and
    /// `manifest()`'s `!deleted && object_key.is_some()` filter would
    /// emit it as a real FileState. On the download-direction diff
    /// that turns into a scheduled fetch of an object the bucket never
    /// received. Returns `(object_key, is_fresh)` so the caller knows
    /// whether persistence is still owed after successful uploads.
    async fn resolve_or_mint_object_key(
        &self,
        relative_path: &str,
    ) -> Result<(String, bool), ProviderCryptoError> {
        if let Some(existing) = lookup_object_key(&self.db, &self.rule_id, relative_path)? {
            return Ok((existing, false));
        }
        Ok((generate_object_key(), true))
    }

    fn object_key_for_read(&self, relative_path: &str) -> Result<String, ProviderCryptoError> {
        lookup_object_key(&self.db, &self.rule_id, relative_path)?.ok_or_else(|| {
            ProviderCryptoError::MissingObjectKey {
                path: relative_path.to_string(),
            }
        })
    }

    async fn write_sidecar(
        &self,
        object_key: &str,
        payload: &SidecarPayload,
        seal_key: &[u8; 32],
        seal_epoch: u64,
    ) -> Result<(), ProviderCryptoError> {
        let bytes = seal_sidecar(seal_key, seal_epoch, Self::random_nonce(), payload)?;
        self.inner
            .write_file(&sidecar_key_for(object_key), &bytes)
            .await?;
        Ok(())
    }
}

#[async_trait]
impl SyncProvider for EncryptingSyncProvider {
    fn display_name(&self) -> String {
        format!("crypto:{}", self.inner.display_name())
    }

    /// Discover new objects, then return the plaintext view from the
    /// local cache. Both halves matter: the discovery step is what lets
    /// a peer add a file that this device then sees on the next cycle,
    /// and the cache-only projection is what keeps `manifest()` off the
    /// critical path of a `read_file`. Sizes and mtimes are the
    /// plaintext values recorded at write time or recovered by
    /// bootstrap — **not** ciphertext sizes, or the diff engine would
    /// re-upload every file every cycle.
    async fn manifest(&self) -> Result<Vec<FileState>, SyncProviderError> {
        self.bootstrap().await.map_err(SyncProviderError::from)?;
        let entries =
            load_sync_state(&self.db, &self.rule_id).map_err(|e| SyncProviderError::Other {
                reason: e.to_string(),
            })?;
        let files = entries
            .into_iter()
            .filter(|e| !e.deleted && e.object_key.is_some())
            .map(|e| FileState {
                relative_path: e.relative_path,
                size: e.file_size,
                modified_at: e.modified_at,
                is_directory: false,
                hash: e.hash,
                chunk_size: None,
                chunk_hashes: None,
            })
            .collect();
        Ok(files)
    }

    async fn read_file(&self, relative_path: &str) -> Result<Vec<u8>, SyncProviderError> {
        validate_relative_path(relative_path)?;
        let object_key = self
            .object_key_for_read(relative_path)
            .map_err(SyncProviderError::from)?;
        let ciphertext = self.inner.read_file(&object_key).await?;
        let epoch = EnvelopeHeader::parse(&ciphertext)
            .map_err(|e| SyncProviderError::Other {
                reason: e.to_string(),
            })?
            .epoch;
        let key = self.open_key(epoch).map_err(SyncProviderError::from)?;
        let (_, plaintext) =
            open_bytes(&key, &ciphertext).map_err(|e| SyncProviderError::Other {
                reason: e.to_string(),
            })?;
        Ok(plaintext)
    }

    async fn read_file_to_path(
        &self,
        relative_path: &str,
        output_path: &Path,
        _expected_chunks: Option<ChunkedHash>,
        on_progress: Arc<dyn Fn(u64, u64) + Send + Sync>,
    ) -> Result<ReadFileResult, SyncProviderError> {
        validate_relative_path(relative_path)?;
        let object_key = self
            .object_key_for_read(relative_path)
            .map_err(SyncProviderError::from)?;

        // Make sure `output_path`'s parent exists before staging beside
        // it — the tmp lands there so it shares a filesystem with the
        // real destination, avoiding tmpfs blow-ups when `/tmp` is
        // RAM-backed.
        if let Some(parent) = output_path.parent() {
            tokio::fs::create_dir_all(parent)
                .await
                .map_err(SyncProviderError::Io)?;
        }

        // Stage the ciphertext to a tempfile via the inner provider so
        // its streaming + resume machinery does the network work; then
        // stream-decrypt into `output_path` chunkwise. The ciphertext
        // tempfile is roughly file-size on disk (never in RAM), and the
        // per-chunk plaintext buffer inside `open_stream` is bounded to
        // one `CHUNK_PLAINTEXT_SIZE` block.
        let ct_tmp = staging_tempfile(output_path).map_err(SyncProviderError::Io)?;
        let ct_path = ct_tmp.path().to_path_buf();
        let ct_info = self
            .inner
            .read_file_to_path(&object_key, &ct_path, None, on_progress)
            .await?;

        let mut reader = tokio::io::BufReader::new(
            tokio::fs::File::open(&ct_path)
                .await
                .map_err(SyncProviderError::Io)?,
        );
        // Stage plaintext into a tempfile beside `output_path` and rename
        // it into place only after decryption + flush succeed. A mid-stream
        // AEAD tag failure or a truncated ciphertext must not leave a
        // partial plaintext at the final destination — the tempfile's
        // Drop deletes it on any early return.
        let pt_tmp = staging_tempfile(output_path).map_err(SyncProviderError::Io)?;
        let pt_path = pt_tmp.path().to_path_buf();
        {
            let mut writer = tokio::io::BufWriter::new(
                tokio::fs::File::create(&pt_path)
                    .await
                    .map_err(SyncProviderError::Io)?,
            );
            let epoch = self
                .peek_header_epoch(&ct_path)
                .await
                .map_err(SyncProviderError::from)?;
            let key = self.open_key(epoch).map_err(SyncProviderError::from)?;
            open_stream(&key, ct_info.bytes, &mut reader, &mut writer)
                .await
                .map_err(|e| SyncProviderError::Other {
                    reason: e.to_string(),
                })?;
            writer.flush().await.map_err(SyncProviderError::Io)?;
        }
        // Atomic swap: on the same filesystem this is a rename(2). Only
        // reached when decryption + flush succeeded, so the destination
        // never observes partial plaintext.
        pt_tmp.persist(output_path).map_err(|e| {
            SyncProviderError::Io(std::io::Error::other(format!(
                "persist decrypted tempfile: {e}"
            )))
        })?;

        // Plaintext size is not returned by open_stream directly; the
        // caller cares about the byte count for progress accounting.
        // Read output size once — cheap stat, no rehash.
        let plaintext_size = tokio::fs::metadata(output_path)
            .await
            .map_err(SyncProviderError::Io)?
            .len();
        Ok(ReadFileResult {
            bytes: plaintext_size,
            // Hash left unset — receiver-side integrity of the
            // ciphertext is the backend's responsibility (TLS + envelope
            // AEAD tag) and the manifest carries no plaintext hash to
            // compare against on this path.
            hash: None,
        })
    }

    async fn write_file(&self, relative_path: &str, data: &[u8]) -> Result<(), SyncProviderError> {
        validate_relative_path(relative_path)?;
        let (epoch, key) = self.seal_key().map_err(SyncProviderError::from)?;
        let (object_key, is_fresh) = self
            .resolve_or_mint_object_key(relative_path)
            .await
            .map_err(SyncProviderError::from)?;

        let ct = seal_bytes(&key, epoch, Self::random_nonce(), data).map_err(|e| {
            SyncProviderError::Other {
                reason: e.to_string(),
            }
        })?;
        self.inner.write_file(&object_key, &ct).await?;

        let payload = SidecarPayload {
            // Step 4 (provider refactor) will populate these with the
            // per-object DEK wrap payload. Round C/D providers seal the
            // content directly under the epoch key, so no wrapped_dek
            // exists on this write path yet.
            content_key: String::new(),
            wrapped_dek: Vec::new(),
            relative_path: relative_path.to_string(),
            size: data.len() as u64,
            modified_at: unix_now(),
            content_type: None,
            blake3: blake3::hash(data).to_hex().to_string(),
        };
        self.write_sidecar(&object_key, &payload, &key, epoch)
            .await
            .map_err(SyncProviderError::from)?;

        // Both uploads succeeded — safe to publish the mapping now.
        // See `resolve_or_mint_object_key` for the deferred-persistence
        // rationale (phantom manifest entries on failed uploads).
        if is_fresh {
            set_object_key(&self.db, &self.rule_id, relative_path, &object_key).map_err(|e| {
                SyncProviderError::Other {
                    reason: e.to_string(),
                }
            })?;
        }
        Ok(())
    }

    async fn write_file_from_path(
        &self,
        relative_path: &str,
        source_path: &Path,
    ) -> Result<(), SyncProviderError> {
        validate_relative_path(relative_path)?;
        let (epoch, key) = self.seal_key().map_err(SyncProviderError::from)?;
        let (object_key, is_fresh) = self
            .resolve_or_mint_object_key(relative_path)
            .await
            .map_err(SyncProviderError::from)?;

        let meta = tokio::fs::metadata(source_path)
            .await
            .map_err(SyncProviderError::Io)?;
        let plaintext_len = meta.len();

        // Stage the ciphertext beside the source file so the tmp file
        // lands on the same filesystem — `/tmp` is `tmpfs` on many Linux
        // hosts, and the module doc promises multi-gigabyte ciphertext
        // never lives in RAM.
        let ct_tmp = staging_tempfile(source_path).map_err(SyncProviderError::Io)?;
        {
            let mut src = tokio::io::BufReader::new(
                tokio::fs::File::open(source_path)
                    .await
                    .map_err(SyncProviderError::Io)?,
            );
            let mut dst = tokio::io::BufWriter::new(
                tokio::fs::File::create(ct_tmp.path())
                    .await
                    .map_err(SyncProviderError::Io)?,
            );
            seal_stream(
                &key,
                epoch,
                Self::random_nonce(),
                plaintext_len,
                &mut src,
                &mut dst,
            )
            .await
            .map_err(|e| SyncProviderError::Other {
                reason: e.to_string(),
            })?;
            dst.flush().await.map_err(SyncProviderError::Io)?;
        }
        self.inner
            .write_file_from_path(&object_key, ct_tmp.path())
            .await?;

        let hash = hash_file_blake3(source_path)
            .await
            .map_err(SyncProviderError::Io)?;
        let payload = SidecarPayload {
            // Step 4 (provider refactor) will populate these with the
            // per-object DEK wrap payload; see write_file above.
            content_key: String::new(),
            wrapped_dek: Vec::new(),
            relative_path: relative_path.to_string(),
            size: plaintext_len,
            modified_at: meta_mtime_secs(&meta),
            content_type: None,
            blake3: hash,
        };
        self.write_sidecar(&object_key, &payload, &key, epoch)
            .await
            .map_err(SyncProviderError::from)?;

        // Both uploads succeeded — safe to publish the mapping now.
        if is_fresh {
            set_object_key(&self.db, &self.rule_id, relative_path, &object_key).map_err(|e| {
                SyncProviderError::Other {
                    reason: e.to_string(),
                }
            })?;
        }
        Ok(())
    }

    async fn delete_file(
        &self,
        relative_path: &str,
        _to_trash: bool,
    ) -> Result<(), SyncProviderError> {
        validate_relative_path(relative_path)?;
        // Two objects, one logical file — delete both. Order: sidecar
        // first, then content. If a crash lands between the two, the
        // orphan-content pass on the next bootstrap surfaces the leftover
        // (Round C behaviour); orphan sidecars would just be logged and
        // ignored, so the surviving-sidecar failure mode is the noisier
        // one. Delete-in-place ignores `to_trash` — S3 has no trash, and
        // `CloudProvider::supports_trash()` returns false.
        if let Some(object_key) = lookup_object_key(&self.db, &self.rule_id, relative_path)
            .map_err(|e| SyncProviderError::Other {
                reason: e.to_string(),
            })?
        {
            let sidecar = sidecar_key_for(&object_key);
            // Ignore "not found" on the sidecar side — an already-missing
            // sidecar means a previous half-crashed delete, not a bug.
            match self.inner.delete_file(&sidecar, false).await {
                Ok(()) | Err(SyncProviderError::NotFound { .. }) => {}
                Err(e) => return Err(e),
            }
            match self.inner.delete_file(&object_key, false).await {
                Ok(()) | Err(SyncProviderError::NotFound { .. }) => {}
                Err(e) => return Err(e),
            }
        }
        mark_object_deleted(&self.db, &self.rule_id, relative_path).map_err(|e| {
            SyncProviderError::Other {
                reason: e.to_string(),
            }
        })?;
        // Also mark deleted via engine::state to keep the two column-
        // sets aligned (`mark_object_deleted` writes the same columns
        // that `mark_deleted` does, but callers reading via
        // engine::state may still touch this row; both are idempotent).
        let _ = mark_deleted(&self.db, &self.rule_id, relative_path);
        Ok(())
    }

    async fn create_directory(&self, _relative_path: &str) -> Result<(), SyncProviderError> {
        // Cloud object stores have no directory concept — the inner
        // CloudProvider returns Ok(()) here anyway, and the sync engine
        // is expected to skip mkdir actions for us via
        // `supports_directories()`.
        Ok(())
    }

    fn supports_streaming(&self) -> bool {
        // Chunkwise via tempfile — see `read_file_to_path` /
        // `write_file_from_path`. Only the ciphertext is staged on disk
        // and it is stream-decrypted into the destination one 1 MiB
        // chunk at a time.
        true
    }

    fn supports_trash(&self) -> bool {
        false
    }

    fn supports_directories(&self) -> bool {
        false
    }

    async fn prime_hash_after_write(&self, _file: &FileState) {
        // Hash is the plaintext BLAKE3, computed at write time from the
        // source bytes — nothing to prime for a receiver-side seed here.
    }
}

impl EncryptingSyncProvider {
    /// Read just the 37-byte header of a ciphertext file to learn its
    /// epoch — cheap enough to do twice (here plus inside `open_stream`)
    /// and lets us resolve the AEAD key before the streaming pass starts.
    async fn peek_header_epoch(&self, path: &Path) -> Result<u64, ProviderCryptoError> {
        use tokio::io::AsyncReadExt as _;
        let mut f = tokio::fs::File::open(path).await?;
        let mut hdr = [0u8; super::envelope::HEADER_SIZE];
        f.read_exact(&mut hdr).await?;
        Ok(EnvelopeHeader::parse(&hdr)
            .map_err(ProviderCryptoError::Crypto)?
            .epoch)
    }
}

/// Create a `NamedTempFile` beside `target` so it shares a filesystem
/// with the intended destination. `/tmp` on many Linux distributions
/// is `tmpfs` (RAM-backed); the module doc promises multi-gigabyte
/// ciphertext never lives in RAM, so staging a large file there would
/// break that guarantee. Falls back to the OS temp dir only if
/// `target` has no parent — an unrooted target implies a small write
/// (say, in tests) where the fallback is harmless.
fn staging_tempfile(target: &Path) -> std::io::Result<tempfile::NamedTempFile> {
    match target.parent() {
        Some(dir) if !dir.as_os_str().is_empty() => tempfile::NamedTempFile::new_in(dir),
        _ => tempfile::NamedTempFile::new(),
    }
}

fn unix_now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

fn meta_mtime_secs(meta: &std::fs::Metadata) -> u64 {
    meta.modified()
        .ok()
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|d| d.as_secs())
        .unwrap_or_else(unix_now)
}

async fn hash_file_blake3(path: &Path) -> Result<String, std::io::Error> {
    use tokio::io::AsyncReadExt as _;
    let mut hasher = blake3::Hasher::new();
    let mut f = tokio::fs::File::open(path).await?;
    let mut buf = vec![0u8; CHUNK_PLAINTEXT_SIZE];
    loop {
        let n = f.read(&mut buf).await?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
    }
    Ok(hasher.finalize().to_hex().to_string())
}
