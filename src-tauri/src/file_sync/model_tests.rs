//! Stored texts of rules: the serde names, sources as the file shares store them, and contact
//! shares recognized (FR-002).

use super::*;

#[test]
fn enums_store_their_serde_names_and_read_them_back() {
    assert_eq!(stored(&SyncRuleDirection::AToB), "aToB");
    assert_eq!(stored(&SyncRuleDirection::Both), "both");
    assert_eq!(stored(&SyncRuleDeleteMode::Ask), "ask");
    assert_eq!(stored(&SyncRuleOutcome::Partial), "partial");
    assert_eq!(stored(&SyncRuleSide::B), "b");
    assert_eq!(
        parse::<SyncRuleDirection>("bToA"),
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
    assert_eq!(SyncRuleSide::A.other(), SyncRuleSide::B);
    assert_eq!(SyncRuleSide::B.other(), SyncRuleSide::A);
}
