use super::{Cap, CapEntry, CapabilitySet};

#[test]
fn capability_set_is_orthogonal() {
    let set = CapabilitySet::builder()
        .read(true) // delegatable
        .write(false) // not delegatable
        .build();

    assert!(set.can(Cap::Read));
    assert!(set.can(Cap::Write));
    assert!(!set.can(Cap::Invite));
    assert!(!set.can(Cap::Admin));

    assert!(set.is_delegatable(Cap::Read));
    assert!(!set.is_delegatable(Cap::Write));
}

#[test]
fn builder_orthogonal_variants_independent() {
    let set = CapabilitySet::builder().admin(true).build();

    assert!(set.can(Cap::Admin));
    assert!(!set.can(Cap::Read));
    assert!(!set.can(Cap::Write));
    assert!(!set.can(Cap::Invite));
}

#[test]
fn capability_set_serde_roundtrip() {
    let set = CapabilitySet::builder().read(true).invite(false).build();
    let json = serde_json::to_string(&set).unwrap();
    let back: CapabilitySet = serde_json::from_str(&json).unwrap();
    assert_eq!(set, back);
}

#[test]
fn serialization_is_canonical_sorted_by_cap_discriminant() {
    // Insertion order Admin, Read, Invite → output must be sorted:
    // Read (1), Invite (3), Admin (4). Write (2) omitted.
    let set = CapabilitySet::builder()
        .admin(false)
        .read(true)
        .invite(true)
        .build();

    let json = serde_json::to_string(&set).unwrap();

    // The three entries must appear in Cap-discriminant order: read, invite, admin.
    let idx_read = json.find("\"read\"").expect("read present");
    let idx_invite = json.find("\"invite\"").expect("invite present");
    let idx_admin = json.find("\"admin\"").expect("admin present");
    assert!(
        idx_read < idx_invite,
        "read must precede invite in canonical order"
    );
    assert!(
        idx_invite < idx_admin,
        "invite must precede admin in canonical order"
    );
}

#[test]
fn duplicate_caps_are_deduplicated_last_wins() {
    // Builder called twice for the same cap: last write wins on delegatable.
    let set = CapabilitySet::builder()
        .read(false)
        .read(true) // overrides previous
        .build();

    assert!(set.can(Cap::Read));
    assert!(set.is_delegatable(Cap::Read));
    assert_eq!(set.entries().count(), 1);
}

#[test]
fn empty_set_holds_nothing() {
    let set = CapabilitySet::default();
    assert!(!set.can(Cap::Read));
    assert!(!set.can(Cap::Write));
    assert!(!set.can(Cap::Invite));
    assert!(!set.can(Cap::Admin));
    assert!(!set.is_delegatable(Cap::Read));
}

#[test]
fn deserialize_rejects_duplicate_cap_entries() {
    // Canonical form has NO duplicates. A malicious/malformed payload with
    // two entries for the same Cap must fail — otherwise "last-wins" would
    // be attacker-controllable.
    let json = r#"[{"cap":"read","delegatable":false},{"cap":"read","delegatable":true}]"#;
    let result: Result<CapabilitySet, _> = serde_json::from_str(json);
    assert!(result.is_err(), "duplicate cap entries must be rejected");
}

#[test]
fn deserialize_normalizes_out_of_order_input() {
    // Wire input in non-canonical order MUST parse (be lenient in reading)
    // and produce an internally-canonical set (be strict in writing).
    let json = r#"[{"cap":"admin","delegatable":false},{"cap":"read","delegatable":true}]"#;
    let set: CapabilitySet = serde_json::from_str(json).unwrap();

    assert!(set.can(Cap::Read));
    assert!(set.can(Cap::Admin));

    let re_encoded = serde_json::to_string(&set).unwrap();
    let idx_read = re_encoded.find("\"read\"").unwrap();
    let idx_admin = re_encoded.find("\"admin\"").unwrap();
    assert!(idx_read < idx_admin);
}

#[test]
fn cap_entry_delegatable_defaults_to_false_when_absent() {
    let json = r#"[{"cap":"invite"}]"#;
    let set: CapabilitySet = serde_json::from_str(json).unwrap();
    assert!(set.can(Cap::Invite));
    assert!(!set.is_delegatable(Cap::Invite));
}

#[test]
fn cap_entry_direct_construction() {
    let entry = CapEntry {
        cap: Cap::Write,
        delegatable: true,
    };
    assert_eq!(entry.cap, Cap::Write);
    assert!(entry.delegatable);
}
