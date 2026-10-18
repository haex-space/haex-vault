//! Where the media server reads a file from (spec 044 FR-012, FR-013, research R4; after haex-vault 2.x (Legacy)
//! `remote_storage/streaming/source.rs`): its size, its media type and a reader for one byte range.
//! The server copies that reader through a fixed buffer, so no file is ever held whole in memory.
//! Storages (US5) add a source that reads ranges from S3.

use std::io::SeekFrom;
use std::path::PathBuf;

use async_trait::async_trait;
use tokio::io::{AsyncRead, AsyncReadExt, AsyncSeekExt};

/// A file the media server can serve.
#[async_trait]
pub trait StreamingSource: Send + Sync {
    /// Its size in bytes.
    async fn size(&self) -> std::io::Result<u64>;
    /// A reader for `len` bytes from `start`.
    async fn open_range(
        &self,
        start: u64,
        len: u64,
    ) -> std::io::Result<Box<dyn AsyncRead + Send + Unpin>>;
    /// Its media type.
    fn content_type(&self) -> &str;
}

/// A file on this device.
pub struct LocalFileSource {
    path: PathBuf,
    content_type: String,
}

impl LocalFileSource {
    pub fn new(path: PathBuf, content_type: impl Into<String>) -> Self {
        Self {
            path,
            content_type: content_type.into(),
        }
    }
}

#[async_trait]
impl StreamingSource for LocalFileSource {
    async fn size(&self) -> std::io::Result<u64> {
        Ok(tokio::fs::metadata(&self.path).await?.len())
    }

    async fn open_range(
        &self,
        start: u64,
        len: u64,
    ) -> std::io::Result<Box<dyn AsyncRead + Send + Unpin>> {
        let mut file = tokio::fs::File::open(&self.path).await?;
        file.seek(SeekFrom::Start(start)).await?;
        Ok(Box::new(file.take(len)))
    }

    fn content_type(&self) -> &str {
        &self.content_type
    }
}

/// A file of a storage (spec 044 US5): each range the player asks for is its own request to the
/// provider, so seeking needs no download up to that point.
pub struct StorageFileSource {
    store: std::sync::Arc<dyn crate::remote_storage::ObjectStore>,
    access: crate::remote_storage::Access,
    key: String,
    size: u64,
    content_type: String,
}

impl StorageFileSource {
    pub fn new(
        store: std::sync::Arc<dyn crate::remote_storage::ObjectStore>,
        access: crate::remote_storage::Access,
        key: String,
        size: u64,
        content_type: impl Into<String>,
    ) -> Self {
        Self {
            store,
            access,
            key,
            size,
            content_type: content_type.into(),
        }
    }
}

#[async_trait]
impl StreamingSource for StorageFileSource {
    async fn size(&self) -> std::io::Result<u64> {
        Ok(self.size)
    }

    async fn open_range(
        &self,
        start: u64,
        len: u64,
    ) -> std::io::Result<Box<dyn AsyncRead + Send + Unpin>> {
        let deadline = tokio::time::Instant::now() + std::time::Duration::from_secs(30);
        self.store
            .get_range(&self.access, &self.key, start, len, deadline)
            .await
            .map_err(|error| std::io::Error::other(error.to_string()))
    }

    fn content_type(&self) -> &str {
        &self.content_type
    }
}
