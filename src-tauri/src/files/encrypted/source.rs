//! Reading a file of an encrypted folder by range (research R5, FR-015): the plaintext range maps to
//! the blocks that hold it, one `get_range` fetches them (at most [`WINDOW`] of plaintext at a
//! time), and every block is checked and opened before a byte of it goes out. The object's header
//! and length are checked once when the file opens; a failure marks the entry damaged (FR-022 to
//! FR-025). [`EncryptedFileSource`] serves the media server window by window, never the whole file.

use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use tokio::io::{AsyncRead, AsyncReadExt};
use tokio::time::Instant;

use super::format::content::{ContentOpener, HEADER_LEN};
use super::format::Id;
use super::tree::{facts, Version};
use super::{damaged, OpenFolder};
use crate::files::storage_source::{storage_error, StorageFiles};
use crate::files::streaming::StreamingSource;
use crate::files::FilesError;
use crate::remote_storage::{Access, RemoteStore, StorageError};

/// The most plaintext one request fetches.
pub const WINDOW: u64 = 8 * 1024 * 1024;

const CALL: Duration = Duration::from_secs(30);

fn soon() -> Instant {
    Instant::now() + CALL
}

/// One opened file of an encrypted folder.
pub struct EncryptedFile {
    store: Arc<dyn RemoteStore>,
    access: Access,
    key: String,
    opener: ContentOpener,
    size: u64,
    folder: Arc<OpenFolder>,
    sid: Id,
}

impl EncryptedFile {
    /// Opens the content object of `version` in `folder`: unwraps its `DEK`, checks its length and
    /// its header against the sidecar.
    pub async fn open(
        files: &StorageFiles,
        folder: Arc<OpenFolder>,
        version: &Version,
    ) -> Result<Self, FilesError> {
        let facts = facts(version).ok_or_else(|| FilesError::invalid_path("not a file"))?;
        let key = folder.content_key(&facts.content);
        let (store, access) = (Arc::clone(files.store()), files.access().clone());
        let checked = async {
            let dek = folder.unwrap_dek(&version.sidecar.entry, facts)?;
            let head = store
                .head(&access, &key, soon())
                .await
                .map_err(missing_is_damaged)?;
            let mut header = Vec::with_capacity(HEADER_LEN);
            store
                .get_range(&access, &key, 0, HEADER_LEN as u64, soon())
                .await
                .map_err(missing_is_damaged)?
                .read_to_end(&mut header)
                .await
                .map_err(|_| storage_error(StorageError::Network))?;
            let opener =
                ContentOpener::new(&header, &facts.cid, &dek, facts.size).map_err(|_| damaged())?;
            opener.check_len(head.size).map_err(|_| damaged())?;
            Ok(opener)
        };
        let opener = match checked.await {
            Ok(opener) => opener,
            Err(error) => return Err(mark(&folder, version.sid, error).await),
        };
        Ok(Self {
            store,
            access,
            key,
            opener,
            size: facts.size,
            folder,
            sid: version.sid,
        })
    }

    /// The plaintext size.
    pub fn size(&self) -> u64 {
        self.size
    }

    /// Plaintext `[start, end)` through one request; callers keep it within [`WINDOW`].
    pub async fn read(&self, start: u64, end: u64) -> Result<Vec<u8>, FilesError> {
        if start >= end {
            return Ok(Vec::new());
        }
        let result = async {
            let range = self
                .opener
                .ciphertext_range(start, end)
                .map_err(|_| FilesError::invalid_path("outside the file"))?;
            let mut bytes = Vec::new();
            self.store
                .get_range(
                    &self.access,
                    &self.key,
                    range.start,
                    range.end - range.start,
                    soon(),
                )
                .await
                .map_err(missing_is_damaged)?
                .read_to_end(&mut bytes)
                .await
                .map_err(|_| storage_error(StorageError::Network))?;
            self.opener
                .open_range(start, end, &bytes)
                .map_err(|_| damaged())
        }
        .await;
        match result {
            Ok(plain) => Ok(plain),
            Err(error) => Err(mark(&self.folder, self.sid, error).await),
        }
    }

    /// A reader for plaintext `[start, start + len)`, one window at a time.
    pub fn reader(self: Arc<Self>, start: u64, len: u64) -> Box<dyn AsyncRead + Send + Unpin> {
        let end = start.saturating_add(len).min(self.size);
        let windows = futures::stream::unfold((self, start), move |(file, at)| async move {
            if at >= end {
                return None;
            }
            let until = end.min(at + WINDOW);
            let chunk = file
                .read(at, until)
                .await
                .map(std::io::Cursor::new)
                .map_err(|error| std::io::Error::other(error.message));
            Some((chunk, (file, until)))
        });
        Box::new(tokio_util::io::StreamReader::new(Box::pin(windows)))
    }
}

/// A missing object is damage of the entry, not a missing entry.
fn missing_is_damaged(error: StorageError) -> FilesError {
    match error {
        StorageError::NotFound => damaged(),
        other => storage_error(other),
    }
}

/// Marks the entry damaged when `error` says so, and hands the error back.
async fn mark(folder: &OpenFolder, sid: Id, error: FilesError) -> FilesError {
    if error.code == crate::files::FilesErrorCode::Damaged {
        folder.mark_damaged(sid).await;
    }
    error
}

/// A file of an encrypted folder for the media server (images and PDF; video and audio with US4).
pub struct EncryptedFileSource {
    file: Arc<EncryptedFile>,
    content_type: String,
}

impl EncryptedFileSource {
    pub fn new(file: EncryptedFile, content_type: impl Into<String>) -> Self {
        Self {
            file: Arc::new(file),
            content_type: content_type.into(),
        }
    }
}

#[async_trait]
impl StreamingSource for EncryptedFileSource {
    async fn size(&self) -> std::io::Result<u64> {
        Ok(self.file.size())
    }

    async fn open_range(
        &self,
        start: u64,
        len: u64,
    ) -> std::io::Result<Box<dyn AsyncRead + Send + Unpin>> {
        Ok(Arc::clone(&self.file).reader(start, len))
    }

    fn content_type(&self) -> &str {
        &self.content_type
    }
}

#[cfg(test)]
#[path = "source_tests.rs"]
mod tests;
