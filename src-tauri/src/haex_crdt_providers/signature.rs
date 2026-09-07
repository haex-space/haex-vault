//! `haex-crdt` [`SignatureProvider`] adapter.
//!
//! # Trait shape vs. haex-vault reality (retro-worthy friction)
//!
//! `haex-crdt` defines `sign_column(preimage: &[u8]) -> Result<Vec<u8>>` and
//! `verify_column(preimage: &[u8], sig: &JsonValue) -> Result<()>`. The
//! crate builds the preimage from just five fields
//! (`table_name`, `row_pks`, `column_name`, `hlc_timestamp`, `value`) —
//! see `haex_crdt::crdt::apply::preimage::column_sig_preimage`.
//!
//! haex-vault's existing signing path
//! (`crate::crdt::column_sig::preimage::build_preimage`) adds a
//! domain-separation tag, the `space_id`, and the `author_did` to the
//! preimage, and looks up the signing key in a per-space
//! [`crate::crdt::column_sig::key_cache::SpaceKeyCache`]. The trait as
//! shipped does not surface any of that context to the provider — the
//! provider sees the concatenated bytes only.
//!
//! For Batch 1 we only prove the trait shape is usable. The `for_test`
//! constructor holds a single Ed25519 keypair and round-trips signatures
//! over whatever preimage the crate hands it. Batch 5 will bridge the two
//! preimage formats (either by widening the trait, or by pre-hashing the
//! haex-vault domain fields into the crate's preimage before the sign
//! call).

use ed25519_dalek::{Signature, Signer, SigningKey, Verifier, VerifyingKey};
use haex_crdt::error::{Error as CrdtError, Result as CrdtResult};
use haex_crdt::{AuthorId, SignatureProvider};
use serde_json::Value as JsonValue;

pub struct HaexVaultSignatureProvider {
    signing_key: SigningKey,
    author: AuthorId,
}

impl HaexVaultSignatureProvider {
    /// General constructor: takes an already-materialised signing key and
    /// the author identity that scans/wire records for this provider's
    /// own writes. Wire the per-space variant into `AppState` in Batch 5.
    pub fn from_signing_key(signing_key: SigningKey, author: AuthorId) -> Self {
        Self {
            signing_key,
            author,
        }
    }

    /// Test-only constructor. Fresh random key + placeholder author DID —
    /// enough to round-trip a `sign_column` → `verify_column` pair without
    /// dragging in the SpaceKeyCache / MLS / UCAN machinery.
    ///
    /// Uses `rand::random` per repo convention (literal test seeds are
    /// flagged by CodeQL as hardcoded credentials — see `CLAUDE.md`).
    #[cfg(test)]
    pub fn for_test() -> Self {
        let signing_key = SigningKey::from_bytes(&rand::random::<[u8; 32]>());
        Self {
            signing_key,
            author: AuthorId("did:test:haex-vault-batch-1".to_string()),
        }
    }

    fn verifying_key(&self) -> VerifyingKey {
        self.signing_key.verifying_key()
    }
}

impl SignatureProvider for HaexVaultSignatureProvider {
    fn sign_column(&self, preimage: &[u8]) -> CrdtResult<Vec<u8>> {
        let sig = self.signing_key.sign(preimage);
        Ok(sig.to_bytes().to_vec())
    }

    fn verify_column(&self, preimage: &[u8], sig: &JsonValue) -> CrdtResult<()> {
        // Wire shape: `{ "bytes": "<hex-encoded 64-byte ed25519 signature>" }`.
        // Batch 5 replaces this with the vault's existing UCAN/DID-carrying
        // signature envelope; the current shape is only rich enough to
        // exercise the round-trip in tests.
        let hex_str = sig
            .as_object()
            .and_then(|m| m.get("bytes"))
            .and_then(|v| v.as_str())
            .ok_or_else(|| {
                CrdtError::Message("signature JSON missing string `bytes` field".to_string())
            })?;
        let sig_bytes = hex::decode(hex_str)
            .map_err(|e| CrdtError::Message(format!("signature hex decode: {e}")))?;
        let signature = Signature::from_slice(&sig_bytes)
            .map_err(|e| CrdtError::Message(format!("signature bytes malformed: {e}")))?;
        self.verifying_key()
            .verify(preimage, &signature)
            .map_err(|e| CrdtError::Message(format!("signature verify failed: {e}")))
    }

    fn author_id(&self) -> AuthorId {
        self.author.clone()
    }
}
