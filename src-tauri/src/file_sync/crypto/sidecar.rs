//! Round C — metadata sidecar: the plaintext record that lets a member
//! reconstruct `relative_path -> object_key` from a bucket listing without
//! downloading file content.
//!
//! A sidecar is sealed the same way as file content: an [`EnvelopeHeader`]
//! followed by one or more AEAD chunks (see [`crate::file_sync::crypto::chunk`]).
//! Sidecars are always small (a JSON record, never streamed), so — unlike the
//! eventual file-content path (Round D) — sealing/opening one can safely
//! build the whole plaintext/ciphertext in memory in one call.
//!
//! ## Deterministic serialization
//!
//! [`SidecarPayload`] is sealed under AEAD, so its plaintext bytes must be
//! reproducible for the same field values — serde_json's struct serializer
//! writes fields in declaration order (unlike a `HashMap`), which is enough:
//! no explicit canonicalization step is needed here.

use serde::{Deserialize, Serialize};

use super::chunk::{self, CryptoError, CHUNK_CIPHERTEXT_SIZE, CHUNK_PLAINTEXT_SIZE};
use super::envelope::{EnvelopeHeader, HEADER_SIZE, NONCE_SIZE};

/// Plaintext sidecar record for one synced file.
///
/// `size` and `modified_at` describe the **plaintext** file, in the same
/// units the rest of `file_sync` uses (bytes; Unix seconds) — see
/// `crate::file_sync::types::FileState` and
/// `crate::file_sync::engine::state::SyncStateEntry`. Keeping units aligned
/// here matters: Round D's diff engine compares these against locally
/// scanned values, and a unit mismatch would silently make every file look
/// "changed" forever (see the plan's "warum der Diff nicht so bricht"
/// pitfall, which is exactly this failure shape for a different field).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SidecarPayload {
    /// Opaque object key that holds the encrypted content bytes — e.g.
    /// `content/o/<hex32>`. Sidecar readers use it to construct the GET
    /// path after unwrapping the DEK.
    pub content_key: String,
    /// AEAD-sealed 32-byte DEK. The seal-key is the grant's KEK
    /// (own-vault vault_key for `own/*` sidecars, MLS epoch key for
    /// `space-<id>/*` sidecars). See `crypto::dek_wrap`.
    pub wrapped_dek: Vec<u8>,
    /// Epoch of the KEK that wraps `wrapped_dek`. `None` on own-vault
    /// sidecars (the vault key has no rotation concept — see
    /// `provider::VAULT_KEY_EPOCH`); `Some(epoch)` on space-scoped
    /// sidecars so the reader knows which MLS epoch key to unwrap under.
    ///
    /// Split from the enclosing envelope's epoch on purpose: after a
    /// Round F5 revocation-driven rewrap the envelope carries the new
    /// epoch (the sidecar bytes are resealed) while the DEK stays the
    /// same and `wrapped_dek_epoch` rotates alongside. Keeping the two
    /// fields distinct lets a rewrap change one without touching the
    /// other's on-wire meaning.
    ///
    /// `Option` for on-wire backward-compat between own-vault and
    /// space-scoped payloads: pre-F3a `own/*.m` sidecars serialised
    /// without the field and still deserialise to `None`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub wrapped_dek_epoch: Option<u64>,
    pub relative_path: String,
    pub size: u64,
    pub modified_at: u64,
    pub content_type: Option<String>,
    /// BLAKE3 of the plaintext file, lowercase hex.
    pub blake3: String,
}

/// Errors from sealing/opening a sidecar.
#[derive(Debug, thiserror::Error)]
pub enum SidecarError {
    #[error(transparent)]
    Crypto(#[from] CryptoError),
    #[error("sidecar payload JSON error: {0}")]
    Json(#[from] serde_json::Error),
}

/// Seal `payload` into an envelope under `key`/`epoch`, using the
/// caller-supplied `file_nonce`. Pure — no RNG, no I/O — so the caller
/// (Round C's bootstrap / Round D's write path) owns randomness generation
/// and this stays trivially testable.
pub fn seal_sidecar(
    key: &[u8; 32],
    epoch: u64,
    file_nonce: [u8; NONCE_SIZE],
    payload: &SidecarPayload,
) -> Result<Vec<u8>, SidecarError> {
    let plaintext = serde_json::to_vec(payload)?;
    let header = EnvelopeHeader::new(epoch, file_nonce);
    let mut out = header.to_bytes().to_vec();
    for (i, pt_chunk) in plaintext.chunks(CHUNK_PLAINTEXT_SIZE).enumerate() {
        out.extend_from_slice(&chunk::seal_chunk(key, &file_nonce, i as u64, pt_chunk)?);
    }
    Ok(out)
}

/// Parse the envelope header and open every chunk of `ciphertext` under
/// `key`, then deserialize the recovered plaintext as a [`SidecarPayload`].
/// Returns the parsed header too — callers (bootstrap) need `header.epoch`
/// to have resolved `key` in the first place, but returning it again here
/// saves them from threading it through separately.
pub fn open_sidecar(
    key: &[u8; 32],
    ciphertext: &[u8],
) -> Result<(EnvelopeHeader, SidecarPayload), SidecarError> {
    let header = EnvelopeHeader::parse(ciphertext)?;
    let body = &ciphertext[HEADER_SIZE..];
    let mut plaintext = Vec::new();
    for (i, ct_chunk) in body.chunks(CHUNK_CIPHERTEXT_SIZE).enumerate() {
        plaintext.extend_from_slice(&chunk::open_chunk(
            key,
            &header.file_nonce,
            i as u64,
            ct_chunk,
        )?);
    }
    let payload: SidecarPayload = serde_json::from_slice(&plaintext)?;
    Ok((header, payload))
}
