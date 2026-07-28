use super::limits::MAX_VALUE_BYTES_LEN;
use super::preimage::build_preimage;
use crate::ucan::verify::public_key_from_did;
use ed25519_dalek::Signature;

#[derive(Debug, thiserror::Error)]
pub enum VerifyColumnSigError {
    #[error("malformed did: {0}")]
    MalformedDid(String),
    #[error("invalid signature")]
    InvalidSignature,
    #[error("signature bytes malformed")]
    MalformedSignatureBytes,
    #[error("value_bytes too large: {actual} > {max}")]
    ValueBytesTooLarge { actual: usize, max: usize },
}

#[allow(clippy::too_many_arguments)]
pub fn verify_column_sig(
    space_id: &[u8],
    table_name: &[u8],
    row_pks: &[u8],
    column_name: &[u8],
    hlc: &[u8],
    author_did: &str,
    value_bytes: &[u8],
    sig_bytes: &[u8],
) -> Result<(), VerifyColumnSigError> {
    if value_bytes.len() > MAX_VALUE_BYTES_LEN {
        return Err(VerifyColumnSigError::ValueBytesTooLarge {
            actual: value_bytes.len(),
            max: MAX_VALUE_BYTES_LEN,
        });
    }
    let verifying = public_key_from_did(author_did)
        .map_err(|e| VerifyColumnSigError::MalformedDid(e.to_string()))?;
    let sig = Signature::from_slice(sig_bytes)
        .map_err(|_| VerifyColumnSigError::MalformedSignatureBytes)?;
    let preimage = build_preimage(
        space_id,
        table_name,
        row_pks,
        column_name,
        hlc,
        author_did.as_bytes(),
        value_bytes,
    );
    // `verify_strict`, not `verify`: the permissive RFC-8032 path accepts
    // small-order (weak) public keys and small-order signature R components,
    // which allow forging a signature that validates against almost any
    // message. This signature IS the authorship binding for shared-space row
    // changes, so a weak-key forgery is a direct impersonation. Strict
    // verification rejects both cases.
    verifying
        .verify_strict(&preimage, &sig)
        .map_err(|_| VerifyColumnSigError::InvalidSignature)?;
    Ok(())
}
