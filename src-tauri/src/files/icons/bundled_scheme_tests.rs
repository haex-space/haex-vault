//! The light scheme of the bundled packs (FR-009): Material's block `light` gives its `*_light`
//! images, Phosphor has a light fassung of every icon, and a pack without a block `light` gives
//! the same image in both schemes.

use super::builtin;
use super::bundled_tests::{dir, file, id_of};
use super::*;

/// The keys of `light.fileNames` and `light.fileExtensions` of the Material manifest.
fn material_light_keys() -> (Vec<String>, Vec<String>) {
    let pack = builtin::pack(BuiltinSet::Material).expect("the Material pack");
    let manifest: serde_json::Value =
        serde_json::from_slice(&pack.files()["icon-theme.json"]).expect("the manifest");
    let keys = |field: &str| -> Vec<String> {
        manifest["light"][field]
            .as_object()
            .map(|map| map.keys().cloned().collect())
            .unwrap_or_default()
    };
    (keys("fileNames"), keys("fileExtensions"))
}

#[test]
fn material_gives_the_light_image_in_the_light_scheme_and_the_main_image_in_the_dark_one() {
    let (names, extensions) = material_light_keys();
    assert!(names.len() >= 20, "{} light names", names.len());
    assert!(
        extensions.len() >= 25,
        "{} light extensions",
        extensions.len()
    );
    let queries = names
        .iter()
        .map(|name| file(name))
        .chain(extensions.iter().map(|ext| file(&format!("a.{ext}"))));
    for query in queries {
        let light = id_of(BuiltinSet::Material, Scheme::Light, &query);
        let dark = id_of(BuiltinSet::Material, Scheme::Dark, &query);
        assert!(
            light.contains("_light"),
            "{}: light gave {light}",
            query.name
        );
        assert_ne!(light, dark, "{}: the schemes differ", query.name);
        assert!(!dark.contains("_light"), "{}: dark gave {dark}", query.name);
    }
}

#[test]
fn phosphor_gives_different_images_for_light_and_dark() {
    for query in [file("a.pdf"), file("a.rs"), dir("src"), dir("zz")] {
        assert_ne!(
            id_of(BuiltinSet::Phosphor, Scheme::Light, &query),
            id_of(BuiltinSet::Phosphor, Scheme::Dark, &query),
            "{}",
            query.name
        );
    }
}

#[test]
fn a_pack_without_a_light_block_gives_the_same_image_in_both_schemes() {
    let theme = manifest::IconTheme::parse(
        br#"{"iconDefinitions": {"f": {"iconPath": "i/f.svg"}, "r": {"iconPath": "i/r.svg"}},
            "file": "f", "fileExtensions": {"rs": "r"}}"#,
    )
    .expect("theme");
    for name in ["a.rs", "a.txt"] {
        assert_eq!(
            resolve::resolve(Some(&theme), Scheme::Light, &file(name)),
            resolve::resolve(Some(&theme), Scheme::Dark, &file(name)),
            "{name}"
        );
    }
}
