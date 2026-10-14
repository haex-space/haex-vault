//! The one seam to a provider (spec 038 research R10, spec 050 contracts/object-store.md): the S3,
//! WebDAV and SFTP stores behind [`super::protocol::ProtocolStore`], or a fake in tests. Every call
//! ends at `deadline`.

use async_trait::async_trait;
use tokio::time::Instant;

use super::{Access, DirListing, Location, ObjectHead, ObjectInfo, StorageError};

/// The provider behind a storage. Keys are relative to the bucket or folder of `access`; a key
/// ending in `/` names a folder.
#[async_trait]
pub trait ObjectStore: Send + Sync {
    /// Writes `body` to `key` in the bucket described by `access`, before `deadline`.
    async fn put(
        &self,
        access: &Access,
        key: &str,
        body: Vec<u8>,
        deadline: Instant,
    ) -> Result<(), StorageError>;

    /// The object, read up to `max_bytes` (more is [`StorageError::TooLarge`]).
    async fn get(
        &self,
        access: &Access,
        key: &str,
        max_bytes: usize,
        deadline: Instant,
    ) -> Result<Vec<u8>, StorageError>;

    /// The objects under `prefix`, following the provider's pages up to `max` (more is
    /// [`StorageError::TooLarge`]).
    async fn list(
        &self,
        access: &Access,
        prefix: &str,
        max: usize,
        deadline: Instant,
    ) -> Result<Vec<ObjectInfo>, StorageError>;

    /// Deletes `key` from the bucket described by `access`, before `deadline`.
    async fn delete(
        &self,
        access: &Access,
        key: &str,
        deadline: Instant,
    ) -> Result<(), StorageError>;

    /// Size and time of `key` (spec 044).
    async fn head(
        &self,
        access: &Access,
        key: &str,
        deadline: Instant,
    ) -> Result<ObjectHead, StorageError>;

    /// A reader for `len` bytes of `key` from `start` (spec 044 FR-012, FR-013). `deadline` holds
    /// until the answer starts; the body then streams without one (a film plays for hours) and is
    /// never held whole.
    async fn get_range(
        &self,
        access: &Access,
        key: &str,
        start: u64,
        len: u64,
        deadline: Instant,
    ) -> Result<Box<dyn tokio::io::AsyncRead + Send + Unpin>, StorageError>;

    /// One level below `prefix` (spec 044 FR-034), following pages up to `max` entries (more is
    /// [`StorageError::TooLarge`]).
    async fn list_dir(
        &self,
        access: &Access,
        prefix: &str,
        max: usize,
        deadline: Instant,
    ) -> Result<DirListing, StorageError>;

    /// Copies `from` to `to` in the same bucket on the provider (spec 044: renaming, moving within
    /// a storage); objects up to 5 GB, the limit of S3's `CopyObject`. Only called when
    /// [`ObjectStore::copies_on_provider`] says so.
    async fn copy(
        &self,
        access: &Access,
        from: &str,
        to: &str,
        deadline: Instant,
    ) -> Result<(), StorageError>;

    /// Whether [`ObjectStore::copy`] copies on the provider (spec 050 research R8); otherwise a
    /// copy streams through holzi.
    fn copies_on_provider(&self, access: &Access) -> bool;

    /// The host key the server at `location` showed when the last connection was refused for it
    /// (spec 050 research R5), as an OpenSSH line; `None` for protocols without host keys.
    fn shown_host_key(&self, _location: &Location) -> Option<String> {
        None
    }

    /// Moves the file or folder `from` to `to` in one call (spec 050 research R8). Only for
    /// protocols with real folders; S3 answers [`StorageError::Unsupported`].
    async fn rename_tree(
        &self,
        access: &Access,
        from: &str,
        to: &str,
        deadline: Instant,
    ) -> Result<(), StorageError>;

    /// Removes the folder `prefix` with everything in it (spec 050 research R8). Only for protocols
    /// with real folders; S3 answers [`StorageError::Unsupported`].
    async fn remove_tree(
        &self,
        access: &Access,
        prefix: &str,
        deadline: Instant,
    ) -> Result<(), StorageError>;

    /// Starts an upload in parts of `key` (spec 044 FR-020, spec 050 research R7); returns its id.
    async fn begin_upload(
        &self,
        access: &Access,
        key: &str,
        deadline: Instant,
    ) -> Result<String, StorageError>;

    /// Uploads part `number` (from 1, in order) of an upload; returns the part's mark (S3: its
    /// ETag).
    async fn upload_part(
        &self,
        access: &Access,
        key: &str,
        upload_id: &str,
        number: u16,
        body: Vec<u8>,
        deadline: Instant,
    ) -> Result<String, StorageError>;

    /// Joins the parts (their marks in order) into the object under `key`.
    async fn finish_upload(
        &self,
        access: &Access,
        key: &str,
        upload_id: &str,
        marks: &[String],
        deadline: Instant,
    ) -> Result<(), StorageError>;

    /// Drops an upload and its parts, so nothing half stays at the provider (FR-020).
    async fn abort_upload(
        &self,
        access: &Access,
        key: &str,
        upload_id: &str,
        deadline: Instant,
    ) -> Result<(), StorageError>;
}
