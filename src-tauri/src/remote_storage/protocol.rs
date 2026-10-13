//! One [`RemoteStore`] for every protocol (spec 050 research R1): each call goes to the store of
//! the protocol of `access`, so no caller needs to know whether a storage speaks S3, WebDAV or
//! SFTP (FR-018). A protocol without a store yet answers [`StorageError::Unsupported`]. A folder on
//! this device ([`Protocol::Local`], spec 048 research R15) always has its store.

use std::sync::Arc;

use async_trait::async_trait;
use tokio::time::Instant;

use super::local::LocalStore;
use super::{
    Access, DirListing, Location, ObjectHead, ObjectInfo, Protocol, RemoteStore, StorageError,
};

/// The stores of the protocols, chosen per call by `access.location.protocol`.
pub struct ProtocolStore {
    s3: Arc<dyn RemoteStore>,
    webdav: Option<Arc<dyn RemoteStore>>,
    sftp: Option<Arc<dyn RemoteStore>>,
    local: Arc<dyn RemoteStore>,
}

impl ProtocolStore {
    /// A store with `s3`, a [`LocalStore`] and, when given, the stores for WebDAV and SFTP.
    pub fn new(
        s3: Arc<dyn RemoteStore>,
        webdav: Option<Arc<dyn RemoteStore>>,
        sftp: Option<Arc<dyn RemoteStore>>,
    ) -> Self {
        Self {
            s3,
            webdav,
            sftp,
            local: Arc::new(LocalStore::new()),
        }
    }

    /// The store for the protocol of `access`.
    fn of(&self, access: &Access) -> Result<&dyn RemoteStore, StorageError> {
        let store = match access.location.protocol {
            Protocol::S3 => Some(&self.s3),
            Protocol::WebDav => self.webdav.as_ref(),
            Protocol::Sftp => self.sftp.as_ref(),
            Protocol::Local => Some(&self.local),
        };
        store.map(Arc::as_ref).ok_or_else(|| {
            log::warn!(
                "remote storage: no store for {}",
                access.location.protocol.as_str()
            );
            StorageError::Unsupported
        })
    }
}

#[async_trait]
impl RemoteStore for ProtocolStore {
    async fn put(
        &self,
        access: &Access,
        key: &str,
        body: Vec<u8>,
        deadline: Instant,
    ) -> Result<(), StorageError> {
        self.of(access)?.put(access, key, body, deadline).await
    }

    async fn get(
        &self,
        access: &Access,
        key: &str,
        max_bytes: usize,
        deadline: Instant,
    ) -> Result<Vec<u8>, StorageError> {
        self.of(access)?.get(access, key, max_bytes, deadline).await
    }

    async fn list(
        &self,
        access: &Access,
        prefix: &str,
        max: usize,
        deadline: Instant,
    ) -> Result<Vec<ObjectInfo>, StorageError> {
        self.of(access)?.list(access, prefix, max, deadline).await
    }

    async fn delete(
        &self,
        access: &Access,
        key: &str,
        deadline: Instant,
    ) -> Result<(), StorageError> {
        self.of(access)?.delete(access, key, deadline).await
    }

    async fn head(
        &self,
        access: &Access,
        key: &str,
        deadline: Instant,
    ) -> Result<ObjectHead, StorageError> {
        self.of(access)?.head(access, key, deadline).await
    }

    async fn get_range(
        &self,
        access: &Access,
        key: &str,
        start: u64,
        len: u64,
        deadline: Instant,
    ) -> Result<Box<dyn tokio::io::AsyncRead + Send + Unpin>, StorageError> {
        self.of(access)?
            .get_range(access, key, start, len, deadline)
            .await
    }

    async fn list_dir(
        &self,
        access: &Access,
        prefix: &str,
        max: usize,
        deadline: Instant,
    ) -> Result<DirListing, StorageError> {
        self.of(access)?
            .list_dir(access, prefix, max, deadline)
            .await
    }

    async fn copy(
        &self,
        access: &Access,
        from: &str,
        to: &str,
        deadline: Instant,
    ) -> Result<(), StorageError> {
        self.of(access)?.copy(access, from, to, deadline).await
    }

    fn copies_on_provider(&self, access: &Access) -> bool {
        self.of(access)
            .is_ok_and(|store| store.copies_on_provider(access))
    }

    fn shown_host_key(&self, location: &Location) -> Option<String> {
        match location.protocol {
            Protocol::Sftp => self.sftp.as_ref()?.shown_host_key(location),
            _ => None,
        }
    }

    async fn rename_tree(
        &self,
        access: &Access,
        from: &str,
        to: &str,
        deadline: Instant,
    ) -> Result<(), StorageError> {
        self.of(access)?
            .rename_tree(access, from, to, deadline)
            .await
    }

    async fn remove_tree(
        &self,
        access: &Access,
        prefix: &str,
        deadline: Instant,
    ) -> Result<(), StorageError> {
        self.of(access)?.remove_tree(access, prefix, deadline).await
    }

    async fn begin_upload(
        &self,
        access: &Access,
        key: &str,
        deadline: Instant,
    ) -> Result<String, StorageError> {
        self.of(access)?.begin_upload(access, key, deadline).await
    }

    async fn upload_part(
        &self,
        access: &Access,
        key: &str,
        upload_id: &str,
        number: u16,
        body: Vec<u8>,
        deadline: Instant,
    ) -> Result<String, StorageError> {
        self.of(access)?
            .upload_part(access, key, upload_id, number, body, deadline)
            .await
    }

    async fn finish_upload(
        &self,
        access: &Access,
        key: &str,
        upload_id: &str,
        marks: &[String],
        deadline: Instant,
    ) -> Result<(), StorageError> {
        self.of(access)?
            .finish_upload(access, key, upload_id, marks, deadline)
            .await
    }

    async fn abort_upload(
        &self,
        access: &Access,
        key: &str,
        upload_id: &str,
        deadline: Instant,
    ) -> Result<(), StorageError> {
        self.of(access)?
            .abort_upload(access, key, upload_id, deadline)
            .await
    }
}
