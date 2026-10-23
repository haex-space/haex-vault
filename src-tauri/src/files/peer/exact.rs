//! The body of a read from another device: exactly as many bytes as the serving device announced.

use std::pin::Pin;
use std::task::{Context, Poll};

use tokio::io::{AsyncRead, ReadBuf};

/// A peer must deliver exactly the byte count it announced. Without this guard, a file that
/// shrinks after the server's metadata check is silently returned as a successful truncated read.
pub struct ExactBytes<R> {
    inner: R,
    remaining: u64,
}

impl<R> ExactBytes<R> {
    pub fn new(inner: R, expected: u64) -> Self {
        Self {
            inner,
            remaining: expected,
        }
    }
}

impl<R: AsyncRead + Unpin> AsyncRead for ExactBytes<R> {
    fn poll_read(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut ReadBuf<'_>,
    ) -> Poll<std::io::Result<()>> {
        if self.remaining == 0 {
            return Poll::Ready(Ok(()));
        }
        let before = buf.filled().len();
        match Pin::new(&mut self.inner).poll_read(cx, buf) {
            Poll::Pending => Poll::Pending,
            Poll::Ready(Ok(())) => {
                let read = buf.filled().len() - before;
                if read == 0 {
                    return Poll::Ready(Err(std::io::Error::new(
                        std::io::ErrorKind::UnexpectedEof,
                        "peer returned fewer bytes than announced",
                    )));
                }
                self.remaining = self.remaining.saturating_sub(read as u64);
                Poll::Ready(Ok(()))
            }
            Poll::Ready(Err(error)) => Poll::Ready(Err(error)),
        }
    }
}
