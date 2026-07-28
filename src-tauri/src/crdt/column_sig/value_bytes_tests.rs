use super::value_bytes::{tag, to_canonical_bytes};
use rusqlite::types::Value;

#[test]
fn null_serializes_to_bare_tag() {
    assert_eq!(to_canonical_bytes(&Value::Null), vec![tag::NULL]);
}

#[test]
fn integer_serializes_as_tagged_be_i64() {
    assert_eq!(
        to_canonical_bytes(&Value::Integer(1)),
        vec![tag::INTEGER, 0, 0, 0, 0, 0, 0, 0, 1]
    );
    assert_eq!(
        to_canonical_bytes(&Value::Integer(-1)),
        vec![tag::INTEGER, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF]
    );
}

#[test]
fn real_serializes_as_tagged_be_ieee754_normalized() {
    // +0.0 == -0.0
    assert_eq!(
        to_canonical_bytes(&Value::Real(-0.0)),
        to_canonical_bytes(&Value::Real(0.0))
    );
    // NaN → canonical quiet-NaN
    let nan_bytes = to_canonical_bytes(&Value::Real(f64::NAN));
    assert_eq!(nan_bytes, vec![tag::REAL, 0x7F, 0xF8, 0, 0, 0, 0, 0, 0]);
    // signaling-NaN also normalized
    let sig_nan = f64::from_bits(0x7FF0_0000_0000_0001);
    assert_eq!(to_canonical_bytes(&Value::Real(sig_nan)), nan_bytes);
}

#[test]
fn text_serializes_as_tagged_utf8_verbatim() {
    let mut expected = vec![tag::TEXT];
    expected.extend_from_slice("hé".as_bytes());
    assert_eq!(to_canonical_bytes(&Value::Text("hé".into())), expected);
    // no NFC normalization
    let composed = "\u{00E9}";
    let decomposed = "e\u{0301}";
    assert_ne!(
        to_canonical_bytes(&Value::Text(composed.into())),
        to_canonical_bytes(&Value::Text(decomposed.into()))
    );
}

#[test]
fn blob_serializes_tagged_verbatim() {
    assert_eq!(
        to_canonical_bytes(&Value::Blob(vec![0, 1, 2, 255])),
        vec![tag::BLOB, 0, 1, 2, 255]
    );
}

/// The storage-class tag exists to stop signature replay across storage
/// classes. Untagged, every pair below produced identical bytes, so a
/// signature captured over one member verified against the other.
#[test]
fn storage_classes_with_identical_bodies_do_not_collide() {
    let empty_cases = [
        to_canonical_bytes(&Value::Null),
        to_canonical_bytes(&Value::Text(String::new())),
        to_canonical_bytes(&Value::Blob(Vec::new())),
    ];
    for (i, a) in empty_cases.iter().enumerate() {
        for b in empty_cases.iter().skip(i + 1) {
            assert_ne!(a, b, "NULL / TEXT(\"\") / BLOB([]) must not collide");
        }
    }

    // Integer(1) shares its 8-byte body with a blob and a text of the same
    // bytes; only the tag keeps them apart.
    let int_one = to_canonical_bytes(&Value::Integer(1));
    let blob_one = to_canonical_bytes(&Value::Blob(vec![0, 0, 0, 0, 0, 0, 0, 1]));
    assert_ne!(int_one, blob_one);
    assert_eq!(
        int_one[1..],
        blob_one[1..],
        "bodies are identical by design"
    );

    // Real bits reinterpreted as a blob likewise.
    let real = to_canonical_bytes(&Value::Real(1.5));
    let real_as_blob = to_canonical_bytes(&Value::Blob(1.5f64.to_bits().to_be_bytes().to_vec()));
    assert_ne!(real, real_as_blob);
    assert_eq!(real[1..], real_as_blob[1..]);
}

/// Tag values are a wire contract shared with the TS port and the fixture
/// generator — pin them so a reordering is a test failure, not a silent
/// cross-language break.
#[test]
fn tag_values_match_sqlite_type_codes() {
    assert_eq!(
        (tag::INTEGER, tag::REAL, tag::TEXT, tag::BLOB, tag::NULL),
        (1, 2, 3, 4, 5)
    );
}
