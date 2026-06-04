//! Android Content URI streaming source.
//!
//! Wraps a `tauri_plugin_android_fs` file descriptor so the local media
//! server can serve byte ranges from a SAF-backed file (Storage Access
//! Framework). Reads never materialise the full file in RAM — each
//! `read_range` opens the fd inside a `spawn_blocking` thread, seeks to
//! the start offset, and reads exactly `end - start + 1` bytes.
//!
//! Phase 2 of the Android media playback plan: replaces the `openSystem`
//! interim for inline playback of large local media.

use async_trait::async_trait;
use tauri::AppHandle;
use tokio::sync::Mutex;

use super::peer_source::content_type_from_path;
use super::source::{ByteRange, StreamingError, StreamingSource};

pub struct ContentUriStreamingSource {
    app_handle: AppHandle,
    /// Full JSON-encoded `FileUri` (as produced by
    /// `tauri_plugin_android_fs::FileUri::to_json_str`) — the resolved
    /// file, not a tree root + sub-path. The frontend already has this
    /// blob via `file.path` for Content URI shares.
    uri_json: String,
    /// File extension or display name carried alongside the URI so we
    /// can derive a MIME type without a JNI round-trip per request.
    name_hint: String,
    cached_size: Mutex<Option<u64>>,
}

impl ContentUriStreamingSource {
    pub fn new(app_handle: AppHandle, uri_json: String, name_hint: String) -> Self {
        Self {
            app_handle,
            uri_json,
            name_hint,
            cached_size: Mutex::new(None),
        }
    }
}

#[async_trait]
impl StreamingSource for ContentUriStreamingSource {
    async fn size(&self) -> Result<u64, StreamingError> {
        let mut cached = self.cached_size.lock().await;
        if let Some(n) = *cached {
            return Ok(n);
        }
        let app = self.app_handle.clone();
        let uri_json = self.uri_json.clone();
        let size = tokio::task::spawn_blocking(move || -> Result<u64, String> {
            use tauri_plugin_android_fs::{AndroidFsExt, FileUri};
            let api = app.android_fs();
            let uri = FileUri::from_json_str(&uri_json)
                .map_err(|e| format!("invalid Content URI: {e:?}"))?;
            api.get_len(&uri)
                .map_err(|e| format!("get_len failed: {e:?}"))
        })
        .await
        .map_err(|e| StreamingError::Backend(format!("size task failed: {e}")))?
        .map_err(StreamingError::Backend)?;
        *cached = Some(size);
        Ok(size)
    }

    async fn read_range(&self, range: ByteRange) -> Result<Vec<u8>, StreamingError> {
        let start = range.start();
        let end = range.end();
        let want = end - start + 1;

        let app = self.app_handle.clone();
        let uri_json = self.uri_json.clone();

        tokio::task::spawn_blocking(move || -> Result<Vec<u8>, StreamingError> {
            use std::io::{Read, Seek, SeekFrom};
            use tauri_plugin_android_fs::{AndroidFsExt, FileUri};

            let api = app.android_fs();
            let uri = FileUri::from_json_str(&uri_json)
                .map_err(|e| StreamingError::BadRequest(format!("invalid Content URI: {e:?}")))?;

            let mut file = api
                .open_file_readable(&uri)
                .map_err(|e| StreamingError::Backend(format!("open_file_readable: {e:?}")))?;

            if start > 0 {
                file.seek(SeekFrom::Start(start))
                    .map_err(|e| StreamingError::Backend(format!("seek: {e}")))?;
            }

            let want_usize = usize::try_from(want)
                .map_err(|_| StreamingError::BadRequest("range too large".into()))?;
            let mut buf = vec![0u8; want_usize];
            let mut filled = 0usize;
            while filled < want_usize {
                match file.read(&mut buf[filled..]) {
                    Ok(0) => break,
                    Ok(n) => filled += n,
                    Err(e) => return Err(StreamingError::Backend(format!("read: {e}"))),
                }
            }
            buf.truncate(filled);
            Ok(buf)
        })
        .await
        .map_err(|e| StreamingError::Backend(format!("read task failed: {e}")))?
    }

    async fn content_type(&self) -> Option<String> {
        content_type_from_path(&self.name_hint)
    }
}
