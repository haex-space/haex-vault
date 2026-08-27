//! Round F2 — encrypting `SyncProvider` decorator (own-vault path).
//!
//! Wraps an inner `SyncProvider` (in production always a
//! [`CloudProvider`](crate::file_sync::cloud_provider::CloudProvider)) and
//! presents the same trait to the sync engine while transparently:
//!
//! - Discovering the local `relative_path -> object_key` mapping on first
//!   `manifest()` for a rule by listing `own/*.m` sidecars in the bucket
//!   and decrypting each one not already cached in
//!   `haex_sync_state_no_sync`.
//! - Answering `manifest()` from that local cache with **plaintext**
//!   sizes and modification times — reporting ciphertext sizes here would
//!   turn every unchanged file into a silent re-upload.
//! - Sealing a fresh file into an envelope + AEAD chunks under a
//!   per-object DEK, sealing the sidecar under the vault key, and
//!   uploading both under opaque keys so the storage operator learns
//!   neither filename nor directory layout.
//! - Reusing an existing content object key on rewrite — a fresh mint per
//!   change would orphan the previous object under the same relative_path.
//! - Streaming through disk on the `read_file_to_path` /
//!   `write_file_from_path` paths so a multi-gigabyte plaintext or
//!   ciphertext never lives in RAM.
//!
//! The `CloudProvider` layer stays dumb: opaque keys are just relative
//! paths from its perspective, and prefixing is its concern alone. This
//! module never touches `cloud_provider.rs` directly.
//!
//! ## Key model — uniform DEK/KEK split
//!
//! Every object gets a fresh per-write **DEK** (Data Encryption Key)
//! that seals the file content bytes. The DEK is then wrapped under the
//! grant's **KEK** (Key Encryption Key) and carried inside the sidecar.
//! For the own-vault path the KEK is the `vault_key` slot value —
//! derived once from the default identity's Ed25519 seed and cached in
//! [`AppState::vault_key`] (`Arc<Mutex<Option<Zeroizing<[u8; 32]>>>>`).
//! The provider reads it just-in-time from the slot on each seal/open,
//! so it holds no copy at rest.
//!
//! Round F3 adds a space-scoped sibling that wraps the same DEK under
//! an MLS-derived KEK per grant — the content object is shared
//! bit-for-bit, only the sidecars differ. The DEK/KEK boundary in this
//! module is what makes that layering possible without duplicating file
//! bytes.
//!
//! ## Envelope epochs on own-vault objects
//!
//! Own-vault content and sidecars carry a synthetic envelope `epoch`
//! of `0` on the wire — the vault key has no rotation concept. Readers
//! ignore the value and load the key straight from the slot.
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
use super::dek_wrap::{unwrap_dek, wrap_dek, DekWrapError, DEK_LEN};
use super::envelope::NONCE_SIZE;
use super::object_key::{
    generate_object_key, lookup_object_key, mark_object_deleted, object_key_known,
    own_sidecar_key_for, set_object_key, upsert_bootstrap_entry, ObjectKeyError,
    OWN_SIDECAR_PREFIX, SIDECAR_SUFFIX,
};
use super::sidecar::{open_sidecar, seal_sidecar, SidecarError, SidecarPayload};

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
    Sidecar(#[from] SidecarError),
    #[error(transparent)]
    Crypto(#[from] super::chunk::CryptoError),
    #[error(transparent)]
    Stream(#[from] StreamCryptoError),
    #[error(transparent)]
    ObjectKey(#[from] ObjectKeyError),
    #[error(transparent)]
    DekWrap(#[from] DekWrapError),
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
    rule_id: String,
    db: DbConnection,
    /// Handle to the own-vault key slot. Read just-in-time on each seal /
    /// open — the provider never keeps a copy at rest.
    vault_key_slot: VaultKeySlot,
}

/// Synthetic epoch stamped into own-vault envelopes. The vault key has
/// no rotation concept, so the value carried on the wire is a fixed
/// sentinel; readers ignore it and load the key from the slot.
const VAULT_KEY_EPOCH: u64 = 0;

impl EncryptingSyncProvider {
    /// Wrap an inner provider with envelope + sidecar encryption for the
    /// own-vault path. The inner provider is expected to treat its
    /// `relative_path` argument as an opaque key (`content/o/<hex>` or
    /// `own/<hex>.m`) — production `CloudProvider` does exactly that;
    /// other providers likely do not and are not supported.
    ///
    /// `vault_key_slot` is the `AppState::vault_key` handle (or a clone
    /// of it). If the slot is empty when the first seal or open happens
    /// the decorator surfaces `OwnVaultNotWired` — the operator sees a
    /// clear error, not silent corruption.
    pub fn new(
        inner: Arc<dyn SyncProvider>,
        rule_id: impl Into<String>,
        db: DbConnection,
        vault_key_slot: VaultKeySlot,
    ) -> Self {
        Self {
            inner,
            rule_id: rule_id.into(),
            db,
            vault_key_slot,
        }
    }

    /// Read the vault key from the slot into a fresh `Zeroizing` buffer,
    /// then release the mutex. Callers get a scoped copy that scrubs on
    /// drop; the provider never keeps a copy at rest.
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

    fn random_nonce() -> [u8; NONCE_SIZE] {
        let mut n = [0u8; NONCE_SIZE];
        rand::fill(&mut n);
        n
    }

    fn random_dek() -> Zeroizing<[u8; DEK_LEN]> {
        let mut dek = Zeroizing::new([0u8; DEK_LEN]);
        rand::fill(dek.as_mut());
        dek
    }

    /// Bootstrap the object-key cache from the remote bucket contents.
    ///
    /// Scans `own/*.m` sidecars via the inner provider's manifest and
    /// upserts one row per newly discovered content object. Idempotent —
    /// already-cached content keys are skipped without a re-download.
    ///
    /// Per-object failures (a corrupt or undecryptable sidecar) never
    /// abort the run: each is logged via `eprintln!` and skipped so a
    /// single bad sidecar cannot brick recovery of the rest of the
    /// library. Returns `Ok(())` on completion regardless of per-object
    /// outcomes; the outer `Result` covers only manifest-listing failures
    /// that block the whole pass.
    async fn bootstrap(&self) -> Result<(), ProviderCryptoError> {
        let manifest = self.inner.manifest().await?;
        for entry in &manifest {
            if entry.is_directory {
                continue;
            }
            let Some(rest) = entry.relative_path.strip_prefix(OWN_SIDECAR_PREFIX) else {
                continue;
            };
            if !rest.ends_with(SIDECAR_SUFFIX) {
                continue;
            }
            if let Err(e) = self.recover_and_store(&entry.relative_path).await {
                // Per-object failure: log-and-continue is the plan's
                // "Fehlerisolation" policy. The decorator surface has no
                // notion of a report today; if we need one later, thread
                // it through here.
                eprintln!(
                    "[EncryptingSyncProvider] bootstrap: failed to recover sidecar {} (rule {}): {e}",
                    entry.relative_path,
                    self.rule_id,
                );
                continue;
            }
        }
        Ok(())
    }

    async fn recover_and_store(&self, sidecar_key: &str) -> Result<(), ProviderCryptoError> {
        let bytes = self.inner.read_file(sidecar_key).await?;
        let key = self.load_vault_key()?;
        let (_, payload) = open_sidecar(&key, &bytes)?;
        if object_key_known(&self.db, &self.rule_id, &payload.content_key)
            .map_err(ProviderCryptoError::Engine)?
        {
            return Ok(());
        }
        upsert_bootstrap_entry(
            &self.db,
            &self.rule_id,
            &payload.relative_path,
            &payload.content_key,
            payload.size,
            payload.modified_at,
            &payload.blake3,
        )
        .map_err(ProviderCryptoError::Engine)?;
        Ok(())
    }

    /// Resolve `(content_key, dek, is_fresh)` for a write: reuse the
    /// existing DEK on rewrite, mint fresh on first upload.
    ///
    /// Reuse matters twice. First, sharing (Round F3+) wraps a single
    /// per-object DEK per grantee; a fresh DEK on every write would
    /// invalidate every space-scoped sidecar the previous DEK backed.
    /// Second, atomicity: content and sidecar are two S3 PUTs, not one
    /// transaction. Rewrite ordering is content-first then sidecar;
    /// with a fresh DEK, a crash between the two leaves new content
    /// under new DEK next to old sidecar carrying the old wrapped DEK,
    /// and every subsequent `read_file` fails the AEAD tag. Reusing the
    /// existing DEK means the surviving old sidecar unwraps to the
    /// same key that seals the new content — read succeeds, only the
    /// plaintext-metadata fields (`size`, `modified_at`, `blake3`) lag
    /// until the next successful write.
    ///
    /// Fresh keys are deliberately not persisted here — the caller
    /// commits the mapping via `set_object_key` only after both content
    /// and sidecar uploads succeed. Otherwise a row inserted before the
    /// upload finishes would carry `deleted=0`, `object_key=<fresh>`,
    /// `size=0`, `modified_at=0`, and `manifest()`'s
    /// `!deleted && object_key.is_some()` filter would emit it as a
    /// real FileState — the download-direction diff would then schedule
    /// a fetch of an object the bucket never received.
    async fn resolve_or_mint_content_key_and_dek(
        &self,
        relative_path: &str,
        vault_key: &[u8; 32],
    ) -> Result<(String, Zeroizing<[u8; DEK_LEN]>, bool), ProviderCryptoError> {
        if let Some(existing) = lookup_object_key(&self.db, &self.rule_id, relative_path)? {
            let dek = self.load_existing_dek(&existing, vault_key).await?;
            return Ok((existing, dek, false));
        }
        Ok((generate_object_key(), Self::random_dek(), true))
    }

    /// Fetch the existing own-vault sidecar for `content_key` and
    /// return the DEK it wraps. Called on the rewrite path — see
    /// `resolve_or_mint_content_key_and_dek` for why the DEK is
    /// reused across writes.
    async fn load_existing_dek(
        &self,
        content_key: &str,
        vault_key: &[u8; 32],
    ) -> Result<Zeroizing<[u8; DEK_LEN]>, ProviderCryptoError> {
        let sidecar_bytes = self
            .inner
            .read_file(&own_sidecar_key_for(content_key))
            .await?;
        let (_, payload) = open_sidecar(vault_key, &sidecar_bytes)?;
        Ok(unwrap_dek(vault_key, &payload.wrapped_dek)?)
    }

    fn content_key_for_read(&self, relative_path: &str) -> Result<String, ProviderCryptoError> {
        lookup_object_key(&self.db, &self.rule_id, relative_path)?.ok_or_else(|| {
            ProviderCryptoError::MissingObjectKey {
                path: relative_path.to_string(),
            }
        })
    }

    async fn write_own_sidecar(
        &self,
        content_key: &str,
        payload: &SidecarPayload,
        vault_key: &[u8; 32],
    ) -> Result<(), ProviderCryptoError> {
        let bytes = seal_sidecar(vault_key, VAULT_KEY_EPOCH, Self::random_nonce(), payload)?;
        self.inner
            .write_file(&own_sidecar_key_for(content_key), &bytes)
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
        let content_key = self
            .content_key_for_read(relative_path)
            .map_err(SyncProviderError::from)?;
        let vault_key = self.load_vault_key().map_err(SyncProviderError::from)?;

        // Sidecar-first: it holds the wrapped DEK that seals the content
        // bytes. Fetching content first would leave us with ciphertext
        // and no key.
        let sidecar_bytes = self
            .inner
            .read_file(&own_sidecar_key_for(&content_key))
            .await?;
        let (_, payload) =
            open_sidecar(&vault_key, &sidecar_bytes).map_err(|e| SyncProviderError::Other {
                reason: e.to_string(),
            })?;
        let dek =
            unwrap_dek(&vault_key, &payload.wrapped_dek).map_err(|e| SyncProviderError::Other {
                reason: e.to_string(),
            })?;

        let ciphertext = self.inner.read_file(&payload.content_key).await?;
        let (_, plaintext) =
            open_bytes(&dek, &ciphertext).map_err(|e| SyncProviderError::Other {
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
        let content_key = self
            .content_key_for_read(relative_path)
            .map_err(SyncProviderError::from)?;
        let vault_key = self.load_vault_key().map_err(SyncProviderError::from)?;

        // Sidecar first, in RAM — it is always small.
        let sidecar_bytes = self
            .inner
            .read_file(&own_sidecar_key_for(&content_key))
            .await?;
        let (_, payload) =
            open_sidecar(&vault_key, &sidecar_bytes).map_err(|e| SyncProviderError::Other {
                reason: e.to_string(),
            })?;
        let dek =
            unwrap_dek(&vault_key, &payload.wrapped_dek).map_err(|e| SyncProviderError::Other {
                reason: e.to_string(),
            })?;

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
            .read_file_to_path(&payload.content_key, &ct_path, None, on_progress)
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
            open_stream(&dek, ct_info.bytes, &mut reader, &mut writer)
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
        let vault_key = self.load_vault_key().map_err(SyncProviderError::from)?;
        let (content_key, dek, is_fresh) = self
            .resolve_or_mint_content_key_and_dek(relative_path, &vault_key)
            .await
            .map_err(SyncProviderError::from)?;

        let ct = seal_bytes(&dek, VAULT_KEY_EPOCH, Self::random_nonce(), data).map_err(|e| {
            SyncProviderError::Other {
                reason: e.to_string(),
            }
        })?;
        self.inner.write_file(&content_key, &ct).await?;

        let wrapped_dek = wrap_dek(&vault_key, &dek).map_err(|e| SyncProviderError::Other {
            reason: e.to_string(),
        })?;
        let payload = SidecarPayload {
            content_key: content_key.clone(),
            wrapped_dek,
            relative_path: relative_path.to_string(),
            size: data.len() as u64,
            modified_at: unix_now(),
            content_type: None,
            blake3: blake3::hash(data).to_hex().to_string(),
        };
        self.write_own_sidecar(&content_key, &payload, &vault_key)
            .await
            .map_err(SyncProviderError::from)?;

        // Both uploads succeeded — safe to publish the mapping now.
        // See `resolve_or_mint_content_key_and_dek` for the deferred-
        // persistence rationale (phantom manifest entries on failed
        // uploads).
        if is_fresh {
            set_object_key(&self.db, &self.rule_id, relative_path, &content_key).map_err(|e| {
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
        let vault_key = self.load_vault_key().map_err(SyncProviderError::from)?;
        let (content_key, dek, is_fresh) = self
            .resolve_or_mint_content_key_and_dek(relative_path, &vault_key)
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
                &dek,
                VAULT_KEY_EPOCH,
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
            .write_file_from_path(&content_key, ct_tmp.path())
            .await?;

        let hash = hash_file_blake3(source_path)
            .await
            .map_err(SyncProviderError::Io)?;
        let wrapped_dek = wrap_dek(&vault_key, &dek).map_err(|e| SyncProviderError::Other {
            reason: e.to_string(),
        })?;
        let payload = SidecarPayload {
            content_key: content_key.clone(),
            wrapped_dek,
            relative_path: relative_path.to_string(),
            size: plaintext_len,
            modified_at: meta_mtime_secs(&meta),
            content_type: None,
            blake3: hash,
        };
        self.write_own_sidecar(&content_key, &payload, &vault_key)
            .await
            .map_err(SyncProviderError::from)?;

        // Both uploads succeeded — safe to publish the mapping now.
        if is_fresh {
            set_object_key(&self.db, &self.rule_id, relative_path, &content_key).map_err(|e| {
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
        // orphan-content pass on the next bootstrap surfaces the leftover;
        // orphan sidecars would just be logged and ignored, so the
        // surviving-sidecar failure mode is the noisier one. Delete-in-
        // place ignores `to_trash` — S3 has no trash, and
        // `CloudProvider::supports_trash()` returns false.
        if let Some(content_key) = lookup_object_key(&self.db, &self.rule_id, relative_path)
            .map_err(|e| SyncProviderError::Other {
                reason: e.to_string(),
            })?
        {
            let sidecar = own_sidecar_key_for(&content_key);
            // Ignore "not found" on the sidecar side — an already-missing
            // sidecar means a previous half-crashed delete, not a bug.
            match self.inner.delete_file(&sidecar, false).await {
                Ok(()) | Err(SyncProviderError::NotFound { .. }) => {}
                Err(e) => return Err(e),
            }
            match self.inner.delete_file(&content_key, false).await {
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
