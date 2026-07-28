use super::value_bytes::to_canonical_bytes;
use rusqlite::types::Value;

#[test]
fn null_serializes_to_empty() {
    assert_eq!(to_canonical_bytes(&Value::Null), Vec::<u8>::new());
}

#[test]
fn integer_serializes_as_be_i64() {
    assert_eq!(
        to_canonical_bytes(&Value::Integer(1)),
        vec![0, 0, 0, 0, 0, 0, 0, 1]
    );
    assert_eq!(to_canonical_bytes(&Value::Integer(-1)), vec![0xFF; 8]);
}

#[test]
fn real_serializes_as_be_ieee754_normalized() {
    // +0.0 == -0.0
    assert_eq!(
        to_canonical_bytes(&Value::Real(-0.0)),
        to_canonical_bytes(&Value::Real(0.0))
    );
    // NaN → canonical quiet-NaN
    let nan_bytes = to_canonical_bytes(&Value::Real(f64::NAN));
    assert_eq!(nan_bytes, vec![0x7F, 0xF8, 0, 0, 0, 0, 0, 0]);
    // signaling-NaN also normalized
    let sig_nan = f64::from_bits(0x7FF0_0000_0000_0001);
    assert_eq!(to_canonical_bytes(&Value::Real(sig_nan)), nan_bytes);
}

#[test]
fn text_serializes_as_utf8_verbatim() {
    assert_eq!(
        to_canonical_bytes(&Value::Text("hé".into())),
        "hé".as_bytes().to_vec()
    );
    // no NFC normalization
    let composed = "\u{00E9}";
    let decomposed = "e\u{0301}";
    assert_ne!(
        to_canonical_bytes(&Value::Text(composed.into())),
        to_canonical_bytes(&Value::Text(decomposed.into()))
    );
}

#[test]
fn blob_serializes_verbatim() {
    assert_eq!(
        to_canonical_bytes(&Value::Blob(vec![0, 1, 2, 255])),
        vec![0, 1, 2, 255]
    );
}
