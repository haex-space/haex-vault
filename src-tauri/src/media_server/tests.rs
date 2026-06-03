use super::*;
use async_trait::async_trait;
use std::sync::Arc;

/// A unique temp dir per call so the `tokio::test` cases (run in parallel by
/// the test harness) never share a path and race on create/remove.
fn unique_test_dir(prefix: &str) -> std::path::PathBuf {
    std::env::temp_dir().join(format!("{prefix}-{}", uuid::Uuid::new_v4()))
}

struct DummySource {
    data: Vec<u8>,
}

#[async_trait]
impl crate::remote_storage::streaming::source::StreamingSource for DummySource {
    async fn size(
        &self,
    ) -> Result<u64, crate::remote_storage::streaming::source::StreamingError> {
        Ok(self.data.len() as u64)
    }
    async fn read_range(
        &self,
        range: crate::remote_storage::streaming::source::ByteRange,
    ) -> Result<Vec<u8>, crate::remote_storage::streaming::source::StreamingError> {
        Ok(self.data[range.start() as usize..=range.end() as usize].to_vec())
    }
}

#[tokio::test]
async fn serves_range_from_streaming_source() {
    let server = MediaServer::start().await.unwrap();
    let source = Arc::new(DummySource {
        data: (0u8..=200).collect(),
    });
    let url = server
        .register_source(source, Some("application/octet-stream".into()))
        .await;

    let client = reqwest::Client::builder().no_proxy().build().unwrap();
    let resp = client
        .get(&url)
        .header("Range", "bytes=10-19")
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status().as_u16(), 206);
    assert_eq!(
        resp.headers().get("Content-Range").unwrap(),
        "bytes 10-19/201",
    );
    let body = resp.bytes().await.unwrap();
    assert_eq!(body.as_ref(), &(10u8..=19).collect::<Vec<u8>>()[..]);
}

#[tokio::test]
async fn serves_full_body_from_streaming_source_without_range_header() {
    let server = MediaServer::start().await.unwrap();
    let source = Arc::new(DummySource {
        data: vec![0xAB; 256],
    });
    let url = server.register_source(source, None).await;

    let client = reqwest::Client::builder().no_proxy().build().unwrap();
    let resp = client.get(&url).send().await.unwrap();
    assert_eq!(resp.status().as_u16(), 200);
    assert_eq!(
        resp.headers().get("Content-Type").unwrap(),
        "application/octet-stream",
    );
    let body = resp.bytes().await.unwrap();
    assert_eq!(body.len(), 256);
    assert!(body.iter().all(|b| *b == 0xAB));
}

/// A `bytes=0-` request against a multi-MiB stream source must NOT
/// allocate the entire object — the server caps any single response
/// at 8 MiB and returns 206 with a partial Content-Range so the
/// browser can pull the remainder in subsequent requests.
#[tokio::test]
async fn caps_open_ended_range_at_8_mib_for_stream_source() {
    const TOTAL: usize = 10 * 1024 * 1024; // 10 MiB
    let server = MediaServer::start().await.unwrap();
    let source = Arc::new(DummySource {
        data: vec![0u8; TOTAL],
    });
    let url = server.register_source(source, None).await;

    let client = reqwest::Client::builder().no_proxy().build().unwrap();
    let resp = client
        .get(&url)
        .header("Range", "bytes=0-")
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status().as_u16(), 206);
    assert_eq!(
        resp.headers().get("Content-Range").unwrap(),
        format!("bytes 0-{}/{}", 8 * 1024 * 1024 - 1, TOTAL).as_str(),
    );
    let body = resp.bytes().await.unwrap();
    assert_eq!(body.len(), 8 * 1024 * 1024);
}

