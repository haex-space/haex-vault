//! Round F3a — space-scoped `SyncProvider` decorator.
//!
//! Sibling of [`EncryptingSyncProvider`] for the shared-space cloud path:
//! same per-object DEK sealing the content bytes, but the KEK that wraps
//! the DEK is the current MLS epoch key for `space_id` instead of the
//! owner's vault key. The content object lives at exactly the same
//! `content/o/<hex>` path as the own-vault path — one physical object per
//! file, regardless of how many spaces grant access — and this decorator
//! only differs on the sidecar prefix (`space-<space_id>/<hex>.m`).
//!
//! ## Two-key threading
//!
//! Each write resolves the current epoch key twice:
//! 1. To wrap the per-object DEK (`wrapped_dek_epoch` in the payload).
//! 2. To seal the sidecar envelope bytes (`envelope.epoch` on the wire).
//!
//! Both start out equal on a fresh write. Round F5's revocation-driven
//! rewrap will rotate the envelope epoch — the sidecar gets resealed
//! under the new epoch key — while the DEK stays the same and its
//! `wrapped_dek_epoch` is what tells the reader which historical key to
//! unwrap the DEK under. Keeping the two fields distinct lets one
//! change without disturbing the other's on-wire meaning.
//!
//! ## Test hook
//!
//! Production wires the decorator with [`MlsSpaceKeyResolver`], which
//! reads the local MLS group state via
//! [`key_resolver::resolve_latest`](crate::file_sync::crypto::key_resolver::resolve_latest).
//! Tests substitute an in-memory implementation that pins a fixed
//! `(epoch, key)` pair so the write path stays exercisable without
//! spinning up a full MLS group.

use std::path::Path;
use std::sync::Arc;

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
use super::content::{open_bytes, open_stream, seal_bytes, seal_stream};
use super::dek_wrap::{unwrap_dek, wrap_dek, DEK_LEN};
use super::envelope::{EnvelopeHeader, NONCE_SIZE};
use super::key_resolver::{resolve_key, resolve_latest, KeyError};
use super::object_key::{
    generate_object_key, lookup_object_key, mark_object_deleted, object_key_known, set_object_key,
    space_sidecar_key_for, space_sidecar_prefix, upsert_bootstrap_entry, SIDECAR_SUFFIX,
};
use super::provider::{
    hash_file_blake3, meta_mtime_secs, staging_tempfile, unix_now, ProviderCryptoError,
};
use super::sidecar::{open_sidecar, seal_sidecar, SidecarPayload};

/// Resolve the KEK for space-scoped grants.
///
/// Production goes through the MLS epoch resolver
/// ([`MlsSpaceKeyResolver`]); tests swap in a fixed-epoch stub so the
/// write path stays exercisable without a live MLS group.
///
/// Split from the module functions rather than picked with a runtime
/// `if cfg!(test)` so the boundary is explicit at call sites and the
/// production path never risks running the test-only shim.
pub trait SpaceKeyResolver: Send + Sync {
    /// `(current_epoch, key)` for a fresh seal.
    fn resolve_latest(
        &self,
        space_id: &str,
        db: &DbConnection,
    ) -> Result<(u64, [u8; 32]), KeyError>;

    /// Key at a specific historical epoch, for opening an existing
    /// sidecar or unwrapping a DEK whose `wrapped_dek_epoch` predates the
    /// current one.
    fn resolve_key(
        &self,
        space_id: &str,
        epoch: u64,
        db: &DbConnection,
    ) -> Result<[u8; 32], KeyError>;
}

/// Production resolver — delegates straight to
/// [`key_resolver::resolve_latest`] and [`key_resolver::resolve_key`].
///
/// Unit struct so it can be `Arc::new`'d cheaply and shared across
/// providers built for different rules of the same MLS-backed vault.
pub struct MlsSpaceKeyResolver;

