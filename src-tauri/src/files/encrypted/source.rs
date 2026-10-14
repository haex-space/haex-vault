//! Reading a file of an encrypted folder by range (research R5, FR-015): the plaintext range maps to
//! the blocks that hold it, one `get_range` fetches them (at most [`WINDOW`] of plaintext at a
//! time), and every block is checked and opened before a byte of it goes out. The object's header
//! and length are checked once when the file opens; a failure marks the entry damaged (FR-022 to
//! FR-025). [`EncryptedFileSource`] serves the media server window by window, never the whole file,
//! and keeps the last two windows (SC-004).

use std::collections::VecDeque;
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::time::Duration;

use async_trait::async_trait;
use tokio::io::{AsyncRead, AsyncReadExt};
use tokio::time::Instant;
use tokio_util::bytes::Bytes;

use super::format::content::{ContentOpener, HEADER_LEN};
use super::format::{Id, CHUNK};
use super::tree::{facts, Version};
use super::{damaged, OpenFolder};
use crate::files::storage_source::{storage_error, StorageFiles};
use crate::files::streaming::StreamingSource;
use crate::files::FilesError;
use crate::remote_storage::{Access, ObjectStore, StorageError};

/// The most plaintext one request fetches.
pub const WINDOW: u64 = 8 * 1024 * 1024;

const CALL: Duration = Duration::from_secs(30);

fn soon() -> Instant {
    Instant::now() + CALL
}

/// One opened file of an encrypted folder.
pub struct EncryptedFile {
    store: Arc<dyn ObjectStore>,
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

/// The windows one open file keeps (research R5): at most `KEPT` × [`WINDOW`] of plaintext. A
/// stream still sending a piece of a dropped window holds that window until it is sent, so each
/// running request adds at most one window more.
const KEPT: usize = 2;

/// Opened plaintext of whole blocks from `start`, at most [`WINDOW`] long.
struct Window {
    start: u64,
    bytes: Bytes,
}

impl Window {
    fn end(&self) -> u64 {
        self.start + self.bytes.len() as u64
    }
}

/// A file of an encrypted folder for the media server (image, PDF, video, audio). A range maps to
/// the blocks that hold it, fetched one window at a time; the last [`KEPT`] windows stay in memory,
/// so a player that reads on or asks for the same place again needs no new request.
pub struct EncryptedFileSource {
    file: Arc<EncryptedFile>,
    content_type: String,
    windows: Arc<Mutex<VecDeque<Window>>>,
}

impl EncryptedFileSource {
    pub fn new(file: EncryptedFile, content_type: impl Into<String>) -> Self {
        Self {
            file: Arc::new(file),
            content_type: content_type.into(),
            windows: Arc::default(),
        }
    }

    /// The plaintext bytes held in memory now.
    pub fn kept(&self) -> usize {
        kept(&self.windows).iter().map(|w| w.bytes.len()).sum()
    }

    /// Plaintext from `at` up to `end` out of one window: a kept one, or the blocks from `at` on,
    /// opened through one request and then kept.
    async fn piece(
        file: &EncryptedFile,
        windows: &Mutex<VecDeque<Window>>,
        at: u64,
        end: u64,
    ) -> Result<Bytes, FilesError> {
        if let Some(piece) = cut(&mut kept(windows), at, end) {
            return Ok(piece);
        }
        let first = at - at % CHUNK as u64;
        let until = end
            .div_ceil(CHUNK as u64)
            .saturating_mul(CHUNK as u64)
            .min(first + WINDOW)
            .min(file.size());
        let window = Window {
            start: first,
            bytes: Bytes::from(file.read(first, until).await?),
        };
        let mut windows = kept(windows);
        if windows.len() == KEPT {
            windows.pop_front();
        }
        windows.push_back(window);
        cut(&mut windows, at, end).ok_or_else(|| FilesError::invalid_path("outside the file"))
    }
}

fn kept(windows: &Mutex<VecDeque<Window>>) -> MutexGuard<'_, VecDeque<Window>> {
    windows.lock().unwrap_or_else(PoisonError::into_inner)
}

/// `[at, end)` out of the kept window that holds `at`, cut at its end; that window becomes the
/// newest.
fn cut(windows: &mut VecDeque<Window>, at: u64, end: u64) -> Option<Bytes> {
    let index = windows.iter().position(|w| w.start <= at && at < w.end())?;
    let window = windows.remove(index)?;
    let from = usize::try_from(at - window.start).ok()?;
    let to = usize::try_from(end.min(window.end()) - window.start).ok()?;
    let piece = window.bytes.slice(from..to);
    windows.push_back(window);
    Some(piece)
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
        let end = start.saturating_add(len).min(self.file.size());
        let state = (Arc::clone(&self.file), Arc::clone(&self.windows), start);
        let pieces = futures::stream::unfold(state, move |(file, windows, at)| async move {
            if at >= end {
                return None;
            }
            match Self::piece(&file, &windows, at, end).await {
                Ok(piece) => {
                    let next = at + piece.len() as u64;
                    Some((Ok(piece), (file, windows, next)))
                }
                // A failed window ends the stream: nothing of it, nothing after it.
                Err(error) => Some((
                    Err(std::io::Error::other(error.message)),
                    (file, windows, end),
                )),
            }
        });
        Ok(Box::new(tokio_util::io::StreamReader::new(Box::pin(
            pieces,
        ))))
    }

    fn content_type(&self) -> &str {
        &self.content_type
    }
}

#[cfg(test)]
#[path = "source_tests.rs"]
mod tests;
