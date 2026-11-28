//! What the theSVG pack gives (spec 103, FR-002 to FR-005, SC-001): every kind of file has an image
//! of its own in both schemes, an unknown name gets the generic file image, the states keep
//! HaexVault's own images, and every colour reads on both schemes.

use super::bundled_tests::{dir, file, id_of};
use super::*;

fn dark(query: &IconQuery) -> String {
    id_of(BuiltinSet::Thesvg, Scheme::Dark, query)
}

fn light(query: &IconQuery) -> String {
    id_of(BuiltinSet::Thesvg, Scheme::Light, query)
}

#[test]
fn every_kind_of_file_has_an_image_of_its_own_in_both_schemes() {
    for (name, image) in [
        ("a.png", "icons/image.svg"),
        ("a.mp4", "icons/video.svg"),
        ("a.mp3", "icons/audio-lines.svg"),
        ("a.pdf", "icons/pdf-file.svg"),
        ("a.txt", "icons/file-text.svg"),
        ("a.rs", "icons/file-code.svg"),
        ("a.zip", "icons/archive-zip.svg"),
        ("a.json", "icons/json-file.svg"),
        ("a.md", "icons/markdown-file.svg"),
        ("a.yaml", "icons/config-file.svg"),
        ("a.xlsx", "icons/spreadsheet.svg"),
        ("a.pptx", "icons/presentation-file.svg"),
        ("a.log", "icons/log-file.svg"),
        ("README", "icons/readme-file.svg"),
        ("LICENSE", "icons/license-file.svg"),
        (".env.local", "icons/env-file.svg"),
        ("pnpm-lock.yaml", "icons/lockfile.svg"),
        ("tsconfig.json", "icons/config-file.svg"),
        ("A.PDF", "icons/pdf-file.svg"),
    ] {
        assert_eq!(dark(&file(name)), image, "{name} (dark)");
        assert_eq!(light(&file(name)), image, "{name} (light)");
    }
}

#[test]
fn a_folder_and_an_unknown_file_get_the_generic_images() {
    assert_eq!(dark(&dir("Projects")), "icons/folder.svg");
    assert_eq!(dark(&file("zz-no-such.file-kind")), "icons/file.svg");
}

#[test]
fn the_places_of_the_side_bar_get_an_image_of_their_own_in_both_schemes() {
    // The sidebar passes the place key (home, downloads …) as the folder name; German folder names
    // of the same places match too, as does a folder that is only named so.
    for (name, image) in [
        ("home", "icons/house.svg"),
        ("downloads", "icons/download.svg"),
        ("pictures", "icons/images.svg"),
        ("Bilder", "icons/images.svg"),
        ("documents", "icons/book-open-text.svg"),
        ("desktop", "icons/monitor.svg"),
        ("Schreibtisch", "icons/monitor.svg"),
        ("videos", "icons/film.svg"),
        ("music", "icons/headphones.svg"),
    ] {
        assert_eq!(dark(&dir(name)), image, "{name} (dark)");
        assert_eq!(light(&dir(name)), image, "{name} (light)");
    }
}

#[test]
fn the_pack_covers_at_least_ninety_extensions_and_names() {
    // SC-001: the count of the mapped entries, read from the manifest of the bundled pack.
    let pack = builtin::pack(BuiltinSet::Thesvg).expect("the theSVG pack");
    let manifest: serde_json::Value =
        serde_json::from_slice(&pack.files()["icon-theme.json"]).expect("a manifest");
    let count = |key: &str| manifest[key].as_object().map_or(0, |map| map.len());
    let covered = count("fileExtensions") + count("fileNames");
    assert!(covered >= 90, "{covered} extensions and names");
}

#[test]
fn the_set_has_no_light_block_because_every_colour_reads_on_both_schemes() {
    let pack = builtin::pack(BuiltinSet::Thesvg).expect("the theSVG pack");
    let manifest: serde_json::Value =
        serde_json::from_slice(&pack.files()["icon-theme.json"]).expect("a manifest");
    assert!(manifest.get("light").is_none(), "no light block (FR-005)");
}

/// WCAG relative luminance of `#rrggbb`.
fn luminance(color: &str) -> f64 {
    let channel = |at: usize| {
        let value = f64::from(u8::from_str_radix(&color[at..at + 2], 16).expect("hex")) / 255.0;
        if value <= 0.03928 {
            value / 12.92
        } else {
            ((value + 0.055) / 1.055).powf(2.4)
        }
    };
    0.2126 * channel(1) + 0.7152 * channel(3) + 0.0722 * channel(5)
}

#[test]
fn every_colour_of_the_images_reads_on_both_schemes() {
    // The images of the set (not HaexVault's own state images) carry only the colour of their kind
    // besides white and black mask cut-outs (FR-005: luminance 0.10 to 0.65).
    let pack = builtin::pack(BuiltinSet::Thesvg).expect("the theSVG pack");
    let mut checked = 0;
    for (path, bytes) in pack.files() {
        if !path.starts_with("icons/") || path.starts_with("icons/haex-") {
            continue;
        }
        let svg = String::from_utf8_lossy(bytes);
        let mut kind_colours = std::collections::BTreeSet::new();
        for (at, _) in svg.match_indices('#') {
            let color = &svg[at..(at + 7).min(svg.len())];
            if color.len() == 7
                && color[1..].chars().all(|c| c.is_ascii_hexdigit())
                && !matches!(color, "#ffffff" | "#000000")
            {
                kind_colours.insert(color.to_ascii_lowercase());
            }
        }
        assert!(
            !kind_colours.is_empty(),
            "{path} has the colour of its kind"
        );
        for color in kind_colours {
            let value = luminance(&color);
            assert!(
                (0.10..=0.65).contains(&value),
                "{path}: {color} has a luminance of {value:.3}"
            );
        }
        assert!(!svg.contains("currentColor"), "{path} is coloured");
        checked += 1;
    }
    assert!(checked >= 15, "{checked} images checked");
}
