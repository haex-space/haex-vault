//! Round D — file-content sealing/opening built on the envelope + chunk
//! primitives from Round A.
//!
//! The sidecar path in [`super::sidecar`] JSON-encodes a small metadata
//! record and buffers the whole thing in RAM; this module handles the
//! *content* path, where a plaintext file may be many gigabytes and
//! streaming is a hard requirement — the plan (`docs/plans/2026-08-25-…`)
//! explicitly forbids buffering the whole file in RAM.
//!
//! Two entry-point pairs:
//!
//! - [`seal_bytes`] / [`open_bytes`] — full-buffer, for small payloads and
//!   for the decorator's `read_file`/`write_file` (non-streaming) methods
//!   where the plaintext is already in a `Vec<u8>`.
//! - [`seal_stream`] / [`open_stream`] — chunkwise, reading from any
//!   `AsyncRead` and writing to any `AsyncWrite`. The decorator's
//!   `read_file_to_path` / `write_file_from_path` route through these so
//!   nothing above one plaintext chunk (`CHUNK_PLAINTEXT_SIZE = 1 MiB`) or
//!   one ciphertext chunk (`CHUNK_CIPHERTEXT_SIZE`) is ever resident.
//!
//! All four functions parse or emit the header defined in
//! [`super::envelope`]; open functions reject an unknown magic/version
//! rather than best-effort interpreting the bytes.

use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};

use super::chunk::{self, CryptoError, CHUNK_CIPHERTEXT_SIZE, CHUNK_PLAINTEXT_SIZE, TAG_SIZE};
use super::envelope::{EnvelopeHeader, HEADER_SIZE, NONCE_SIZE};

/// Seal `plaintext` into an envelope + chunks under `key`/`epoch` with the
/// caller-supplied `file_nonce`. Full-buffer variant — allocates the whole
/// ciphertext at once. Use [`seal_stream`] for anything larger than a few
/// megabytes.
pub fn seal_bytes(
    key: &[u8; 32],
    epoch: u64,
    file_nonce: [u8; NONCE_SIZE],
    plaintext: &[u8],
) -> Result<Vec<u8>, CryptoError> {
    let header = EnvelopeHeader::new(epoch, file_nonce);
    let mut out = Vec::with_capacity(HEADER_SIZE + plaintext.len() + TAG_SIZE);
    out.extend_from_slice(&header.to_bytes());
    for (i, pt_chunk) in plaintext.chunks(CHUNK_PLAINTEXT_SIZE).enumerate() {
        out.extend_from_slice(&chunk::seal_chunk(key, &file_nonce, i as u64, pt_chunk)?);
    }
    Ok(out)
}

/// Parse the envelope header of `ciphertext`, open every chunk under
/// `key`, and return `(header, plaintext)`. Full-buffer variant — the
/// entire plaintext is materialised in memory.
pub fn open_bytes(
    key: &[u8; 32],
    ciphertext: &[u8],
) -> Result<(EnvelopeHeader, Vec<u8>), CryptoError> {
    let header = EnvelopeHeader::parse(ciphertext)?;
    let body = &ciphertext[HEADER_SIZE..];
    let mut plaintext = Vec::with_capacity(body.len().saturating_sub(TAG_SIZE));
    for (i, ct_chunk) in body.chunks(CHUNK_CIPHERTEXT_SIZE).enumerate() {
        plaintext.extend_from_slice(&chunk::open_chunk(
            key,
            &header.file_nonce,
            i as u64,
            ct_chunk,
        )?);
    }
    Ok((header, plaintext))
}