impl SpaceKeyResolver for MlsSpaceKeyResolver {
    fn resolve_latest(
        &self,
        space_id: &str,
        db: &DbConnection,
    ) -> Result<(u64, [u8; 32]), KeyError> {
        resolve_latest(space_id, db)
    }

    fn resolve_key(
        &self,
        space_id: &str,
        epoch: u64,
        db: &DbConnection,
    ) -> Result<[u8; 32], KeyError> {
        resolve_key(space_id, epoch, db)
    }
}

/// Space-scoped encrypting decorator. Wraps an inner `SyncProvider` and
/// treats every content-plane object as opaque `content/o/<hex>` /
/// `space-<space_id>/<hex>.m` — the inner provider (in production a
/// `CloudProvider`) must accept opaque keys as its `relative_path`
/// argument, same contract the own-vault [`EncryptingSyncProvider`]
/// assumes.
pub struct SpaceContentSyncProvider {
    inner: Arc<dyn SyncProvider>,
    rule_id: String,
    db: DbConnection,
    space_id: String,
    resolver: Arc<dyn SpaceKeyResolver>,
}

impl SpaceContentSyncProvider {
    pub fn new(
        inner: Arc<dyn SyncProvider>,
        rule_id: impl Into<String>,
        db: DbConnection,
        space_id: impl Into<String>,
        resolver: Arc<dyn SpaceKeyResolver>,
    ) -> Self {
        Self {
            inner,
            rule_id: rule_id.into(),
            db,
            space_id: space_id.into(),
            resolver,
        }
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

    /// Bootstrap the object-key cache from the remote bucket by scanning
    /// this space's sidecar prefix. Log-and-continue on per-object
    /// failures — one corrupt sidecar cannot brick recovery of the rest.
    async fn bootstrap(&self) -> Result<(), ProviderCryptoError> {
        let manifest = self.inner.manifest().await?;
        let prefix = space_sidecar_prefix(&self.space_id);
        for entry in &manifest {
            if entry.is_directory {
                continue;
            }
            if !entry.relative_path.starts_with(&prefix)
                || !entry.relative_path.ends_with(SIDECAR_SUFFIX)
            {
                continue;
            }
            if let Err(e) = self.recover_and_store(&entry.relative_path).await {
                eprintln!(
                    "[SpaceContentSyncProvider] bootstrap: failed to recover sidecar {} \
                     (rule {}, space {}): {e}",
                    entry.relative_path, self.rule_id, self.space_id,
                );
                continue;
            }
        }
        Ok(())
    }

    async fn recover_and_store(&self, sidecar_key: &str) -> Result<(), ProviderCryptoError> {
        let bytes = self.inner.read_file(sidecar_key).await?;
        let epoch = EnvelopeHeader::parse(&bytes)
            .map_err(|e| ProviderCryptoError::Sidecar(e.into()))?
            .epoch;
        let kek = self.resolver.resolve_key(&self.space_id, epoch, &self.db)?;
        let (_, payload) = open_sidecar(&kek, &bytes)?;
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

    /// Resolve `(content_key, dek, is_fresh)` for a write, mirroring the
    /// own-vault provider's rewrite semantics: the DEK is per-content-
    /// object and reused across writes so a mid-flight crash between the
    /// content PUT and the sidecar PUT still leaves the file readable.
    /// A fresh DEK per write would AEAD-fail every subsequent
    /// `read_file` after such a crash.
    async fn resolve_or_mint_content_key_and_dek(
        &self,
        relative_path: &str,
    ) -> Result<(String, Zeroizing<[u8; DEK_LEN]>, bool), ProviderCryptoError> {
        if let Some(existing) = lookup_object_key(&self.db, &self.rule_id, relative_path)? {
            let dek = self.load_existing_dek(&existing).await?;
            return Ok((existing, dek, false));
        }
        Ok((generate_object_key(), Self::random_dek(), true))
    }

    /// Open the existing space-sidecar for `content_key` and return the
    /// DEK it wraps. The sidecar's envelope epoch tells us which epoch
    /// key seals the sidecar bytes; the payload's `wrapped_dek_epoch`
    /// tells us which epoch key wraps the DEK. In the common case both
    /// are equal; after a Round F5 rewrap they will differ.
    async fn load_existing_dek(
        &self,
        content_key: &str,
    ) -> Result<Zeroizing<[u8; DEK_LEN]>, ProviderCryptoError> {
        Ok(self.open_space_sidecar(content_key).await?.1)
    }

    /// Open a space sidecar and its wrapped DEK. The envelope epoch selects
    /// the key that seals the sidecar bytes; `wrapped_dek_epoch` selects the
    /// key that wraps the DEK, which may differ after a future rewrap.
    async fn open_space_sidecar(
        &self,
        content_key: &str,
    ) -> Result<(SidecarPayload, Zeroizing<[u8; DEK_LEN]>), ProviderCryptoError> {
        let sidecar_key = space_sidecar_key_for(&self.space_id, content_key);
        let bytes = self.inner.read_file(&sidecar_key).await?;
        let envelope_epoch = EnvelopeHeader::parse(&bytes)
            .map_err(|e| ProviderCryptoError::Sidecar(e.into()))?
            .epoch;
        let envelope_kek = self
            .resolver
            .resolve_key(&self.space_id, envelope_epoch, &self.db)?;
        let (_, payload) = open_sidecar(&envelope_kek, &bytes)?;
        let dek_epoch = payload.wrapped_dek_epoch.unwrap_or(envelope_epoch);
        let dek_kek = if dek_epoch == envelope_epoch {
            envelope_kek
        } else {
            self.resolver
                .resolve_key(&self.space_id, dek_epoch, &self.db)?
        };
        let dek = unwrap_dek(&dek_kek, &payload.wrapped_dek)?;
        Ok((payload, dek))
    }

    fn content_key_for_read(&self, relative_path: &str) -> Result<String, ProviderCryptoError> {
        lookup_object_key(&self.db, &self.rule_id, relative_path)?.ok_or_else(|| {
            ProviderCryptoError::MissingObjectKey {
                path: relative_path.to_string(),
            }
        })
    }

    async fn write_space_sidecar(
        &self,
        content_key: &str,
        epoch: u64,
        kek: &[u8; 32],
        payload: &SidecarPayload,
    ) -> Result<(), ProviderCryptoError> {
        let bytes = seal_sidecar(kek, epoch, Self::random_nonce(), payload)?;
        self.inner
            .write_file(&space_sidecar_key_for(&self.space_id, content_key), &bytes)
            .await?;
        Ok(())
    }
}

#[async_trait]
impl SyncProvider for SpaceContentSyncProvider {
    fn display_name(&self) -> String {
        format!("space-crypto:{}", self.inner.display_name())
    }

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

        let (payload, dek) = self
            .open_space_sidecar(&content_key)
            .await
            .map_err(SyncProviderError::from)?;

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

        let (payload, dek) = self
            .open_space_sidecar(&content_key)
            .await
            .map_err(SyncProviderError::from)?;

        if let Some(parent) = output_path.parent() {
            tokio::fs::create_dir_all(parent)
                .await
                .map_err(SyncProviderError::Io)?;
        }

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
        pt_tmp.persist(output_path).map_err(|e| {
            SyncProviderError::Io(std::io::Error::other(format!(
                "persist decrypted tempfile: {e}"
            )))
        })?;

        let plaintext_size = tokio::fs::metadata(output_path)
            .await
            .map_err(SyncProviderError::Io)?
            .len();
        Ok(ReadFileResult {
            bytes: plaintext_size,
            hash: None,
        })
    }

    async fn write_file(&self, relative_path: &str, data: &[u8]) -> Result<(), SyncProviderError> {
        validate_relative_path(relative_path)?;
        let (content_key, dek, is_fresh) = self
            .resolve_or_mint_content_key_and_dek(relative_path)
            .await
            .map_err(SyncProviderError::from)?;

        let (epoch, kek) = self
            .resolver
            .resolve_latest(&self.space_id, &self.db)
            .map_err(|e| SyncProviderError::Other {
                reason: e.to_string(),
            })?;

        let ct = seal_bytes(&dek, epoch, Self::random_nonce(), data).map_err(|e| {
            SyncProviderError::Other {
                reason: e.to_string(),
            }
        })?;
        self.inner.write_file(&content_key, &ct).await?;

        let wrapped_dek = wrap_dek(&kek, &dek).map_err(|e| SyncProviderError::Other {
            reason: e.to_string(),
        })?;
        let payload = SidecarPayload {
            content_key: content_key.clone(),
            wrapped_dek,
            wrapped_dek_epoch: Some(epoch),
            relative_path: relative_path.to_string(),
            size: data.len() as u64,
            modified_at: unix_now(),
            content_type: None,
            blake3: blake3::hash(data).to_hex().to_string(),
        };
        self.write_space_sidecar(&content_key, epoch, &kek, &payload)
            .await
            .map_err(SyncProviderError::from)?;

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
        let (content_key, dek, is_fresh) = self
            .resolve_or_mint_content_key_and_dek(relative_path)
            .await
            .map_err(SyncProviderError::from)?;

        let (epoch, kek) = self
            .resolver
            .resolve_latest(&self.space_id, &self.db)
            .map_err(|e| SyncProviderError::Other {
                reason: e.to_string(),
            })?;

        let meta = tokio::fs::metadata(source_path)
            .await
            .map_err(SyncProviderError::Io)?;
        let plaintext_len = meta.len();

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
            .write_file_from_path(&content_key, ct_tmp.path())
            .await?;

        let hash = hash_file_blake3(source_path)
            .await
            .map_err(SyncProviderError::Io)?;
        let wrapped_dek = wrap_dek(&kek, &dek).map_err(|e| SyncProviderError::Other {
            reason: e.to_string(),
        })?;
        let payload = SidecarPayload {
            content_key: content_key.clone(),
            wrapped_dek,
            wrapped_dek_epoch: Some(epoch),
            relative_path: relative_path.to_string(),
            size: plaintext_len,
            modified_at: meta_mtime_secs(&meta),
            content_type: None,
            blake3: hash,
        };
        self.write_space_sidecar(&content_key, epoch, &kek, &payload)
            .await
            .map_err(SyncProviderError::from)?;

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
        if let Some(content_key) = lookup_object_key(&self.db, &self.rule_id, relative_path)
            .map_err(|e| SyncProviderError::Other {
                reason: e.to_string(),
            })?
        {
            let sidecar = space_sidecar_key_for(&self.space_id, &content_key);
            // Space-scoped delete only touches the sidecar for THIS
            // space. The content object may still be referenced by an
            // own-vault sidecar or by other spaces — leaving it in place
            // is correct. A Round F5 revocation pass will GC content
            // objects with no remaining sidecars.
            match self.inner.delete_file(&sidecar, false).await {
                Ok(()) | Err(SyncProviderError::NotFound { .. }) => {}
                Err(e) => return Err(e),
            }
        }
        mark_object_deleted(&self.db, &self.rule_id, relative_path).map_err(|e| {
            SyncProviderError::Other {
                reason: e.to_string(),
            }
        })?;
        let _ = mark_deleted(&self.db, &self.rule_id, relative_path);
        Ok(())
    }

    async fn create_directory(&self, _relative_path: &str) -> Result<(), SyncProviderError> {
        Ok(())
    }

    fn supports_streaming(&self) -> bool {
        true
    }

    fn supports_trash(&self) -> bool {
        false
    }

    fn supports_directories(&self) -> bool {
        false
    }

    async fn prime_hash_after_write(&self, _file: &FileState) {}
}
