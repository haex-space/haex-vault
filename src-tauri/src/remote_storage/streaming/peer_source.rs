//! P2P (iroh) streaming source.
//!
//! Wraps `PeerEndpoint::remote_read_range_bytes` / `remote_stat` so the
//! generic streaming layer can pull byte ranges from a remote peer
//! without ever materialising the file on disk.
//!
//! Each `read_range` opens a fresh QUIC stream — iroh reuses the
//! underlying connection, so the cost per range is one short round trip.
//! A typical HTML `<video>` element fires 10–100 of these over the
//! lifetime of a playback.

use std::sync::Arc;

use async_trait::async_trait;
use iroh::{EndpointId, RelayUrl};
use tokio::sync::{Mutex, RwLock};

use super::source::{ByteRange, StreamingError, StreamingSource};
use crate::peer_storage::endpoint::PeerEndpoint;

pub struct PeerStreamingSource {
    endpoint: Arc<RwLock<PeerEndpoint>>,
    remote_id: EndpointId,
    relay_url: Option<RelayUrl>,
    path: String,
    ucan_token: String,
    // Size is fetched once on first call to `size()` and cached. Inner
    // Mutex (not RwLock) because the only mutation is the one-shot fill.
    cached_size: Mutex<Option<u64>>,
}

impl PeerStreamingSource {
    pub fn new(
        endpoint: Arc<RwLock<PeerEndpoint>>,
        remote_id: EndpointId,
        relay_url: Option<RelayUrl>,
        path: String,
        ucan_token: String,
    ) -> Self {
        Self {
            endpoint,
            remote_id,
            relay_url,
            path,
            ucan_token,
            cached_size: Mutex::new(None),
        }
    }
}

#[async_trait]
impl StreamingSource for PeerStreamingSource {
    async fn size(&self) -> Result<u64, StreamingError> {
        // Hold the Mutex across the RPC so two concurrent first-time
        // callers don't both issue a `remote_stat`. The Mutex is per
        // source, so other sources are unaffected.
        let mut cached = self.cached_size.lock().await;
        if let Some(n) = *cached {
            return Ok(n);
        }
        let entry = {
            let guard = self.endpoint.read().await;
            guard
                .remote_stat(
                    self.remote_id,
                    self.relay_url.clone(),
                    &self.path,
                    &self.ucan_token,
                )
                .await
        }
        .map_err(map_peer_error)?;
        *cached = Some(entry.size);
        Ok(entry.size)
    }

    async fn read_range(&self, range: ByteRange) -> Result<Vec<u8>, StreamingError> {
        let guard = self.endpoint.read().await;
        guard
            .remote_read_range_bytes(
                self.remote_id,
                self.relay_url.clone(),
                &self.path,
                [range.start(), range.end()],
                &self.ucan_token,
            )
            .await
            .map_err(map_peer_error)
    }

    async fn content_type(&self) -> Option<String> {
        content_type_from_path(&self.path)
    }
}

/// Map peer-layer errors into the trait's error type. "not found"-like
/// messages from the protocol layer land as `NotFound`; everything else
/// is `Backend`. Keep the substring matches narrow — the wire protocol
/// stringifies the server's path-resolution errors as English.
fn map_peer_error(err: crate::peer_storage::error::PeerStorageError) -> StreamingError {
    use crate::peer_storage::error::PeerStorageError;
    match &err {
        PeerStorageError::ProtocolError { reason }
            if reason.to_lowercase().contains("not found")
                || reason.to_lowercase().contains("no such file") =>
        {
            StreamingError::NotFound(reason.clone())
        }
        _ => StreamingError::Backend(format!("{err}")),
    }
}

/// MIME from file extension. Mirrors the table in the local
/// `media_server::mime_for` so both layers agree on what to send for the
/// same file. Case-insensitive on the extension.
pub(super) fn content_type_from_path(path: &str) -> Option<String> {
    let ext = path.rsplit('.').next()?.to_ascii_lowercase();
    let mime = match ext.as_str() {
        "mp3" => "audio/mpeg",
        "wav" => "audio/wav",
        "flac" => "audio/flac",
        "ogg" => "audio/ogg",
        "aac" => "audio/aac",
        "m4a" => "audio/mp4",
        "mp4" => "video/mp4",
        "mov" => "video/quicktime",
        "webm" => "video/webm",
        "ogv" => "video/ogg",
        "jpg" | "jpeg" => "image/jpeg",
        "png" => "image/png",
        "gif" => "image/gif",
        "webp" => "image/webp",
        "pdf" => "application/pdf",
        _ => "application/octet-stream",
    };
    Some(mime.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn content_type_falls_back_to_octet_stream_for_unknown_extension() {
        assert_eq!(
            content_type_from_path("noext").as_deref(),
            Some("application/octet-stream"),
        );
    }

    #[test]
    fn content_type_recognises_common_media_extensions() {
        assert_eq!(content_type_from_path("foo/bar.mp4").as_deref(), Some("video/mp4"));
        assert_eq!(content_type_from_path("song.flac").as_deref(), Some("audio/flac"));
        assert_eq!(content_type_from_path("clip.WEBM").as_deref(), Some("video/webm")); // case-insensitive
    }

    #[test]
    fn map_peer_error_classifies_path_lookup_failures_as_not_found() {
        use crate::peer_storage::error::PeerStorageError;
        let cases = [
            "Path not found",
            "File not found: /share/clip.mp4",
            "Share not found: media",
            "No such file or directory (os error 2)",
        ];
        for reason in cases {
            let mapped = map_peer_error(PeerStorageError::ProtocolError {
                reason: reason.to_string(),
            });
            assert!(
                matches!(mapped, StreamingError::NotFound(_)),
                "expected NotFound for {reason:?}, got {mapped:?}"
            );
        }
    }

    #[test]
    fn map_peer_error_falls_back_to_backend_for_other_failures() {
        use crate::peer_storage::error::PeerStorageError;
        let mapped = map_peer_error(PeerStorageError::ConnectionFailed {
            reason: "TLS handshake failed".to_string(),
        });
        assert!(matches!(mapped, StreamingError::Backend(_)));

        let mapped = map_peer_error(PeerStorageError::ProtocolError {
            reason: "unexpected response".to_string(),
        });
        assert!(matches!(mapped, StreamingError::Backend(_)));
    }
}
