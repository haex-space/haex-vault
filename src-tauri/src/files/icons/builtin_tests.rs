//! Tests of the bundled packs: the ZIPs in `src-tauri/icon-sets/` are the result of packing the
//! source directories next to them, and every one passes the checks of an import (research R6).

use std::collections::BTreeMap;
use std::path::PathBuf;

use super::*;
use crate::files::icons::pack::open_pack;
use crate::files::icons::pack_dir;

/// The allowed SPDX identifiers of a bundled set (FR-017, SC-008); `AND` joins several.
const ALLOWED: [&str; 4] = ["MIT", "ISC", "Apache-2.0", "CC0-1.0"];

fn icon_sets_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("icon-sets")
}

const PACKS: [(BuiltinSet, &str); 2] = [
    (BuiltinSet::Phosphor, "phosphor"),
    (BuiltinSet::Material, "material"),
];

/// Writes the ZIPs of the bundled packs from their source directories. Run it after
/// `scripts/icon-sets/prepare.ts` changed a directory:
/// `cargo test --lib files::icons::builtin::tests::build_bundled_packs -- --ignored`.
#[test]
#[ignore = "writes src-tauri/icon-sets/*.zip; run after prepare.ts"]
fn build_bundled_packs() {
    for (_, dir) in PACKS {
        let bytes = pack_dir(&icon_sets_dir().join(dir)).expect("pack the directory");
        std::fs::write(icon_sets_dir().join(format!("{dir}.zip")), &bytes).expect("write the pack");
        eprintln!("{dir}.zip: {} bytes", bytes.len());
    }
}

/// The files of a directory by their path with `/`, read from disk.
fn files_of(dir: &std::path::Path) -> BTreeMap<String, Vec<u8>> {
    walkdir::WalkDir::new(dir)
        .into_iter()
        .map(|entry| entry.expect("walk"))
        .filter(|entry| entry.file_type().is_file())
        .map(|entry| {
            let name = entry
                .path()
                .strip_prefix(dir)
                .expect("inside")
                .components()
                .map(|part| part.as_os_str().to_string_lossy().into_owned())
                .collect::<Vec<_>>()
                .join("/");
            (name, std::fs::read(entry.path()).expect("read"))
        })
        .collect()
}

#[test]
fn every_bundled_pack_passes_the_checks() {
    for (name, dir) in PACKS {
        let pack = open_pack(zip_bytes(name), None)
            .unwrap_or_else(|error| panic!("the bundled pack {dir} does not open: {error}"));
        assert!(pack.image_count() > 0, "{dir} has images");
        // Every image of the theme is made without an error: SVG sanitized, raster checked.
        for path in pack.theme().image_paths() {
            pack.image(path)
                .unwrap_or_else(|error| panic!("{dir}: {path}: {error}"));
        }
        assert!(
            !pack.license_text().is_empty(),
            "{dir} carries its license text"
        );
    }
}

#[test]
fn every_bundled_pack_is_what_its_source_directory_packs_to() {
    for (name, dir) in PACKS {
        let source = files_of(&icon_sets_dir().join(dir));
        let pack = open_pack(zip_bytes(name), None).expect("opens");
        assert_eq!(
            pack.files().keys().collect::<Vec<_>>(),
            source.keys().collect::<Vec<_>>(),
            "{dir}: the same files"
        );
        for (path, bytes) in &source {
            assert!(
                pack.files()[path] == *bytes,
                "{dir}: {path} differs; run build_bundled_packs"
            );
        }
    }
}

#[test]
fn the_license_of_every_bundled_set_is_open_and_complete() {
    for license in licenses() {
        assert!(!license.author.is_empty(), "{}: an author", license.name);
        assert!(
            !license.source.is_empty() && !license.source.contains("/home/"),
            "{}: a source",
            license.name
        );
        assert!(!license.text.is_empty(), "{}: a text", license.name);
        let parts: Vec<&str> = license.license.split(" AND ").collect();
        assert!(!parts.is_empty());
        for part in parts {
            assert!(
                ALLOWED.contains(&part),
                "{}: {part} is not an allowed license",
                license.name
            );
        }
    }
    let text = |name: &str| {
        licenses()
            .into_iter()
            .find(|l| l.name == name)
            .expect("listed")
            .text
    };
    assert!(
        text("Lucide").contains("ISC License") && text("Lucide").contains("Cole Bemis"),
        "ISC and the MIT of Feather"
    );
    assert!(text("Phosphor").contains("MIT License"));
    let material = text("Material");
    assert!(material.contains("MIT License") && material.contains("Apache License"));
    assert!(
        material.contains("belong to their owners"),
        "the note on the marks"
    );
}

#[test]
fn the_list_names_the_three_sets_and_none_can_be_removed() {
    let sets = infos();
    assert_eq!(
        sets.iter().map(|set| set.name.as_str()).collect::<Vec<_>>(),
        ["Lucide", "Phosphor", "Material"]
    );
    assert!(sets
        .iter()
        .all(|set| !set.removable && set.state == IconSetState::Ready));
}

#[test]
fn a_bundled_pack_holds_no_logo_named_image_of_phosphor() {
    // Phosphor ships `*-logo` icons of companies; the selection must never take one (FR-017).
    let pack = pack(BuiltinSet::Phosphor).expect("opens");
    assert!(pack.files().keys().all(|name| !name.contains("-logo")));
}
