use super::preimage::build_preimage;
use ed25519_dalek::{Signature, Signer, SigningKey};

pub fn sign_column(
    signing_key: &SigningKey,
    space_id: &[u8],
    table_name: &[u8],
    row_pks: &[u8],
    column_name: &[u8],
    hlc: &[u8],
    author_did: &[u8],
    value_bytes: &[u8],
) -> Signature {
    let preimage = build_preimage(
        space_id,
        table_name,
        row_pks,
        column_name,
        hlc,
        author_did,
        value_bytes,
    );
    signing_key.sign(&preimage)
}
