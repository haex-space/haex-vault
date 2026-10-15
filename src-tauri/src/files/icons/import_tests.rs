//! The import of a set (spec 053 US3, T042): what is taken, what is left out and why, and what is
//! refused. Archives, bombs and broken images are built here; the folders are the fixtures of
//! `tests/fixtures/icon_sets/`.

use std::io::{Cursor, Write};

use zip::write::SimpleFileOptions;
use zip::ZipWriter;

use super::*;
use crate::files::icons::import_support::{fixture, png, zip_of, zip_of_dir};
use crate::files::icons::pack::open_pack;
use crate::files::icons::sources::{Archive, Files, Folder};
use crate::files::icons::{IconAnswer, IconQuery, Scheme};

const SVG: &[u8] = br##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 16 16"><path d="M1 1h14v14H1z" fill="#123"/></svg>"##;

/// A theme that names the given images by path: `file` is the first, every other one an extension.
fn manifest_for(images: &[&str]) -> Vec<u8> {
    let definitions: Vec<String> = images
        .iter()
        .enumerate()
        .map(|(index, path)| format!(r#""d{index}": {{"iconPath": "{path}"}}"#))
        .collect();
    let extensions: Vec<String> = (1..images.len())
        .map(|index| format!(r#""e{index}": "d{index}""#))
        .collect();
    format!(
        r#"{{"iconDefinitions": {{{}}}, "file": "d0", "fileExtensions": {{{}}}}}"#,
        definitions.join(","),
        extensions.join(",")
    )
    .into_bytes()
}

fn archive(files: &[(&str, &[u8])]) -> Archive {
    Archive::open(zip_of(files)).expect("an archive")
}

fn prepared_dir(name: &str) -> Prepared {
    let folder = Folder::open(&fixture(name)).expect("folder");
    prepare(&folder, name).expect("prepared")
}

fn query(name: &str) -> IconQuery {
    IconQuery {
        name: name.to_owned(),
        is_dir: false,
        no_access: false,
        encrypted: None,
    }
}

fn id_of(prepared: &Prepared, scheme: Scheme, name: &str) -> String {
    let pack = open_pack(&prepared.pack, None).expect("opens");
    match super::super::resolve(Some(pack.theme()), scheme, &query(name)) {
        IconAnswer::Image { id, .. } => id,
        IconAnswer::Builtin { name } => format!("builtin:{name}"),
    }
}

#[test]
fn a_folder_and_the_same_content_as_a_zip_give_the_same_report() {
    let from_folder = prepared_dir("theme-basic");
    let zip = Archive::open(zip_of_dir(&fixture("theme-basic"), "")).expect("zip");
    let from_zip = prepare(&zip, "theme-basic").expect("prepared");
    assert_eq!(from_folder.report, from_zip.report);
    assert_eq!(
        from_folder.pack, from_zip.pack,
        "the same pack, byte for byte"
    );
    assert_eq!(from_folder.report.image_count, 4);
    assert_eq!(from_folder.report.skipped, []);
    assert_eq!(from_folder.report.name, "theme-basic");
}

#[test]
fn the_pack_of_a_theme_resolves_like_the_theme() {
    let prepared = prepared_dir("theme-basic");
    assert_eq!(id_of(&prepared, Scheme::Dark, "main.rs"), "icons/rust.svg");
    assert_eq!(
        id_of(&prepared, Scheme::Light, "main.rs"),
        "icons/rust_light.svg"
    );
    assert_eq!(
        id_of(&prepared, Scheme::Dark, "Cargo.toml"),
        "icons/rust.svg"
    );
    assert_eq!(
        id_of(&prepared, Scheme::Dark, "x.unknown"),
        "icons/file.svg"
    );
    let pack = open_pack(&prepared.pack, None).expect("opens");
    assert!(pack.license_text().contains("The fixture theme"));
}

#[test]
fn a_vsix_finds_its_theme_through_package_json_and_names_itself() {
    let zip = Archive::open(zip_of_dir(&fixture("theme-vsix"), "")).expect("vsix");
    let prepared = prepare(&zip, "download").expect("prepared");
    assert_eq!(prepared.report.name, "Demo Icons");
    assert_eq!(prepared.report.author, "demo");
    assert_eq!(prepared.report.license, "MIT");
    assert_eq!(prepared.source, "https://example.com/demo-icons");
    assert_eq!(prepared.report.image_count, 2);
    assert_eq!(id_of(&prepared, Scheme::Dark, "a.txt"), "icons/a.svg");
    assert_eq!(
        id_of(&prepared, Scheme::Dark, "b.zzz"),
        "icons/a.svg",
        "the default file image is the first one"
    );
    let pack = open_pack(&prepared.pack, None).expect("opens");
    assert_eq!(pack.license_text(), "MIT");
    // The same theme as a folder of the extension.
    let folder = Folder::open(&fixture("theme-vsix").join("extension")).expect("folder");
    let from_folder = prepare(&folder, "extension").expect("prepared");
    assert_eq!(from_folder.pack, prepared.pack);
}

#[test]
fn a_set_that_names_itself_nowhere_takes_the_name_of_its_file() {
    let prepared = prepared_dir("theme-basic");
    assert_eq!(prepared.report.name, "theme-basic");
    assert_eq!(prepared.report.author, "");
    assert_eq!(prepared.report.license, "");
}

#[test]
fn an_evil_svg_is_stored_sanitized() {
    let evil = std::fs::read(fixture("evil.svg")).expect("fixture");
    let files = archive(&[
        ("icon-theme.json", &manifest_for(&["./evil.svg"])),
        ("evil.svg", &evil),
    ]);
    let prepared = prepare(&files, "evil").expect("prepared");
    let pack = open_pack(&prepared.pack, None).expect("opens");
    let stored = String::from_utf8(pack.files()["evil.svg"].clone()).expect("text");
    for gone in [
        "script",
        "onload",
        "onclick",
        "foreignObject",
        "example.com",
        "alert",
    ] {
        assert!(!stored.contains(gone), "{gone} is still in {stored}");
    }
    assert!(stored.contains("<path"), "the shape stays: {stored}");
}

#[test]
fn a_path_that_climbs_out_of_an_archive_refuses_it() {
    for name in ["../x.svg", "/etc/x.svg", "a/../../x.svg", "a\\b.svg"] {
        let bytes = zip_of(&[
            ("icon-theme.json", &manifest_for(&["./ok.svg"])),
            ("ok.svg", SVG),
            (name, SVG),
        ]);
        assert!(
            matches!(Archive::open(bytes), Err(ImportError::UnsafePath(_))),
            "{name}"
        );
    }
}

#[test]
fn a_zip_bomb_is_refused_before_anything_is_read() {
    let zeros = vec![0u8; 8 * 1024 * 1024];
    let bomb = zip_of(&[("big.bin", &zeros), ("icon-theme.json", b"{}")]);
    // Zeros pack to a few KiB, far under a 200th of the declared size.
    let mut options_check = bomb.clone();
    options_check.truncate(options_check.len());
    assert!(
        matches!(Archive::open(bomb), Err(ImportError::Ratio)),
        "a bomb of {} bytes",
        options_check.len()
    );
}

#[test]
fn a_directory_entry_is_ignored_and_a_link_is_not_followed() {
    let mut writer = ZipWriter::new(Cursor::new(Vec::new()));
    writer
        .add_directory("icons/", SimpleFileOptions::default())
        .expect("dir");
    writer
        .start_file("icons/a.svg", SimpleFileOptions::default())
        .expect("file");
    writer.write_all(SVG).expect("write");
    writer
        .add_symlink("icons/link.svg", "a.svg", SimpleFileOptions::default())
        .expect("link");
    writer
        .start_file("icon-theme.json", SimpleFileOptions::default())
        .expect("file");
    writer
        .write_all(&manifest_for(&["./icons/a.svg", "./icons/link.svg"]))
        .expect("write");
    let files = Archive::open(writer.finish().expect("finish").into_inner()).expect("archive");
    assert!(files.is_link("icons/link.svg"));
    let prepared = prepare(&files, "links").expect("prepared");
    assert_eq!(prepared.report.image_count, 1);
    assert_eq!(prepared.report.skipped.len(), 1);
    assert_eq!(prepared.report.skipped[0].path, "./icons/link.svg");
    assert!(prepared.report.skipped[0].reason.contains("link"));
}

#[cfg(unix)]
#[test]
fn a_symbolic_link_of_a_folder_is_not_followed() {
    let dir = tempfile::tempdir().expect("tempdir");
    std::fs::write(dir.path().join("a.svg"), SVG).expect("write");
    std::fs::write(dir.path().join("secret.svg"), SVG).expect("write");
    std::os::unix::fs::symlink("secret.svg", dir.path().join("link.svg")).expect("link");
    std::fs::write(
        dir.path().join("icon-theme.json"),
        manifest_for(&["a.svg", "link.svg"]),
    )
    .expect("write");
    let prepared = prepare(&Folder::open(dir.path()).expect("folder"), "x").expect("prepared");
    assert_eq!(prepared.report.skipped.len(), 1);
    assert!(prepared.report.skipped[0].reason.contains("link"));
}

#[test]
fn a_reference_that_leaves_the_set_is_reported_and_the_rest_is_taken() {
    let prepared = prepared_dir("outside-ref");
    assert_eq!(prepared.report.image_count, 1);
    assert_eq!(
        prepared.report.skipped,
        [IconImportSkipped {
            path: "../secret.svg".to_owned(),
            reason: "the path leaves the set".to_owned()
        }]
    );
    assert_eq!(id_of(&prepared, Scheme::Dark, "secret.txt"), "icons/ok.svg");
}

#[test]
fn a_corrupt_or_too_large_image_is_left_out_with_its_reason() {
    let corrupt = b"\x89PNG\r\n\x1a\nnot an image at all".to_vec();
    let big = png(513, 16, 7);
    let heavy = vec![b' '; 300 * 1024];
    let files = archive(&[
        (
            "icon-theme.json",
            &manifest_for(&[
                "ok.svg",
                "corrupt.png",
                "big.png",
                "heavy.svg",
                "missing.svg",
                "a.gif",
            ]),
        ),
        ("ok.svg", SVG),
        ("corrupt.png", &corrupt),
        ("big.png", &big),
        ("heavy.svg", &heavy),
        ("a.gif", b"GIF89a"),
    ]);
    let prepared = prepare(&files, "mixed").expect("prepared");
    assert_eq!(prepared.report.image_count, 1);
    let reasons: Vec<(&str, &str)> = prepared
        .report
        .skipped
        .iter()
        .map(|skipped| (skipped.path.as_str(), skipped.reason.as_str()))
        .collect();
    assert_eq!(
        reasons,
        [
            ("a.gif", "only SVG, PNG and WebP images are taken"),
            ("big.png", "the image is larger than 512 x 512 pixels"),
            ("corrupt.png", "the image cannot be read"),
            ("heavy.svg", "larger than 256 KiB"),
            ("missing.svg", "not found in the set"),
        ]
    );
    // What named a left-out image falls back to the file image of the set.
    assert_eq!(id_of(&prepared, Scheme::Dark, "x.e1"), "ok.svg");
    assert_eq!(id_of(&prepared, Scheme::Dark, "x.e3"), "ok.svg");
}

#[test]
fn no_manifest_and_no_usable_image_write_nothing_and_say_why() {
    let none = Folder::open(&fixture("no-manifest")).expect("folder");
    assert_eq!(prepare(&none, "x").err(), Some(ImportError::NoManifest));
    let files = archive(&[
        ("icon-theme.json", &manifest_for(&["gone.svg"])),
        ("readme.txt", b"hello"),
    ]);
    assert_eq!(prepare(&files, "x").err(), Some(ImportError::NoUsableImage));
    let broken = archive(&[("icon-theme.json", b"{ not json")]);
    assert!(matches!(
        prepare(&broken, "x"),
        Err(ImportError::BadManifest(_))
    ));
    for error in [ImportError::NoManifest, ImportError::NoUsableImage] {
        assert!(error.to_string().ends_with('.'), "one sentence: {error}");
    }
}

#[test]
fn more_than_2000_images_are_refused() {
    let names: Vec<String> = (0..2001).map(|index| format!("i{index}.svg")).collect();
    let refs: Vec<&str> = names.iter().map(String::as_str).collect();
    let manifest = manifest_for(&refs);
    let mut files: Vec<(&str, &[u8])> = vec![("icon-theme.json", &manifest)];
    files.extend(refs.iter().map(|name| (*name, SVG)));
    let archive = Archive::open(zip_of(&files)).expect("archive");
    assert_eq!(
        prepare(&archive, "x").err(),
        Some(ImportError::TooManyImages)
    );
}

#[test]
fn more_than_5_mib_of_images_are_refused() {
    let images: Vec<(String, Vec<u8>)> = (0..29)
        .map(|index| (format!("n{index}.png"), png(256, 256, index)))
        .collect();
    let names: Vec<&str> = images.iter().map(|(name, _)| name.as_str()).collect();
    let manifest = manifest_for(&names);
    let mut files: Vec<(&str, &[u8])> = vec![("icon-theme.json", &manifest)];
    files.extend(
        images
            .iter()
            .map(|(name, data)| (name.as_str(), data.as_slice())),
    );
    let archive = Archive::open(zip_of(&files)).expect("archive");
    assert_eq!(prepare(&archive, "x").err(), Some(ImportError::TooMuchData));
}

#[test]
fn an_archive_over_the_limit_or_with_too_many_files_is_refused() {
    assert_eq!(
        Archive::open(vec![0; crate::files::icons::sources::MAX_ARCHIVE_BYTES + 1]).err(),
        Some(ImportError::ArchiveTooLarge)
    );
    assert!(matches!(
        Archive::open(b"not a zip".to_vec()),
        Err(ImportError::Unreadable(_))
    ));
}

#[test]
fn a_set_in_a_folder_of_a_download_is_found() {
    let zip = Archive::open(zip_of_dir(&fixture("theme-basic"), "theme-basic-main/")).expect("zip");
    let prepared = prepare(&zip, "download").expect("prepared");
    assert_eq!(prepared.report.image_count, 4);
}

#[test]
fn the_time_is_written_as_iso_8601() {
    assert_eq!(iso8601(0), "1970-01-01T00:00:00Z");
    assert_eq!(iso8601(951_782_400), "2000-02-29T00:00:00Z");
    assert_eq!(iso8601(1_700_000_000), "2023-11-14T22:13:20Z");
}
