use super::*;

const ID: Id = [
    0x0f, 0x1e, 0x2d, 0x3c, 0x4b, 0x5a, 0x69, 0x78, 0x87, 0x96, 0xa5, 0xb4, 0xc3, 0xd2, 0xe1, 0xf0,
];

#[test]
fn an_id_is_26_lowercase_base32_characters_and_back() {
    let text = encode_id(&ID);
    assert_eq!(text, "b4pc2pclljuxrb4wuw2mhuxb6a");
    assert_eq!(decode_id(&text), Some(ID));
}

#[test]
fn only_the_one_spelling_of_an_id_decodes() {
    // Upper case, a wrong length, padding, a character outside the alphabet, non-zero trailing bits.
    for text in [
        "B4PC2PCLLJUXRB4WUW2MHUXB6A",
        "b4pc2pclljuxrb4wuw2mhuxb6",
        "b4pc2pclljuxrb4wuw2mhuxb6a======",
        "b4pc2pclljuxrb4wuw2mhuxb61",
        "b4pc2pclljuxrb4wuw2mhuxb6b",
    ] {
        assert_eq!(decode_id(text), None, "{text}");
    }
}

#[test]
fn the_folder_form_is_26_base32_characters_and_hxef() {
    assert!(is_folder_prefix("b4pc2pclljuxrb4wuw2mhuxb6a.hxef"));
    assert_eq!(folder_id("b4pc2pclljuxrb4wuw2mhuxb6a.hxef"), Some(ID));
    for name in [
        "b4pc2pclljuxrb4wuw2mhuxb6a",
        "b4pc2pclljuxrb4wuw2mhuxb6a.hxef/",
        "B4PC2PCLLJUXRB4WUW2MHUXB6A.hxef",
        "b4pc2pclljuxrb4wuw2mhuxb6.hxef",
        "b4pc2pclljuxrb4wuw2mhuxb6aa.hxef",
        "b4pc2pclljuxrb4wuw2mhuxb6a.HXEF",
        "Unterlagen.hxef",
        ".hxef",
    ] {
        assert!(!is_folder_prefix(name), "{name}");
        assert_eq!(folder_id(name), None, "{name}");
    }
}

#[test]
fn a_folder_form_without_a_valid_id_still_counts_as_one() {
    // Trailing bits set: the form is reserved, the id does not decode, so it shows as damaged.
    let name = "b4pc2pclljuxrb4wuw2mhuxb6b.hxef";
    assert!(is_folder_prefix(name));
    assert_eq!(folder_id(name), None);
}

#[test]
fn the_prefix_check_tells_a_newer_version_from_a_foreign_object() {
    assert!(check_prefix(b"HXEF\x01\x02rest", TYPE_SIDECAR).is_ok());
    assert!(matches!(
        check_prefix(b"HXEF\x02\x02", TYPE_SIDECAR),
        Err(FormatError::NewerFormat)
    ));
    for bytes in [&b"HXEF\x01\x01"[..], b"HXFE\x01\x02", b"HXEF\x00\x02"] {
        assert!(matches!(
            check_prefix(bytes, TYPE_SIDECAR),
            Err(FormatError::NotHxef)
        ));
    }
    assert!(matches!(
        check_prefix(b"HXEF\x01", TYPE_SIDECAR),
        Err(FormatError::Malformed)
    ));
}
