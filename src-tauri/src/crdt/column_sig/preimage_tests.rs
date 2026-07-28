use super::preimage::{build_preimage, DOMAIN_TAG};

#[test]
fn preimage_starts_with_length_prefixed_domain_tag() {
    let p = build_preimage(b"S", b"T", b"P", b"C", b"H", b"D", b"V");
    let tag = DOMAIN_TAG.as_bytes();
    let mut expected = (tag.len() as u32).to_be_bytes().to_vec();
    expected.extend_from_slice(tag);
    assert_eq!(&p[..expected.len()], &expected[..]);
}

#[test]
fn preimage_is_length_prefixed_concatenation() {
    let p = build_preimage(b"space", b"tbl", b"pk", b"col", b"hlc", b"did", b"val");
    let tag_len = DOMAIN_TAG.as_bytes().len();
    let expected_total = 4 + tag_len + 4 + 5 + 4 + 3 + 4 + 2 + 4 + 3 + 4 + 3 + 4 + 3 + 4 + 3;
    assert_eq!(p.len(), expected_total);
}

#[test]
fn preimage_domain_tag_matches_spec() {
    assert_eq!(DOMAIN_TAG, "haex/space-col-sig/v1");
}

#[test]
fn different_space_ids_produce_different_preimages() {
    let a = build_preimage(b"S1", b"T", b"P", b"C", b"H", b"D", b"V");
    let b = build_preimage(b"S2", b"T", b"P", b"C", b"H", b"D", b"V");
    assert_ne!(a, b);
}

#[test]
fn empty_value_bytes_still_produces_valid_preimage() {
    let p = build_preimage(b"S", b"T", b"P", b"C", b"H", b"D", b"");
    // last 4 bytes = length prefix "0"; no trailing payload
    assert_eq!(&p[p.len() - 4..], &[0, 0, 0, 0]);
}
