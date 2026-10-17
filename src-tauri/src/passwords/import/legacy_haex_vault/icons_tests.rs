use super::*;

#[test]
fn every_target_name_is_a_literal_of_the_interface_list() {
    let path = concat!(env!("CARGO_MANIFEST_DIR"), "/../src/lib/passwords/icons.ts");
    let source = std::fs::read_to_string(path).expect("read icons.ts");
    for name in target_names() {
        assert!(
            source.contains(&format!("'{name}'")),
            "{name} is not a literal in src/lib/passwords/icons.ts"
        );
    }
}

#[test]
fn every_table_entry_points_at_a_target() {
    for (from, to) in MDI.iter().chain(LEGACY) {
        assert!(
            TARGETS.contains(to),
            "{from} maps to {to}, which is no target"
        );
    }
    for (from, to) in LUCIDE_RENAMED {
        let target = format!("lucide:{to}");
        assert!(
            TARGETS.contains(&target.as_str()),
            "{from} is renamed to {to}, which is no target"
        );
    }
}

#[test]
fn lucide_names_of_legacy_haex_vault_become_the_same_picture() {
    assert_eq!(map_icon("i-lucide-key"), Some("lucide:key"));
    assert_eq!(map_icon("i-lucide-trash-2"), Some("lucide:trash-2"));
    assert_eq!(map_icon("lucide:mail"), Some("lucide:mail"));
}

#[test]
fn renamed_lucide_names_follow_the_rename() {
    assert_eq!(map_icon("i-lucide-home"), Some("lucide:house"));
    assert_eq!(
        map_icon("i-lucide-alert-triangle"),
        Some("lucide:triangle-alert")
    );
}

#[test]
fn names_of_the_picker_and_the_old_extension_are_mapped() {
    assert_eq!(map_icon("mdi:bank"), Some("lucide:landmark"));
    assert_eq!(map_icon("mdi:key-variant"), Some("lucide:key-round"));
    assert_eq!(map_icon("key"), Some("lucide:key"));
    assert_eq!(map_icon("file"), Some("lucide:file-text"));
    assert_eq!(map_icon("home"), Some("lucide:house"));
}

#[test]
fn unknown_names_have_no_picture() {
    assert_eq!(map_icon("something-weird"), None);
    assert_eq!(map_icon("mdi:does-not-exist"), None);
    assert_eq!(map_icon("i-lucide-does-not-exist"), None);
    assert_eq!(map_icon("simple-icons:github"), None);
    assert_eq!(map_icon(""), None);
}
