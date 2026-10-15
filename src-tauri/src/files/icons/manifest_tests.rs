use super::*;

const THEME: &str = r##"{
  "iconDefinitions": {
    "file": {"iconPath": "icons/file.svg"},
    "folder": {"iconPath": "icons/folder.svg"},
    "rust": {"iconPath": "./icons/rust.svg"},
    "rust-light": {"iconPath": "icons/rust_light.svg"},
    "ts": {"iconPath": "icons/ts.svg"},
    "src": {"iconPath": "icons/src.svg"},
    "font": {"fontCharacter": "\\E001", "fontColor": "#fff"},
    "readme": {"iconPath": "icons/readme.svg"},
    "readme-light": {"iconPath": "icons/readme_light.svg"},
    "image": {"iconPath": "icons/image.svg"}
  },
  "file": "file",
  "folder": "folder",
  "folderExpanded": "folder",
  "fileExtensions": {"RS": "rust", "test.ts": "ts", "ts": "ts", "glyph": "font"},
  "fileNames": {"README.md": "readme"},
  "folderNames": {"SRC": "src"},
  "folderNamesExpanded": {"src": "src"},
  "languageIds": {"rust": "rust"},
  "hidesExplorerArrows": true,
  "highContrast": {"file": "file"},
  "light": {
    "file": "readme-light",
    "fileExtensions": {"rs": "rust-light"},
    "fileNames": {"readme.md": "readme-light"}
  },
  "x-holzi": {
    "roles": {"image": "image", "file": "file", "lockedFile": "rust", "unknownRole": "file"},
    "origin": {"author": "Anna", "license": "MIT", "source": "https://example.com"}
  }
}"##;

fn theme() -> IconTheme {
    IconTheme::parse(THEME.as_bytes()).expect("a theme")
}

#[test]
fn names_extensions_and_folders_are_read_in_lower_case() {
    let theme = theme();
    assert_eq!(theme.extension(Scheme::Dark, "rs"), Some("icons/rust.svg"));
    assert_eq!(
        theme.file_name(Scheme::Dark, "readme.md"),
        Some("icons/readme.svg")
    );
    assert_eq!(
        theme.folder_name(Scheme::Dark, "src"),
        Some("icons/src.svg")
    );
    assert_eq!(
        theme.extension(Scheme::Dark, "test.ts"),
        Some("icons/ts.svg")
    );
}

#[test]
fn defaults_and_roles_are_read() {
    let theme = theme();
    assert_eq!(
        theme.default_for(Scheme::Dark, false),
        Some("icons/file.svg")
    );
    assert_eq!(
        theme.default_for(Scheme::Dark, true),
        Some("icons/folder.svg")
    );
    assert_eq!(
        theme.role(Scheme::Dark, IconRole::Image),
        Some("icons/image.svg")
    );
    assert_eq!(theme.role(Scheme::Dark, IconRole::Pdf), None);
}

#[test]
fn the_light_block_wins_in_the_light_scheme_and_falls_back_to_the_main_part() {
    let theme = theme();
    assert_eq!(
        theme.extension(Scheme::Light, "rs"),
        Some("icons/rust_light.svg")
    );
    assert_eq!(theme.extension(Scheme::Dark, "rs"), Some("icons/rust.svg"));
    assert_eq!(
        theme.extension(Scheme::Light, "test.ts"),
        Some("icons/ts.svg"),
        "not in the block"
    );
    assert_eq!(
        theme.default_for(Scheme::Light, false),
        Some("icons/readme_light.svg")
    );
    assert_eq!(
        theme.default_for(Scheme::Light, true),
        Some("icons/folder.svg")
    );
}

#[test]
fn a_role_uses_its_light_definition_in_the_light_scheme() {
    let theme = theme();
    assert_eq!(
        theme.role(Scheme::Dark, IconRole::LockedFile),
        Some("icons/rust.svg")
    );
    assert_eq!(
        theme.role(Scheme::Light, IconRole::LockedFile),
        Some("icons/rust_light.svg")
    );
    assert_eq!(
        theme.role(Scheme::Light, IconRole::Image),
        Some("icons/image.svg"),
        "no -light: the same"
    );
}

#[test]
fn what_the_file_browser_does_not_use_is_ignored_without_an_error() {
    let theme = theme();
    // A glyph definition has no image, so the entry that uses it is left out.
    assert_eq!(theme.extension(Scheme::Dark, "glyph"), None);
    assert!(!theme.image_paths().iter().any(|path| path.is_empty()));
}

#[test]
fn the_origin_and_the_image_paths_are_read() {
    let theme = theme();
    assert_eq!(theme.origin().author, "Anna");
    assert_eq!(theme.origin().license, "MIT");
    assert!(
        theme.image_paths().contains(&"icons/rust.svg".to_owned()),
        "a leading ./ is dropped"
    );
    assert!(
        theme.image_paths().windows(2).all(|pair| pair[0] < pair[1]),
        "sorted, no duplicates"
    );
}

#[test]
fn an_empty_object_is_a_theme_without_entries() {
    let theme = IconTheme::parse(b"{}").expect("an empty theme");
    assert_eq!(theme.extension(Scheme::Dark, "rs"), None);
    assert!(theme.image_paths().is_empty());
}

#[test]
fn a_path_outside_the_pack_and_broken_json_are_refused() {
    for path in ["../x.svg", "/abs.svg", "a\\b.svg", "icons/../../x.svg", ""] {
        let json = format!(r#"{{"iconDefinitions": {{"a": {{"iconPath": {path:?}}}}}}}"#);
        assert!(
            matches!(
                IconTheme::parse(json.as_bytes()),
                Err(ManifestError::BadPath(_))
            ),
            "{path:?}"
        );
    }
    assert!(matches!(
        IconTheme::parse(b"not json"),
        Err(ManifestError::Invalid(_))
    ));
    assert!(matches!(
        IconTheme::parse(br#"{"fileNames": 3}"#),
        Err(ManifestError::Invalid(_))
    ));
}

#[test]
fn a_manifest_over_the_limit_is_refused() {
    let big = vec![b' '; MAX_MANIFEST_BYTES + 1];
    assert_eq!(IconTheme::parse(&big).err(), Some(ManifestError::TooLarge));
}

#[test]
fn the_extensions_of_a_name_are_tried_longest_first() {
    assert_eq!(
        extension_candidates("a.test.ts").collect::<Vec<_>>(),
        ["test.ts", "ts"]
    );
    assert_eq!(extension_candidates("main.rs").collect::<Vec<_>>(), ["rs"]);
    assert_eq!(extension_candidates(".gitignore").count(), 0);
    assert_eq!(
        extension_candidates(".eslintrc.json").collect::<Vec<_>>(),
        ["json"]
    );
    assert_eq!(extension_candidates("trailing.").count(), 0);
    assert_eq!(extension_candidates("noext").count(), 0);
}
