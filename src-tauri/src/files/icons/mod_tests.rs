use super::*;

#[test]
fn a_selection_value_round_trips_for_every_built_in_set() {
    for name in BuiltinSet::ALL {
        let set = IconSetRef::Builtin { name };
        assert_eq!(
            IconSetRef::parse_selection(&set.selection_value()),
            Some(set)
        );
    }
}

#[test]
fn a_custom_selection_needs_a_uuid() {
    let id = "0b9a2c52-6a53-4f4e-9a1b-3c1a1d0a77aa";
    assert_eq!(
        IconSetRef::parse_selection(&format!("custom:{id}")),
        Some(IconSetRef::Custom { id: id.to_owned() })
    );
    for bad in [
        "custom:",
        "custom:not-a-uuid",
        "custom:0b9a2c5266a534f4e9a1b3c1a1d0a77aa",
        "builtin:future",
        "builtin:",
        "lucide",
        "",
    ] {
        assert!(!is_valid_selection(bad), "{bad:?} is not a selection");
    }
}

#[test]
fn only_the_states_are_states() {
    for role in [
        IconRole::FolderNoAccess,
        IconRole::EncryptedFolder,
        IconRole::LockedFolder,
        IconRole::LockedFile,
        IconRole::Damaged,
    ] {
        assert!(role.is_state());
    }
    for role in [
        IconRole::Folder,
        IconRole::File,
        IconRole::Image,
        IconRole::Video,
        IconRole::Audio,
        IconRole::Pdf,
        IconRole::Text,
    ] {
        assert!(!role.is_state());
    }
}

#[test]
fn every_role_key_names_a_role_and_has_a_fallback_symbol() {
    for key in [
        "folder",
        "folderNoAccess",
        "file",
        "image",
        "video",
        "audio",
        "pdf",
        "text",
        "encryptedFolder",
        "lockedFolder",
        "lockedFile",
        "damaged",
    ] {
        let role = IconRole::from_key(key).expect("a role");
        assert!(role.fallback_symbol().starts_with("lucide:"));
    }
    assert_eq!(IconRole::from_key("folderExpanded"), None);
}
