//! Phase 4 Round F3b — sealed [`ScopedCred`] payload for
//! `haex_s3_shared_access.encrypted_cred`.
//!
//! Reuses the envelope + chunk primitives from
//! [`crate::file_sync::crypto::content`] so the sharing feature never rolls
//! its own AEAD. A single-chunk payload — the JSON-encoded credential is a
//! few hundred bytes at most — is buffered, sealed under the current MLS
//! epoch key, and base64-encoded so the ciphertext survives as an opaque
//! TEXT column.
//!
//! Nonce discipline mirrors `sidecar` / `content` sealing at the caller
//! layer: this helper owns the per-call random `file_nonce` so the
//! sharing pipeline doesn't have to thread randomness through the row
//! writer. The envelope's `epoch` is redundant with the row's `epoch`
//! column (the receiver looks up the key by the row column, not by the
//! envelope header) but keeping it aligned with the row column keeps the
//! envelope self-describing and lets a future audit detect drift.

use base64::{engine::general_purpose::STANDARD as B64, Engine as _};
use serde::{Deserialize, Serialize};

use super::SharedAccessError;
use crate::file_sync::crypto::chunk::CryptoError;
use crate::file_sync::crypto::content::{open_bytes, seal_bytes};
use crate::file_sync::crypto::envelope::NONCE_SIZE;
use crate::remote_storage::iam_adapter::ScopedCred;

/// Private wire type — kept module-local so `ScopedCred` itself never
/// grows a public serde surface. That keeps every path that turns the
/// secret access key into bytes routed through this sealing chokepoint,
/// so a stray `serde_json::to_string(&cred)` or `#[serde]` derive on a
/// containing struct can't accidentally leak the plaintext.
#[derive(Serialize, Deserialize)]
struct ScopedCredWire {
    access_key_id: String,
    secret_access_key: String,
    iam_user_name: String,
}

impl From<&ScopedCred> for ScopedCredWire {
    fn from(c: &ScopedCred) -> Self {
        Self {
            access_key_id: c.access_key_id.clone(),
            secret_access_key: c.secret_access_key.clone(),
            iam_user_name: c.iam_user_name.clone(),
        }
    }
}

impl From<ScopedCredWire> for ScopedCred {
    fn from(w: ScopedCredWire) -> Self {
        Self {
            access_key_id: w.access_key_id,
            secret_access_key: w.secret_access_key,
            iam_user_name: w.iam_user_name,
        }
    }
}

/// Serialise `cred` to JSON, seal it into an envelope under `key`/`epoch`
/// with a fresh random per-call file nonce, and base64-encode the
/// ciphertext so it fits the TEXT `encrypted_cred` column of
/// `haex_s3_shared_access`.
pub fn seal_scoped_cred(
    cred: &ScopedCred,
    key: &[u8; 32],
    epoch: u64,
) -> Result<String, SharedAccessError> {
    let wire = ScopedCredWire::from(cred);
    let plaintext = serde_json::to_vec(&wire).map_err(|e| SharedAccessError::Codec {
        reason: format!("serialize ScopedCred: {e}"),
    })?;
    let mut file_nonce = [0u8; NONCE_SIZE];
    rand::fill(&mut file_nonce);
    let sealed = seal_bytes(key, epoch, file_nonce, &plaintext).map_err(map_crypto)?;
    Ok(B64.encode(sealed))
}

/// Inverse of [`seal_scoped_cred`]: base64-decode, open the envelope, and
/// deserialise the recovered plaintext back into a [`ScopedCred`]. The
/// envelope header is discarded — the caller already has the row's
/// `epoch` column and doesn't need the envelope epoch to look up the
/// key.
pub fn open_scoped_cred(sealed_b64: &str, key: &[u8; 32]) -> Result<ScopedCred, SharedAccessError> {
    let sealed = B64
        .decode(sealed_b64.as_bytes())
        .map_err(|e| SharedAccessError::Codec {
            reason: format!("base64 decode: {e}"),
        })?;
    let (_header, plaintext) = open_bytes(key, &sealed).map_err(map_crypto)?;
    let wire: ScopedCredWire =
        serde_json::from_slice(&plaintext).map_err(|e| SharedAccessError::Codec {
            reason: format!("deserialize ScopedCred: {e}"),
        })?;
    Ok(wire.into())
}

fn map_crypto(e: CryptoError) -> SharedAccessError {
    SharedAccessError::Crypto {
        reason: e.to_string(),
    }
}
