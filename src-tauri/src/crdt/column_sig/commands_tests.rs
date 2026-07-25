use base64::{engine::general_purpose::STANDARD as BASE64, Engine as _};
use ed25519_dalek::SigningKey;

use super::commands::{verify_column_sig_batch_inner, ColumnSigChange, VerifyColumnSigBatchInput};
use super::sign::sign_column;
use crate::crdt::commands::apply::ColumnSig;
use crate::ucan::verify::did_key_from_public_key;

fn make_key() -> SigningKey {
    let seed: [u8; 32] = rand::random();
    SigningKey::from_bytes(&seed)
}

#[allow(clippy::too_many_arguments)]
fn build_change(
    key: &SigningKey,
    space_id: &str,
    table_name: &str,
    row_pks: &str,
    column_name: &str,
    hlc: &str,
    value_bytes: &[u8],
    corrupt_sig: bool,
) -> ColumnSigChange {
    let did = did_key_from_public_key(&key.verifying_key());
    let sig = sign_column(
        key,
        space_id.as_bytes(),
        table_name.as_bytes(),
        row_pks.as_bytes(),
        column_name.as_bytes(),
        hlc.as_bytes(),
        did.as_bytes(),
        value_bytes,
    );
    let sig_b64 = if corrupt_sig {
        // Bit-flip every byte: still exactly 64 bytes so it decodes as
        // a Signature (no MalformedSignatureBytes), but the actual
        // Ed25519 check fails as InvalidSignature — the failure mode
        // the batch verifier is meant to catch.
        let mut bytes = sig.to_bytes();
        for b in bytes.iter_mut() {
            *b ^= 0xFF;
        }
        BASE64.encode(bytes)
    } else {
        BASE64.encode(sig.to_bytes())
    };
    ColumnSigChange {
        table_name: table_name.to_string(),
        row_pks: row_pks.to_string(),
        column_name: column_name.to_string(),
        hlc_timestamp: hlc.to_string(),
        value_bytes: BASE64.encode(value_bytes),
        sig: ColumnSig {
            author_did: did,
            sig: sig_b64,
        },
    }
}

#[test]
fn verify_batch_returns_verified_and_rejected_split() {
    let key = make_key();
    let space_id = "space_A";

    let valid = build_change(
        &key,
        space_id,
        "ext_calendar",
        r#"{"id":"R1"}"#,
        "title",
        "hlc-1",
        b"Hi",
        false,
    );
    let tampered = build_change(
        &key,
        space_id,
        "ext_calendar",
        r#"{"id":"R2"}"#,
        "title",
        "hlc-2",
        b"Hi",
        true,
    );

    let valid_key = format!(
        "{}|{}|{}|{}",
        valid.table_name, valid.row_pks, valid.column_name, valid.hlc_timestamp
    );
    let tampered_key = format!(
        "{}|{}|{}|{}",
        tampered.table_name, tampered.row_pks, tampered.column_name, tampered.hlc_timestamp
    );

    let out = verify_column_sig_batch_inner(VerifyColumnSigBatchInput {
        changes: vec![valid, tampered],
        expected_space_id: space_id.to_string(),
    });

    assert_eq!(out.verified, vec![valid_key]);
    assert_eq!(out.rejected.len(), 1);
    assert_eq!(out.rejected[0].row_key, tampered_key);
    assert_eq!(out.rejected[0].reason, "InvalidSignature");
}