/// Streaming seal errors keep the underlying I/O error separate from the
/// AEAD/envelope error so the caller can tell "network dropped" from
/// "ciphertext corrupt" without string parsing.
#[derive(Debug, thiserror::Error)]
pub enum StreamCryptoError {
    #[error(transparent)]
    Crypto(#[from] CryptoError),
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
}

/// Stream-seal a plaintext source into a ciphertext sink. Reads exactly
/// `plaintext_len` plaintext bytes from `plaintext`, emits the envelope
/// header followed by one AEAD-sealed chunk per full/partial 1 MiB block.
///
/// `plaintext_len` is a hard cap — the function returns
/// [`std::io::ErrorKind::UnexpectedEof`] if the reader yields fewer bytes
/// and stops reading once it has consumed `plaintext_len` bytes. That way
/// a source file that grows mid-transfer does not silently truncate the
/// ciphertext, and a source that shrinks does not silently pad it.
pub async fn seal_stream<R, W>(
    key: &[u8; 32],
    epoch: u64,
    file_nonce: [u8; NONCE_SIZE],
    plaintext_len: u64,
    plaintext: &mut R,
    ciphertext: &mut W,
) -> Result<(), StreamCryptoError>
where
    R: AsyncRead + Unpin + ?Sized,
    W: AsyncWrite + Unpin + ?Sized,
{
    let header = EnvelopeHeader::new(epoch, file_nonce);
    ciphertext.write_all(&header.to_bytes()).await?;

    let mut buf = vec![0u8; CHUNK_PLAINTEXT_SIZE];
    let mut chunk_index: u64 = 0;
    let mut remaining = plaintext_len;
    while remaining > 0 {
        let want = std::cmp::min(remaining as usize, CHUNK_PLAINTEXT_SIZE);
        plaintext.read_exact(&mut buf[..want]).await?;
        let sealed = chunk::seal_chunk(key, &file_nonce, chunk_index, &buf[..want])?;
        ciphertext.write_all(&sealed).await?;
        chunk_index += 1;
        remaining -= want as u64;
    }
    ciphertext.flush().await?;
    Ok(())
}

/// Stream-open a ciphertext source into a plaintext sink. Reads exactly
/// `ciphertext_len` ciphertext bytes from `ciphertext`. Returns the parsed
/// envelope header on success — callers (e.g. the decorator's read path)
/// need `header.epoch` for logging but the AEAD key was already resolved
/// before this call.
///
/// One AEAD-sealed chunk at a time is buffered on the stack-of-heap:
/// `CHUNK_CIPHERTEXT_SIZE` bytes for the read side, and the plaintext
/// chunk is streamed straight to the writer without a second allocation
/// large enough to hold the whole file.
pub async fn open_stream<R, W>(
    key: &[u8; 32],
    ciphertext_len: u64,
    ciphertext: &mut R,
    plaintext: &mut W,
) -> Result<EnvelopeHeader, StreamCryptoError>
where
    R: AsyncRead + Unpin + ?Sized,
    W: AsyncWrite + Unpin + ?Sized,
{
    if ciphertext_len < HEADER_SIZE as u64 {
        return Err(StreamCryptoError::Crypto(CryptoError::MalformedCiphertext(
            format!("ciphertext {ciphertext_len} < header {HEADER_SIZE}"),
        )));
    }
    let mut header_buf = [0u8; HEADER_SIZE];
    ciphertext.read_exact(&mut header_buf).await?;
    let header = EnvelopeHeader::parse(&header_buf)?;

    let mut body_remaining = ciphertext_len - HEADER_SIZE as u64;
    let mut chunk_index: u64 = 0;
    let mut buf = vec![0u8; CHUNK_CIPHERTEXT_SIZE];
    while body_remaining > 0 {
        let want = std::cmp::min(body_remaining as usize, CHUNK_CIPHERTEXT_SIZE);
        if want <= TAG_SIZE {
            return Err(StreamCryptoError::Crypto(CryptoError::MalformedCiphertext(
                format!("body tail {want} <= tag size {TAG_SIZE}"),
            )));
        }
        ciphertext.read_exact(&mut buf[..want]).await?;
        let pt = chunk::open_chunk(key, &header.file_nonce, chunk_index, &buf[..want])?;
        plaintext.write_all(&pt).await?;
        chunk_index += 1;
        body_remaining -= want as u64;
    }
    plaintext.flush().await?;
    Ok(header)
}
