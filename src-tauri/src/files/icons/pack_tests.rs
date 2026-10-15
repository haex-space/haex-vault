use std::io::{Cursor, Write};

use zip::write::SimpleFileOptions;
use zip::{CompressionMethod, ZipWriter};

use super::*;

const MANIFEST: &str = r#"{"iconDefinitions": {"a": {"iconPath": "icons/a.svg"}}, "file": "a", "fileExtensions": {"rs": "a"}, "x-holzi": {"origin": {"license": "MIT"}}}"#;
const SVG: &str =
    r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 1 1"><path d="M0 0h1v1z"/></svg>"##;

/// A ZIP of `(name, bytes)` entries; nothing is checked here, so a test can build a bad pack.
fn zip_of(entries: &[(&str, &[u8])]) -> Vec<u8> {
    zip_with(entries, CompressionMethod::Deflated)
}

fn zip_with(entries: &[(&str, &[u8])], method: CompressionMethod) -> Vec<u8> {
    let mut writer = ZipWriter::new(Cursor::new(Vec::new()));
    let options = SimpleFileOptions::default().compression_method(method);
    for (name, data) in entries {
        writer.start_file(*name, options).expect("start");
        writer.write_all(data).expect("write");
    }
    writer.finish().expect("finish").into_inner()
}

fn good() -> Vec<u8> {
    zip_of(&[
        ("icon-theme.json", MANIFEST.as_bytes()),
        ("icons/a.svg", SVG.as_bytes()),
        ("LICENSE", b"MIT text"),
        ("NOTICE", b"a notice"),
    ])
}

#[test]
fn a_good_pack_opens_and_counts_its_images() {
    let pack = open_pack(&good(), None).expect("opens");
    assert_eq!(pack.image_count(), 1);
    assert_eq!(pack.theme().image_paths(), ["icons/a.svg"]);
    assert_eq!(pack.license_text(), "MIT text\n\na notice");
}

#[test]
fn the_license_text_puts_license_first_and_the_notice_last() {
    let bytes = zip_of(&[
        ("icon-theme.json", b"{}"),
        ("NOTICE", b"notice"),
        ("LICENSE-APACHE-2.0", b"apache"),
        ("LICENSE", b"mit"),
    ]);
    assert_eq!(
        open_pack(&bytes, None).expect("opens").license_text(),
        "mit\n\napache\n\nnotice"
    );
}

#[test]
fn a_path_that_breaks_the_rules_is_refused() {
    for name in [
        "../x.svg",
        "/abs.svg",
        "a\\b.svg",
        "a:b.svg",
        "icons/./a.svg",
        "icons//a.svg",
        "",
    ] {
        let bytes = zip_of(&[("icon-theme.json", b"{}"), (name, SVG.as_bytes())]);
        assert!(
            matches!(open_pack(&bytes, None), Err(PackError::Path(_))),
            "{name:?} must be refused"
        );
    }
}

#[test]
fn two_paths_that_fold_to_the_same_name_are_refused() {
    let bytes = zip_of(&[
        ("icon-theme.json", b"{}"),
        ("icons/A.svg", SVG.as_bytes()),
        ("icons/a.svg", SVG.as_bytes()),
    ]);
    assert!(matches!(open_pack(&bytes, None), Err(PackError::Path(_))));
}

#[test]
fn a_directory_and_a_link_are_refused() {
    let mut writer = ZipWriter::new(Cursor::new(Vec::new()));
    writer
        .add_directory("icons/", SimpleFileOptions::default())
        .expect("dir");
    let with_dir = writer.finish().expect("finish").into_inner();
    assert!(open_pack(&with_dir, None).is_err());

    let mut writer = ZipWriter::new(Cursor::new(Vec::new()));
    writer
        .start_file("icon-theme.json", SimpleFileOptions::default())
        .expect("start");
    writer.write_all(b"{}").expect("write");
    writer
        .add_symlink("icons-link", "/etc/passwd", SimpleFileOptions::default())
        .expect("link");
    let with_link = writer.finish().expect("finish").into_inner();
    assert!(matches!(
        open_pack(&with_link, None),
        Err(PackError::EntryKind(_))
    ));
}

#[test]
fn the_limits_each_refuse_a_pack() {
    // More than 2000 images.
    let names: Vec<String> = (0..=limits::IMAGES).map(|n| format!("i/{n}.svg")).collect();
    let mut entries: Vec<(&str, &[u8])> = vec![("icon-theme.json", b"{}")];
    entries.extend(names.iter().map(|name| (name.as_str(), SVG.as_bytes())));
    assert_eq!(
        open_pack(&zip_of(&entries), None).err(),
        Some(PackError::TooManyImages)
    );

    // An image over 256 KiB.
    let big = vec![b'a'; limits::IMAGE_BYTES as usize + 1];
    let bytes = zip_with(
        &[("icon-theme.json", b"{}"), ("big.svg", &big)],
        CompressionMethod::Stored,
    );
    assert!(matches!(
        open_pack(&bytes, None),
        Err(PackError::ImageTooLarge(_))
    ));

    // More than 5 MiB unpacked, in images of 200 KiB that do not compress.
    let noise: Vec<u8> = (0..200 * 1024u32)
        .map(|n| (n.wrapping_mul(2_654_435_761) >> 13) as u8)
        .collect();
    let names: Vec<String> = (0..27).map(|n| format!("n/{n}.svg")).collect();
    let mut entries: Vec<(&str, &[u8])> = vec![("icon-theme.json", b"{}")];
    entries.extend(names.iter().map(|name| (name.as_str(), noise.as_slice())));
    assert_eq!(
        open_pack(&zip_with(&entries, CompressionMethod::Stored), None).err(),
        Some(PackError::TooMuchData)
    );

    // Packed over 8 MiB.
    assert_eq!(
        open_pack(&vec![0u8; limits::PACKED_BYTES + 1], None).err(),
        Some(PackError::TooLarge)
    );

    // A ZIP bomb: 4 MiB of zeros in a few KiB.
    let zeros = vec![0u8; 200 * 1024];
    let names: Vec<String> = (0..20).map(|n| format!("z/{n}.svg")).collect();
    let mut entries: Vec<(&str, &[u8])> = vec![("icon-theme.json", b"{}")];
    entries.extend(names.iter().map(|name| (name.as_str(), zeros.as_slice())));
    assert_eq!(
        open_pack(&zip_of(&entries), None).err(),
        Some(PackError::Ratio)
    );
}