/// `StreamingError::NotFound` from `size()` (i.e. before any
/// response headers have been written) must surface as HTTP 404,
/// not 500. Once we are past `size()` the wire is committed and we
/// can only drop the connection.
#[tokio::test]
async fn size_returning_not_found_yields_http_404() {
    struct NotFoundSource;
    #[async_trait]
    impl crate::remote_storage::streaming::source::StreamingSource for NotFoundSource {
        async fn size(
            &self,
        ) -> Result<u64, crate::remote_storage::streaming::source::StreamingError> {
            Err(
                crate::remote_storage::streaming::source::StreamingError::NotFound(
                    "missing.mp4".into(),
                ),
            )
        }
        async fn read_range(
            &self,
            _: crate::remote_storage::streaming::source::ByteRange,
        ) -> Result<
            Vec<u8>,
            crate::remote_storage::streaming::source::StreamingError,
        > {
            unreachable!("size() returns first")
        }
    }
    let server = MediaServer::start().await.unwrap();
    let url = server
        .register_source(Arc::new(NotFoundSource), None)
        .await;
    let client = reqwest::Client::builder().no_proxy().build().unwrap();
    let resp = client.get(&url).send().await.unwrap();
    assert_eq!(resp.status().as_u16(), 404);
}

/// The local-file path that the file browser's local-share audio/video
/// playback routes through (`media_server_register` → `MediaSource::Local`).
/// WebKitGTK's GStreamer pipeline rejected the previous `asset://` URL —
/// this confirms the loopback server serves a real on-disk media file with
/// the Range support GStreamer needs: 206 + Content-Range + Accept-Ranges,
/// a seek to an arbitrary offset, and a no-Range full-body fall-back.
#[tokio::test]
async fn serves_local_media_file_with_range_and_seek() {
    let dir = unique_test_dir("haex-media-server-range");
    tokio::fs::create_dir_all(&dir).await.unwrap();
    // Byte N has value N so range slices are trivially checkable.
    let path = dir.join("clip.mp4");
    let data: Vec<u8> = (0u8..100).collect();
    tokio::fs::write(&path, &data).await.unwrap();

    let server = MediaServer::start().await.unwrap();
    let url = server.register(path.clone()).await;
    let client = reqwest::Client::builder().no_proxy().build().unwrap();

    // Opening probe: partial range from the start.
    let resp = client
        .get(&url)
        .header("Range", "bytes=0-9")
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status().as_u16(), 206);
    assert_eq!(resp.headers().get("Content-Range").unwrap(), "bytes 0-9/100");
    assert_eq!(resp.headers().get("Accept-Ranges").unwrap(), "bytes");
    assert_eq!(resp.headers().get("Content-Type").unwrap(), "video/mp4");
    assert_eq!(
        resp.bytes().await.unwrap().as_ref(),
        &(0u8..=9).collect::<Vec<u8>>()[..],
    );

    // Seek to an arbitrary offset (what video scrubbing / moov-atom probing
    // needs — the failure mode that made MP4 unplayable without Range).
    let resp = client
        .get(&url)
        .header("Range", "bytes=50-59")
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status().as_u16(), 206);
    assert_eq!(resp.headers().get("Content-Range").unwrap(), "bytes 50-59/100");
    assert_eq!(
        resp.bytes().await.unwrap().as_ref(),
        &(50u8..=59).collect::<Vec<u8>>()[..],
    );

    // No Range header → full 200 body.
    let resp = client.get(&url).send().await.unwrap();
    assert_eq!(resp.status().as_u16(), 200);
    assert_eq!(resp.bytes().await.unwrap().len(), 100);

    tokio::fs::remove_dir_all(&dir).await.ok();
}

/// Re-registering the same local path reuses the existing token instead of
/// growing the registry unbounded across repeated plays of one file.
#[tokio::test]
async fn register_dedupes_same_local_path() {
    let dir = unique_test_dir("haex-media-server-dedupe");
    tokio::fs::create_dir_all(&dir).await.unwrap();
    let path = dir.join("dedupe.mp3");
    tokio::fs::write(&path, b"id3").await.unwrap();

    let server = MediaServer::start().await.unwrap();
    let url_a = server.register(path.clone()).await;
    let url_b = server.register(path.clone()).await;
    assert_eq!(url_a, url_b);

    tokio::fs::remove_dir_all(&dir).await.ok();
}
