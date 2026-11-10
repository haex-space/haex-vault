//! Stored texts of rules: the serde names, sources as the file shares store them, and contact
//! shares recognized (FR-002).

use super::*;

#[test]
fn enums_store_their_serde_names_and_read_them_back() {
    assert_eq!(stored(&SyncRuleDirection::SourceToTarget), "sourceToTarget");
    assert_eq!(stored(&SyncRuleDirection::Both), "both");
    assert_eq!(stored(&SyncRuleDeleteMode::Ask), "ask");
    assert_eq!(stored(&SyncRuleOutcome::Partial), "partial");
    assert_eq!(stored(&SyncRuleSide::Target), "target");
    assert_eq!(
        parse::<SyncRuleDirection>("targetToSource"),
        None,
        "one way is always from the source"
    );
    assert_eq!(parse::<SyncRuleDeleteMode>("later"), None, "a newer value");
}

#[test]
fn sources_round_trip_and_unknown_texts_are_none() {
    for source in [
        SourceRef::Device,
        SourceRef::Storage {
            storage_id: "s1".to_owned(),
        },
        SourceRef::Storage {
            storage_id: "peer:dev".to_owned(),
        },
    ] {
        assert_eq!(source_of(&source_text(&source)), Some(source));
    }
    assert_eq!(source_of("storage:"), None);
    assert_eq!(source_of("cloud:x"), None);
}

#[test]
fn contact_shares_are_recognized() {
    let share = SourceRef::Storage {
        storage_id: format!("{SHARE_PREFIX}abc"),
    };
    let peer = SourceRef::Storage {
        storage_id: "peer:dev".to_owned(),
    };
    assert!(is_contact_share(&share));
    assert!(!is_contact_share(&peer));
    assert!(!is_contact_share(&SourceRef::Device));
}

#[test]
fn the_other_side() {
    assert_eq!(SyncRuleSide::Source.other(), SyncRuleSide::Target);
    assert_eq!(SyncRuleSide::Target.other(), SyncRuleSide::Source);
}

#[test]
fn the_source_kind_is_stored_by_name_and_a_draft_without_it_says_nothing() {
    assert_eq!(stored(&SyncRuleSourceKind::Folder), "folder");
    assert_eq!(stored(&SyncRuleSourceKind::File), "file");
    assert_eq!(SyncRuleSourceKind::default(), SyncRuleSourceKind::Folder);
    assert_eq!(
        parse::<SyncRuleSourceKind>("symlink"),
        None,
        "a newer value"
    );
    assert_eq!(stored(&SyncRuleOutcome::InvalidSource), "invalidSource");

    let draft: SyncRuleDraft = serde_json::from_value(serde_json::json!({
        "name": "Fotos",
        "source": { "location": { "kind": "device" }, "path": "/a" },
        "target": { "location": { "kind": "device" }, "path": "/b" },
        "direction": "sourceToTarget",
        "deleteMode": "keep",
        "intervalSecs": 900,
        "enabled": true,
        "allowDecrypted": false
    }))
    .expect("a draft without sourceKind");
    assert_eq!(draft.source_kind, None);
    assert_eq!(draft.kind(), SyncRuleSourceKind::Folder);

    let named: SyncRuleDraft = serde_json::from_value(serde_json::json!({
        "name": "Haushalt",
        "source": { "location": { "kind": "device" }, "path": "/a/f.ods" },
        "target": { "location": { "kind": "device" }, "path": "/b" },
        "sourceKind": "file",
        "direction": "sourceToTarget",
        "deleteMode": "keep",
        "intervalSecs": 900,
        "enabled": true,
        "allowDecrypted": false
    }))
    .expect("a draft with sourceKind");
    assert_eq!(named.kind(), SyncRuleSourceKind::File);
}
