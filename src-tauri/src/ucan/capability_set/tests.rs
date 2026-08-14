use super::{cap_from_str, enforce_delegatable, Cap, CapEntry, CapabilitySet, DelegationError};

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
fn deserialize_rejects_unknown_cap_entry_fields() {
    // Fail-closed on unknown fields: a future revision that adds a
    // security-relevant CapEntry constraint (expires_at, restricted_to_row,
    // …) must NOT be silently dropped by an older peer, which would turn
    // an expired/restricted grant into an unrestricted one on the wire.
    let json = r#"[{"cap":"read","delegatable":true,"junk":42}]"#;
    let result: Result<CapabilitySet, _> = serde_json::from_str(json);
    assert!(result.is_err(), "unknown cap entry fields must be rejected");
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

// ---------------------------------------------------------------------------
// C.2 — Chain-verify delegatable enforcement (pure predicate fn)
// ---------------------------------------------------------------------------

#[test]
fn enforce_delegatable_allows_delegation_of_delegatable_cap() {
    let parent = CapabilitySet::builder().read(true).build();
    let child = CapabilitySet::builder().read(false).build();
    assert!(enforce_delegatable(&parent, &child).is_ok());
}

#[test]
fn enforce_delegatable_rejects_non_delegatable_parent() {
    let parent = CapabilitySet::builder().invite(false).build();
    let child = CapabilitySet::builder().invite(true).build();
    let err = enforce_delegatable(&parent, &child).unwrap_err();
    assert_eq!(err, DelegationError::NotDelegatable(Cap::Invite));
}

#[test]
fn enforce_delegatable_rejects_missing_cap_in_parent() {
    let parent = CapabilitySet::builder().read(true).build();
    let child = CapabilitySet::builder().write(false).build();
    let err = enforce_delegatable(&parent, &child).unwrap_err();
    assert_eq!(err, DelegationError::Missing(Cap::Write));
}

#[test]
fn enforce_delegatable_empty_child_is_ok() {
    let parent = CapabilitySet::builder().read(true).build();
    let child = CapabilitySet::default();
    assert!(enforce_delegatable(&parent, &child).is_ok());
}

#[test]
fn enforce_delegatable_child_subset_of_parent() {
    let parent = CapabilitySet::builder()
        .read(true)
        .write(true)
        .invite(true)
        .build();
    let child = CapabilitySet::builder().read(false).write(false).build();
    assert!(enforce_delegatable(&parent, &child).is_ok());
}

#[test]
fn enforce_delegatable_reports_first_violating_cap_by_discriminant_order() {
    // Child holds Read (parent OK, delegatable) + Write (parent has, not delegatable)
    // + Invite (parent doesn't have). First violation encountered walking child in
    // discriminant order is Write (NotDelegatable) — that must be reported first,
    // NOT Invite (Missing). Deterministic error surface for debuggers/tests.
    let parent = CapabilitySet::builder().read(true).write(false).build();
    let child = CapabilitySet::builder()
        .read(false)
        .write(true)
        .invite(false)
        .build();
    let err = enforce_delegatable(&parent, &child).unwrap_err();
    assert_eq!(err, DelegationError::NotDelegatable(Cap::Write));
}

#[test]
fn cap_holder_may_exercise_even_without_delegatability() {
    // The plan (`cap_can_be_exercised_even_if_not_delegatable`) — holding a
    // non-delegatable cap is still a valid hold; the flag only gates onward
    // delegation, not exercise. `can()` must NOT depend on `delegatable`.
    let set = CapabilitySet::builder().invite(false).build();
    assert!(set.can(Cap::Invite));
    assert!(!set.is_delegatable(Cap::Invite));
}

#[test]
fn cap_from_str_accepts_bare_names() {
    assert_eq!(cap_from_str("read"), Ok(Cap::Read));
    assert_eq!(cap_from_str("write"), Ok(Cap::Write));
    assert_eq!(cap_from_str("invite"), Ok(Cap::Invite));
    assert_eq!(cap_from_str("admin"), Ok(Cap::Admin));
}

#[test]
fn cap_from_str_accepts_space_prefixed_bridge() {
    // Pre-Task-8 frontend still emits the "space/" prefix; the helper
    // strips it so backend code can migrate ahead of the wire.
    assert_eq!(cap_from_str("space/read"), Ok(Cap::Read));
    assert_eq!(cap_from_str("space/write"), Ok(Cap::Write));
    assert_eq!(cap_from_str("space/invite"), Ok(Cap::Invite));
    assert_eq!(cap_from_str("space/admin"), Ok(Cap::Admin));
}

#[test]
fn cap_from_str_rejects_unknown_names() {
    assert!(cap_from_str("").is_err());
    assert!(cap_from_str("space/").is_err());
    assert!(cap_from_str("owner").is_err());
    assert!(cap_from_str("READ").is_err()); // case-sensitive
    assert!(cap_from_str("space/READ").is_err());
}
