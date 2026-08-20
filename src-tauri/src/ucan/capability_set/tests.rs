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
fn deserialize_rejects_cap_entry_missing_delegatable() {
    // `delegatable` is required — it must never silently default. An absent
    // flag cannot be distinguished from a producer that dropped it while
    // meaning `true`, so the only safe reading is a hard reject. The
    // end-to-end wire contract is pinned by the
    // `cap_entry_missing_delegatable_in_leaf` fixture vector.
    let json = r#"[{"cap":"read"}]"#;
    let result: Result<CapabilitySet, _> = serde_json::from_str(json);
    assert!(
        result.is_err(),
        "cap entry without `delegatable` must be rejected, got {result:?}"
    );

    // Same for every other cap — the requirement is not read-specific.
    for cap in ["write", "invite", "admin"] {
        let json = format!(r#"[{{"cap":"{cap}"}}]"#);
        let result: Result<CapabilitySet, _> = serde_json::from_str(&json);
        assert!(
            result.is_err(),
            "cap entry {cap:?} without `delegatable` must be rejected"
        );
    }

    // Non-boolean `delegatable` is rejected too (no coercion from 1/"true").
    let json = r#"[{"cap":"read","delegatable":1}]"#;
    let result: Result<CapabilitySet, _> = serde_json::from_str(json);
    assert!(
        result.is_err(),
        "non-boolean `delegatable` must be rejected"
    );
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

// ---------------------------------------------------------------------------
// `CapabilitySet::can_or_admin` — Admin acts as X gate helper
// ---------------------------------------------------------------------------

#[test]
fn admin_only_can_or_admin_invite() {
    let set = CapabilitySet::builder().admin(false).build();
    assert!(set.can_or_admin(Cap::Invite));
}

#[test]
fn write_only_can_or_admin_invite_false() {
    let set = CapabilitySet::builder().write(false).build();
    assert!(!set.can_or_admin(Cap::Invite));
}

#[test]
fn invite_only_can_or_admin_invite() {
    let set = CapabilitySet::builder().invite(false).build();
    assert!(set.can_or_admin(Cap::Invite));
}

#[test]
fn empty_set_can_or_admin_invite_false() {
    let set = CapabilitySet::default();
    assert!(!set.can_or_admin(Cap::Invite));
}

// ---------------------------------------------------------------------------
// `CapabilitySet::singleton` — single-cap grants
// ---------------------------------------------------------------------------

#[test]
fn singleton_admin_delegatable() {
    let set = CapabilitySet::singleton(Cap::Admin, true);
    assert!(set.can(Cap::Admin));
    assert!(set.is_delegatable(Cap::Admin));
    // Only one entry.
    assert_eq!(set.entries().count(), 1);
}

#[test]
fn singleton_read_not_delegatable() {
    let set = CapabilitySet::singleton(Cap::Read, false);
    assert!(set.can(Cap::Read));
    assert!(!set.is_delegatable(Cap::Read));
    assert_eq!(set.entries().count(), 1);
}

// ---------------------------------------------------------------------------
// `CapabilitySet::role_preset` / `owner_root` — the D2 role table
//
// Pinned entry-by-entry (cap, delegatable bit AND canonical order) so a
// silent edit to one bit fails here rather than at a peer's attenuation
// check. Mirror of `src/tests/spaces/capability-presets.test.ts`.
//
// Two regressions are locked down:
//
// - the inviter preset used to be `read(false) invite(true)`, which made
//   `invite` inert: `enforce_delegatable` reports the first offender in
//   Cap-discriminant order, so the holder tripped on `read` and could grant
//   nothing at all.
// - the admin preset used to be `read(false) admin(true)` — wrong on both
//   bits. The delegatable `admin` permitted admin proliferation, and the
//   missing write/invite stripped what an admin needs in order to hand
//   anything out.
// ---------------------------------------------------------------------------

fn entries_of(set: &CapabilitySet) -> Vec<CapEntry> {
    set.entries().cloned().collect()
}

fn entry(cap: Cap, delegatable: bool) -> CapEntry {
    CapEntry { cap, delegatable }
}

#[test]
fn role_preset_reader_holds_only_a_non_delegatable_read() {
    assert_eq!(
        entries_of(&CapabilitySet::role_preset(Cap::Read)),
        vec![entry(Cap::Read, false)]
    );
}

#[test]
fn role_preset_writer_holds_read_and_write_neither_delegatable() {
    assert_eq!(
        entries_of(&CapabilitySet::role_preset(Cap::Write)),
        vec![entry(Cap::Read, false), entry(Cap::Write, false)]
    );
}

#[test]
fn role_preset_inviter_may_delegate_read_and_invite() {
    // `read` MUST be delegatable here — otherwise `invite` is unreachable.
    assert_eq!(
        entries_of(&CapabilitySet::role_preset(Cap::Invite)),
        vec![entry(Cap::Read, true), entry(Cap::Invite, true)]
    );
}

#[test]
fn role_preset_admin_may_delegate_read_write_invite_but_not_admin() {
    assert_eq!(
        entries_of(&CapabilitySet::role_preset(Cap::Admin)),
        vec![
            entry(Cap::Read, true),
            entry(Cap::Write, true),
            entry(Cap::Invite, true),
            entry(Cap::Admin, false),
        ]
    );
}

#[test]
fn owner_root_holds_all_four_caps_every_one_delegatable() {
    assert_eq!(
        entries_of(&CapabilitySet::owner_root()),
        vec![
            entry(Cap::Read, true),
            entry(Cap::Write, true),
            entry(Cap::Invite, true),
            entry(Cap::Admin, true),
        ]
    );
}

#[test]
fn role_presets_satisfy_the_invite_delegatability_invariant() {
    // If a set contains `invite`, every other cap in it is delegatable —
    // except `admin`, whose non-delegatability reserves admin-minting to
    // the space root.
    for cap in [Cap::Read, Cap::Write, Cap::Invite, Cap::Admin] {
        let set = CapabilitySet::role_preset(cap);
        if !set.can(Cap::Invite) {
            continue;
        }
        for e in set.entries() {
            if e.cap == Cap::Admin {
                assert!(
                    !e.delegatable,
                    "preset {cap:?} must not carry a delegatable admin"
                );
            } else {
                assert!(
                    e.delegatable,
                    "preset {cap:?} holds invite, so {:?} must be delegatable",
                    e.cap
                );
            }
        }
    }
}

// --- Behaviour through `enforce_delegatable` -------------------------------

#[test]
fn inviter_preset_can_delegate_a_reader_preset() {
    // The regression test for the inert-invite bug.
    assert!(enforce_delegatable(
        &CapabilitySet::role_preset(Cap::Invite),
        &CapabilitySet::role_preset(Cap::Read),
    )
    .is_ok());
}

#[test]
fn inviter_preset_cannot_delegate_a_writer_preset() {
    let err = enforce_delegatable(
        &CapabilitySet::role_preset(Cap::Invite),
        &CapabilitySet::role_preset(Cap::Write),
    )
    .unwrap_err();
    assert_eq!(err, DelegationError::Missing(Cap::Write));
}

#[test]
fn admin_preset_can_delegate_reader_writer_and_inviter_presets() {
    let admin = CapabilitySet::role_preset(Cap::Admin);
    for target in [Cap::Read, Cap::Write, Cap::Invite] {
        assert!(
            enforce_delegatable(&admin, &CapabilitySet::role_preset(target)).is_ok(),
            "admin preset must be able to delegate the {target:?} preset"
        );
    }
}

#[test]
fn admin_preset_cannot_mint_another_admin() {
    let err = enforce_delegatable(
        &CapabilitySet::role_preset(Cap::Admin),
        &CapabilitySet::role_preset(Cap::Admin),
    )
    .unwrap_err();
    assert_eq!(err, DelegationError::NotDelegatable(Cap::Admin));
}

#[test]
fn owner_root_can_delegate_every_preset_admin_included() {
    let owner = CapabilitySet::owner_root();
    for target in [Cap::Read, Cap::Write, Cap::Invite, Cap::Admin] {
        assert!(
            enforce_delegatable(&owner, &CapabilitySet::role_preset(target)).is_ok(),
            "owner root must be able to delegate the {target:?} preset"
        );
    }
}

#[test]
fn reader_and_writer_presets_can_delegate_nothing() {
    // Neither carries `invite`, so neither should ever reach a grant
    // boundary — which is why their `read` bit is deliberately false.
    for holder in [Cap::Read, Cap::Write] {
        for target in [Cap::Read, Cap::Write, Cap::Invite, Cap::Admin] {
            assert!(
                enforce_delegatable(
                    &CapabilitySet::role_preset(holder),
                    &CapabilitySet::role_preset(target),
                )
                .is_err(),
                "{holder:?} preset must not be able to delegate the {target:?} preset"
            );
        }
    }
}

// --- `role_preset_union` --------------------------------------------------

#[test]
fn role_preset_union_of_a_single_cap_is_that_preset() {
    for cap in [Cap::Read, Cap::Write, Cap::Invite, Cap::Admin] {
        assert_eq!(
            CapabilitySet::role_preset_union([cap]),
            CapabilitySet::role_preset(cap),
            "union of a single {cap:?} must equal its preset"
        );
    }
}

#[test]
fn role_preset_union_read_and_write_is_the_writer_preset() {
    assert_eq!(
        entries_of(&CapabilitySet::role_preset_union([Cap::Read, Cap::Write])),
        vec![entry(Cap::Read, false), entry(Cap::Write, false)]
    );
}

#[test]
fn role_preset_union_write_and_invite_makes_write_delegatable() {
    // OR-ing the bits alone would leave `write` non-delegatable, so the
    // holder could hand out a reader but not a writer. The invariant makes
    // the whole non-admin set delegatable once `invite` is present.
    assert_eq!(
        entries_of(&CapabilitySet::role_preset_union([Cap::Write, Cap::Invite])),
        vec![
            entry(Cap::Read, true),
            entry(Cap::Write, true),
            entry(Cap::Invite, true),
        ]
    );
    assert!(enforce_delegatable(
        &CapabilitySet::role_preset_union([Cap::Write, Cap::Invite]),
        &CapabilitySet::role_preset(Cap::Write),
    )
    .is_ok());
}

#[test]
fn role_preset_union_never_yields_a_delegatable_admin() {
    let set = CapabilitySet::role_preset_union([Cap::Admin, Cap::Write, Cap::Invite]);
    assert_eq!(
        entries_of(&set),
        entries_of(&CapabilitySet::role_preset(Cap::Admin))
    );
    assert!(!set.is_delegatable(Cap::Admin));
}

#[test]
fn role_preset_union_is_order_independent_and_idempotent() {
    let a = CapabilitySet::role_preset_union([Cap::Invite, Cap::Write, Cap::Write]);
    let b = CapabilitySet::role_preset_union([Cap::Write, Cap::Invite]);
    assert_eq!(a, b);
}

#[test]
fn role_preset_union_of_nothing_is_empty() {
    // `parse_capabilities` never yields an empty list (it defaults to
    // `["space/read"]`), so this only pins the degenerate shape.
    assert_eq!(
        CapabilitySet::role_preset_union(std::iter::empty()),
        CapabilitySet::default()
    );
}
