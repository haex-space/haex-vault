//! The area of an extension in a bucket (research R4, R5, SC-003): disjoint per vault, per
//! extension and for development versions, and a corpus of escape attempts that are all refused.

use uuid::Uuid;

use super::{fits_tree, Area, MAX_KEY_BYTES};
use crate::extensions::ids::{extension_id, storage_vault_id, ExtensionName, PublicKey};

const VAULT_A: [u8; 32] = [1; 32];
const VAULT_B: [u8; 32] = [2; 32];

fn installed() -> Uuid {
    extension_id(
        &PublicKey::parse(&"ab".repeat(32)).expect("key"),
        &ExtensionName::parse("notes").expect("name"),
    )
}

#[test]
fn the_area_names_the_vault_and_the_extension_and_nothing_else() {
    let area = Area::new(storage_vault_id(&VAULT_A), installed(), false);
    assert_eq!(
        area.prefix(),
        format!("holzi-ext/{}/{}/", storage_vault_id(&VAULT_A), installed())
    );
    assert!(
        !area.prefix().contains(&"ab".repeat(32)) && !area.prefix().contains("notes"),
        "neither the publisher key nor the name reach the provider"
    );
    assert!(area.prefix().len() <= 88, "short enough for 1024-byte keys");
}

#[test]
fn two_vaults_and_a_development_version_get_disjoint_areas() {
    let a = Area::new(storage_vault_id(&VAULT_A), installed(), false);
    let b = Area::new(storage_vault_id(&VAULT_B), installed(), false);
    let dev = Area::new(storage_vault_id(&VAULT_A), installed(), true);
    for (x, y) in [(&a, &b), (&a, &dev), (&b, &dev)] {
        assert!(!x.prefix().starts_with(y.prefix()) && !y.prefix().starts_with(x.prefix()));
    }
    let theirs = a.key("photos/1.jpg").expect("key");
    assert_eq!(
        b.strip(&theirs),
        None,
        "another vault's object is not in the area"
    );
    assert_eq!(
        dev.strip(&theirs),
        None,
        "a development version with the installed key and name never sees its objects"
    );
    assert_eq!(a.strip(&theirs), Some("photos/1.jpg"));
}

#[test]
fn escape_attempts_are_refused() {
    let area = Area::new(storage_vault_id(&VAULT_A), installed(), false);
    let corpus = [
        "",
        "/abs",
        "../x",
        "a/../../x",
        "a/./b",
        ".",
        "..",
        "a//b",
        "a/",
        "a\\b",
        "..\\x",
        "a\0b",
        "a\nb",
        "a\rb",
        "a\tb",
        "a\u{7f}b",
        "a\u{1b}b",
    ];
    for key in corpus {
        assert!(area.key(key).is_err(), "{key:?} must be refused");
    }
    let long = "k".repeat(MAX_KEY_BYTES - area.prefix().len() + 1);
    assert!(
        area.key(&long).is_err(),
        "longer than 1024 bytes with the prefix"
    );
    let fits = "k".repeat(MAX_KEY_BYTES - area.prefix().len());
    assert!(area.key(&fits).is_ok());
}

#[test]
fn ordinary_keys_and_list_prefixes_pass() {
    let area = Area::new(storage_vault_id(&VAULT_A), installed(), false);
    for key in [
        "a",
        "a/b.txt",
        "Fotos/2026/Urlaub am Meer.jpg",
        "x..y",
        ".hidden",
        "ä/ö",
    ] {
        assert_eq!(area.key(key).expect(key), format!("{}{key}", area.prefix()));
    }
    assert_eq!(area.list_prefix("").expect("whole area"), area.prefix());
    assert_eq!(
        area.list_prefix("photos/").expect("a folder"),
        format!("{}photos/", area.prefix())
    );
    for prefix in ["/", "../", "a//", "a/../", "a\\"] {
        assert!(area.list_prefix(prefix).is_err(), "{prefix:?}");
    }
}

#[test]
fn keys_of_the_provider_outside_the_area_are_dropped() {
    let area = Area::new(storage_vault_id(&VAULT_A), installed(), false);
    assert_eq!(area.strip("holzi-test/123"), None);
    assert_eq!(
        area.strip(area.prefix()),
        None,
        "the area itself is no object"
    );
    assert_eq!(area.strip("other/a"), None);
}

#[test]
fn encoded_separators_and_dots_stay_literal_inside_the_area() {
    let area = Area::new(storage_vault_id(&VAULT_A), installed(), false);
    let endpoint = "https://s3.example.com".parse().expect("endpoint");
    let bucket = rusty_s3::Bucket::new(endpoint, rusty_s3::UrlStyle::Path, "b", "eu").expect("b");
    let inside = format!("/b/{}", area.prefix());
    for key in [
        "%2e%2e/x",
        "a/%2E%2E/%2e%2e/x",
        "a%2F..%2F..%2Fx",
        "..%2fx",
        ".%2e/x",
        "%2F",
        "%5C..%5Cx",
        "a?x=%2e%2e%2F",
        "a#%2e%2e%2F",
        "a;/%2e%2e/x",
    ] {
        let full = area.key(key).expect(key);
        let url = bucket.object_url(&full).expect("url");
        assert!(
            url.path().starts_with(&inside),
            "{key:?} reached {}",
            url.path()
        );
        assert!(
            !url.path().contains("/../") && !url.path().contains("/./"),
            "{key:?}"
        );
        assert_eq!(url.query(), None, "{key:?}");
        assert_eq!(url.fragment(), None, "{key:?}");
    }
}

/// Spec 050 research R9: what a server with real folders cannot store, refused on WebDAV and SFTP.
#[test]
fn a_server_with_real_folders_takes_no_long_parts_or_refused_characters() {
    for good in [
        "a/b.txt",
        "Ärger/ü ö.txt",
        &"x".repeat(255),
        "a.holzi-part-x",
    ] {
        assert!(fits_tree(good), "{good:?}");
    }
    let long = "x".repeat(256);
    let part = format!("a/{}1", crate::remote_storage::PART_PREFIX);
    for bad in ["a:b", "a/b*", "was?", "\"x\"", "<x>", "a|b", &long, &part] {
        assert!(!fits_tree(bad), "{bad:?}");
    }
}
