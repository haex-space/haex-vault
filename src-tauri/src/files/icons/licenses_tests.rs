//! The license rule of the bundled sets (spec 053 FR-017, SC-008, T055): what the page "Lizenzen"
//! lists is complete, open and free of anything local.

use super::builtin::licenses;

/// The licenses that may be bundled; `AND` joins several.
const ALLOWED: [&str; 4] = ["MIT", "ISC", "Apache-2.0", "CC0-1.0"];
/// Copyleft families that may never be bundled.
const COPYLEFT: [&str; 6] = ["GPL", "AGPL", "LGPL", "MPL", "CC-BY-SA", "EUPL"];

#[test]
fn the_page_lists_lucide_phosphor_and_material_in_full() {
    let all = licenses();
    let names: Vec<&str> = all.iter().map(|entry| entry.name.as_str()).collect();
    assert_eq!(names, ["Lucide", "Phosphor", "Material"]);
    for entry in &all {
        assert!(!entry.author.trim().is_empty(), "{}: an author", entry.name);
        assert!(
            entry.source.starts_with("https://"),
            "{}: a source address, not {:?}",
            entry.name,
            entry.source
        );
        assert!(!entry.text.trim().is_empty(), "{}: a text", entry.name);
    }
}

#[test]
fn every_license_is_open_and_none_is_copyleft() {
    for entry in licenses() {
        for part in entry.license.split(" AND ") {
            assert!(
                ALLOWED.contains(&part.trim()),
                "{}: {part:?} is not an allowed license",
                entry.name
            );
        }
        for family in COPYLEFT {
            assert!(
                !entry.license.contains(family),
                "{}: {family} is copyleft",
                entry.name
            );
        }
    }
}

#[test]
fn the_texts_are_those_the_licenses_ask_for() {
    let all = licenses();
    let text = |name: &str| {
        all.iter()
            .find(|entry| entry.name == name)
            .map(|entry| entry.text.clone())
            .unwrap_or_default()
    };
    // Lucide is ISC for its own icons and MIT for what it took from Feather.
    let lucide = text("Lucide");
    assert!(lucide.contains("ISC License"), "Lucide: the ISC text");
    assert!(lucide.contains("MIT License"), "Lucide: the MIT text");
    assert!(text("Phosphor").contains("MIT License"));
    // Material carries the Apache-2.0 text of Material Design Icons and the note on marks.
    let material = text("Material");
    assert!(
        material.contains("MIT License"),
        "Material: its own MIT text"
    );
    assert!(
        material.contains("Apache License"),
        "Material: the Apache-2.0 text"
    );
    assert!(
        material.contains("logos and marks"),
        "Material: the note on the marks of companies and products"
    );
}

#[test]
fn nothing_names_a_path_of_this_machine() {
    for entry in licenses() {
        for field in [
            &entry.name,
            &entry.author,
            &entry.license,
            &entry.source,
            &entry.text,
        ] {
            for local in ["/home/", "/Users/", "C:\\", "/tmp/", "file://"] {
                assert!(!field.contains(local), "{}: {local}", entry.name);
            }
        }
    }
}