#[test]
fn a_pack_needs_a_manifest_that_names_only_images_it_holds() {
    let bytes = zip_of(&[("icons/a.svg", SVG.as_bytes())]);
    assert_eq!(open_pack(&bytes, None).err(), Some(PackError::NoManifest));

    let bytes = zip_of(&[("icon-theme.json", MANIFEST.as_bytes())]);
    assert_eq!(
        open_pack(&bytes, None).err(),
        Some(PackError::MissingImage("icons/a.svg".to_owned()))
    );

    let bytes = zip_of(&[("icon-theme.json", b"not json")]);
    assert!(matches!(
        open_pack(&bytes, None),
        Err(PackError::Manifest(_))
    ));
    assert!(matches!(
        open_pack(b"not a zip", None),
        Err(PackError::Archive(_))
    ));
}

#[test]
fn a_pack_that_does_not_match_its_hash_is_refused() {
    let bytes = good();
    let hash = pack_hash(&bytes);
    assert_eq!(hash.len(), 64);
    open_pack(&bytes, Some(&hash)).expect("the right hash");
    assert_eq!(
        open_pack(&bytes, Some(&"0".repeat(64))).err(),
        Some(PackError::HashMismatch)
    );
    let mut flipped = bytes.clone();
    let middle = flipped.len() / 2;
    flipped[middle] ^= 0xff;
    assert_eq!(
        open_pack(&flipped, Some(&hash)).err(),
        Some(PackError::HashMismatch)
    );
}

#[test]
fn an_image_is_served_sanitized_and_only_once_made() {
    let evil = r#"<svg xmlns="http://www.w3.org/2000/svg"><script>alert(1)</script><path d="M0 0"/></svg>"#;
    let bytes = zip_of(&[
        ("icon-theme.json", MANIFEST.as_bytes()),
        ("icons/a.svg", evil.as_bytes()),
    ]);
    let pack = open_pack(&bytes, None).expect("opens");
    let served = pack.image("icons/a.svg").expect("served");
    assert_eq!(served.mime, ImageMime::Svg);
    let text = String::from_utf8(served.bytes.clone()).expect("utf-8");
    assert!(!text.contains("script") && text.contains("<path"));
    assert!(Arc::ptr_eq(
        &served,
        &pack.image("icons/a.svg").expect("again")
    ));
    assert!(matches!(
        pack.image("icons/none.svg"),
        Err(PackError::NoSuchImage(_))
    ));
    assert!(
        matches!(
            pack.image("icon-theme.json"),
            Err(PackError::NoSuchImage(_))
        ),
        "only images"
    );
}

#[test]
fn a_broken_image_is_refused_when_it_is_served() {
    let bytes = zip_of(&[
        ("icon-theme.json", MANIFEST.as_bytes()),
        ("icons/a.svg", b"<svg><g></svg>"),
    ]);
    let pack = open_pack(&bytes, None).expect("opens: images are checked when served");
    assert!(matches!(
        pack.image("icons/a.svg"),
        Err(PackError::BadImage { .. })
    ));
}

#[test]
fn a_directory_packs_the_same_way_twice_and_opens_again() {
    let dir = tempfile::tempdir().expect("tempdir");
    std::fs::create_dir_all(dir.path().join("icons")).expect("mkdir");
    std::fs::write(dir.path().join("icon-theme.json"), MANIFEST).expect("manifest");
    std::fs::write(dir.path().join("icons/a.svg"), SVG).expect("svg");
    std::fs::write(dir.path().join("LICENSE"), "MIT text").expect("license");

    let first = pack_dir(dir.path()).expect("packs");
    let second = pack_dir(dir.path()).expect("packs again");
    assert_eq!(first, second, "the same directory gives the same bytes");

    let pack = open_pack(&first, None).expect("opens");
    assert_eq!(
        pack.files().keys().map(String::as_str).collect::<Vec<_>>(),
        ["LICENSE", "icon-theme.json", "icons/a.svg"]
    );
}

#[cfg(unix)]
#[test]
fn a_directory_with_a_link_is_not_packed() {
    let dir = tempfile::tempdir().expect("tempdir");
    std::fs::write(dir.path().join("real.svg"), SVG).expect("svg");
    std::os::unix::fs::symlink(dir.path().join("real.svg"), dir.path().join("link.svg"))
        .expect("link");
    assert!(matches!(pack_dir(dir.path()), Err(PackError::EntryKind(_))));
}
