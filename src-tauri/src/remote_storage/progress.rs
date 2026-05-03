//! Progress-tracking wrappers around `AsyncRead`/`AsyncWrite`.
//!
//! Used by streaming uploads/downloads so the file-sync engine can show
//! real bytes/sec and percentage UI without buffering the whole file.

use std::pin::Pin;
use std::sync::Arc;
use std::task::{Context, Poll};
use std::time::{Duration, Instant};

use tokio::io::{AsyncRead, AsyncWrite, ReadBuf};

pub type ProgressCallback = Arc<dyn Fn(u64, u64) + Send + Sync>;

/// Minimum interval between progress callbacks.
/// Without throttling, small chunks (e.g. 16 KiB) would fire thousands
/// of callbacks per second on a multi-GB transfer.
const EMIT_INTERVAL: Duration = Duration::from_millis(100);

/// `AsyncRead` adapter that counts bytes and reports progress.
pub struct ProgressReader<R> {
    inner: R,
    bytes_read: u64,
    total: u64,
    cb: Option<ProgressCallback>,
    last_emit: Instant,
}

impl<R> ProgressReader<R> {
    pub fn new(inner: R, total: u64, cb: Option<ProgressCallback>) -> Self {
        Self {
            inner,
            bytes_read: 0,
            total,
            cb,
            last_emit: Instant::now() - EMIT_INTERVAL,
        }
    }

    pub fn bytes_read(&self) -> u64 {
        self.bytes_read
    }
}

impl<R: AsyncRead + Unpin> AsyncRead for ProgressReader<R> {
    fn poll_read(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut ReadBuf<'_>,
    ) -> Poll<std::io::Result<()>> {
        let before = buf.filled().len();
        let inner = Pin::new(&mut self.inner);
        match inner.poll_read(cx, buf) {
            Poll::Ready(Ok(())) => {
                let after = buf.filled().len();
                let delta = (after - before) as u64;
                if delta > 0 {
                    self.bytes_read += delta;
                    let now = Instant::now();
                    let reached_total = self.total > 0 && self.bytes_read >= self.total;
                    if let Some(cb) = self.cb.clone() {
                        if reached_total
                            || now.duration_since(self.last_emit) >= EMIT_INTERVAL
                        {
                            self.last_emit = now;
                            cb(self.bytes_read, self.total.max(self.bytes_read));
                        }
                    }
                }
                Poll::Ready(Ok(()))
            }
            other => other,
        }
    }
}

/// `AsyncWrite` adapter that counts bytes and reports progress.
pub struct ProgressWriter<W> {
    inner: W,
    bytes_written: u64,
    total: u64,
    cb: Option<ProgressCallback>,
    last_emit: Instant,
}

impl<W> ProgressWriter<W> {
    pub fn new(inner: W, total: u64, cb: Option<ProgressCallback>) -> Self {
        Self {
            inner,
            bytes_written: 0,
            total,
            cb,
            last_emit: Instant::now() - EMIT_INTERVAL,
        }
    }

    pub fn bytes_written(&self) -> u64 {
        self.bytes_written
    }

    fn record(&mut self, delta: u64) {
        if delta == 0 {
            return;
        }
        self.bytes_written += delta;
        let now = Instant::now();
        if let Some(cb) = self.cb.clone() {
            if now.duration_since(self.last_emit) >= EMIT_INTERVAL {
                self.last_emit = now;
                cb(self.bytes_written, self.total.max(self.bytes_written));
            }
        }
    }
}

impl<W: AsyncWrite + Unpin> AsyncWrite for ProgressWriter<W> {
    fn poll_write(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &[u8],
    ) -> Poll<std::io::Result<usize>> {
        let inner = Pin::new(&mut self.inner);
        match inner.poll_write(cx, buf) {
            Poll::Ready(Ok(n)) => {
                self.record(n as u64);
                Poll::Ready(Ok(n))
            }
            other => other,
        }
    }

    fn poll_flush(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
    ) -> Poll<std::io::Result<()>> {
        Pin::new(&mut self.inner).poll_flush(cx)
    }

    fn poll_shutdown(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
    ) -> Poll<std::io::Result<()>> {
        if let Some(cb) = self.cb.clone() {
            let total = self.total.max(self.bytes_written);
            cb(self.bytes_written, total);
        }
        Pin::new(&mut self.inner).poll_shutdown(cx)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU64, Ordering};
    use std::sync::Mutex;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    #[allow(clippy::type_complexity)]
    fn counter_cb() -> (Arc<AtomicU64>, Arc<Mutex<Vec<(u64, u64)>>>, ProgressCallback) {
        let calls = Arc::new(AtomicU64::new(0));
        let samples = Arc::new(Mutex::new(Vec::new()));
        let calls_c = calls.clone();
        let samples_c = samples.clone();
        let cb: ProgressCallback = Arc::new(move |done, total| {
            calls_c.fetch_add(1, Ordering::Relaxed);
            samples_c.lock().unwrap().push((done, total));
        });
        (calls, samples, cb)
    }

    #[tokio::test]
    async fn reader_passes_bytes_through() {
        let data = vec![7u8; 8192];
        let (_calls, _samples, cb) = counter_cb();
        let mut reader = ProgressReader::new(&data[..], data.len() as u64, Some(cb));
        let mut out = Vec::new();
        reader.read_to_end(&mut out).await.unwrap();
        assert_eq!(out, data);
        assert_eq!(reader.bytes_read(), data.len() as u64);
    }

    #[tokio::test]
    async fn reader_throttles_callbacks() {
        // 1 MiB read in tiny chunks should emit far fewer callbacks than chunks.
        let data = vec![0u8; 1024 * 1024];
        let (calls, _samples, cb) = counter_cb();
        let mut reader = ProgressReader::new(&data[..], data.len() as u64, Some(cb));
        let mut buf = [0u8; 64];
        loop {
            let n = reader.read(&mut buf).await.unwrap();
            if n == 0 {
                break;
            }
        }
        // 16384 reads, throttled to ≤ ~10/sec, so under any sane CI we should
        // see far fewer than `data.len() / 64`. Be generous to avoid flakes.
        let count = calls.load(Ordering::Relaxed);
        assert!(count < 1000, "expected throttled emits, got {count}");
    }

    #[tokio::test]
    async fn writer_counts_and_emits_on_shutdown() {
        let (_calls, samples, cb) = counter_cb();
        let buf: Vec<u8> = Vec::new();
        let mut writer = ProgressWriter::new(buf, 4, Some(cb));
        writer.write_all(&[1, 2, 3, 4]).await.unwrap();
        writer.shutdown().await.unwrap();
        assert_eq!(writer.bytes_written(), 4);
        let samples = samples.lock().unwrap();
        // Final emit happens on shutdown.
        let last = samples.last().expect("at least one progress emit");
        assert_eq!(*last, (4, 4));
    }
}
