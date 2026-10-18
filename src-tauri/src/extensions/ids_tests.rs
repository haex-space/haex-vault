use uuid::Uuid;

use super::*;

const PK: &str = "abababababababababababababababababababababababababababababababab";
const DEVICE: &str = "11111111-2222-3333-4444-555555555555";

fn pk() -> PublicKey {
    PublicKey::parse(PK).expect("valid key")
}

fn name() -> ExtensionName {
    ExtensionName::parse("haex-notes").expect("valid name")
}

#[test]
fn derived_ids_are_pinned_for_fixed_inputs() {
    // Expected values computed independently (Python `uuid.uuid5`); a changed namespace or input
    // format fails here, because it would give existing rows new ids on updated devices.
    let ext = extension_id(&pk(), &name());
    assert_eq!(ext.to_string(), "b849d26a-a1d0-587d-b212-71a81da7b609");

    let bundle = bundle_id(&"cd".repeat(32));
    assert_eq!(bundle.to_string(), "f59b1fe6-0670-5df6-9ce1-914d28cc0124");
    assert_eq!(
        bundle_file_id(bundle, "index.html").to_string(),
        "be2440dd-6d7d-5c78-8dd9-bd7aca28835c"
    );
    assert_eq!(
        migration_id(ext, "0000_init", &"ef".repeat(32)).to_string(),
        "aa15d3ee-0ea9-5887-9529-dfc58c024ad8"
    );
    assert_eq!(
        permission_id(ext, "web", "GET", "https://example.org/*", Uuid::nil()).to_string(),
        "e6f657e6-5ec8-5999-8a9a-046d6abcbfb6"
    );
    assert_eq!(
        limits_id(ext).to_string(),
        "f3441b8e-855b-518f-a6ec-3b9ae8dfd420"
    );
    let device = Uuid::parse_str(DEVICE).expect("uuid");
    assert_eq!(
        device_status_id(ext, device).to_string(),
        "8536e7e8-e7ce-50f9-a95a-85c08b3e1ec8"
    );
    assert_eq!(
        dev_extension_id(device, &pk(), &name()).to_string(),
        "221fbdbf-0326-5684-825e-11372026b3af"
    );
}

#[test]
fn the_extension_id_ignores_the_case_of_the_key() {
    let upper = PublicKey::parse_case_insensitive(&PK.to_uppercase()).expect("valid key");
    assert_eq!(extension_id(&upper, &name()), extension_id(&pk(), &name()));
}

#[test]
fn a_public_key_is_64_lowercase_hex_characters() {
    assert!(PublicKey::parse(PK).is_ok());
    assert_eq!(
        PublicKey::parse(&PK.to_uppercase()),
        Err(IdError::InvalidPublicKey)
    );
    assert_eq!(PublicKey::parse(&PK[..63]), Err(IdError::InvalidPublicKey));
    assert_eq!(
        PublicKey::parse(&format!("{PK}a")),
        Err(IdError::InvalidPublicKey)
    );
    assert_eq!(
        PublicKey::parse(&format!("{}g", &PK[..63])),
        Err(IdError::InvalidPublicKey)
    );
}

#[test]
fn an_extension_name_follows_fr_004() {
    for good in ["a", "haex-notes", "x1", "a-b-c"] {
        assert!(ExtensionName::parse(good).is_ok(), "{good}");
    }
    for bad in [
        "",
        "1abc",
        "-abc",
        "Haex",
        "haex_notes",
        "a__b",
        "haex notes",
        "ä",
    ] {
        assert_eq!(
            ExtensionName::parse(bad),
            Err(IdError::InvalidName),
            "{bad:?}"
        );
    }
}

#[test]
fn a_table_name_splits_into_exactly_three_parts() {
    let table = format!("{PK}__haex-notes__pages");
    let parsed = ExtensionTable::parse(&table).expect("extension table");
    assert_eq!(parsed.prefix.public_key, pk());
    assert_eq!(parsed.prefix.name, name());
    assert_eq!(parsed.table, "pages");
    assert_eq!(parsed.prefix.to_string(), format!("{PK}__haex-notes__"));

    let with_underscores = format!("{PK}__haex-notes__note_tags_no_sync");
    assert_eq!(
        ExtensionTable::parse(&with_underscores)
            .expect("single underscores are fine")
            .table,
        "note_tags_no_sync"
    );
}

#[test]
fn the_prefix_of_one_extension_never_covers_another() {
    // `a` and `a__b` would collide with a plain `starts_with` (haex-vault 2.x (Legacy) `utils.rs:202-210`); an
    // extension name cannot contain `__`, and a table name with four parts is not an extension
    // table at all.
    assert!(ExtensionTable::parse(&format!("{PK}__a__b__t")).is_err());
    assert!(ExtensionTable::parse(&format!("{PK}__a__t__u")).is_err());
    let a = ExtensionTable::parse(&format!("{PK}__a__t")).expect("table of a");
    assert_eq!(a.prefix.name.as_str(), "a");
}

#[test]
fn malformed_table_names_are_not_extension_tables() {
    for bad in [
        "chat_threads".to_string(),
        "haex_crdt_configs_no_sync".to_string(),
        format!("{PK}__haex-notes"),
        format!("{PK}__haex-notes__"),
        format!("__new_{PK}__haex-notes__pages"),
        format!("{}__haex-notes__pages", &PK[..62]),
        format!("{PK}__Haex_Notes__pages"),
        format!("{PK}__haex-notes__1pages"),
        format!("{PK}__haex-notes__pa ges"),
    ] {
        assert!(ExtensionTable::parse(&bad).is_err(), "{bad}");
    }
}

#[test]
fn table_names_compare_without_regard_to_case() {
    let lower = ExtensionTable::parse(&format!("{PK}__haex-notes__pages")).expect("lower");
    let upper = ExtensionTable::parse(&format!("{}__HAEX-NOTES__Pages", PK.to_uppercase()))
        .expect("SQLite resolves table names case-insensitively");
    assert_eq!(lower, upper);
}

#[test]
fn a_public_key_gives_the_bytes_its_hex_stands_for() {
    let key = PublicKey::parse(&format!("00ff10{}", "a".repeat(58))).unwrap();
    let bytes = key.bytes();
    assert_eq!(&bytes[..3], &[0x00, 0xff, 0x10]);
    assert!(bytes[3..].iter().all(|b| *b == 0xaa));
}
