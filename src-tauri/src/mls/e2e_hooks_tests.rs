#![cfg(all(test, feature = "e2e-hooks"))]

use super::*;

#[test]
fn commit_gate_outcome_serializes_with_a_kind_tag() {
    let json = serde_json::to_value(TestCommitGateOutcome::Accepted).unwrap();
    assert_eq!(json["kind"], "accepted");
}

#[test]
fn classify_maps_each_gate_to_its_own_variant() {
    // Every arm below is a substring lifted verbatim from a production
    // rejection string in `mls::authorization`, `mls::manager`, or
    // `mls::commit_bind`. The point of this table is to fail LOUDLY when
    // a message is reworded without updating `classify_rejection` — a
    // predicate with no case here would silently downgrade a real attack
    // to `rejectedOther`.
    let cases = [
        // Phase-1 (`authorization::authorize`)
        ("Rejecting MLS commit for space s: addee did:x is not a member of this space (haex_space_members ⋈ haex_identities)", "rejectedPhase1"),
        ("Rejecting MLS commit for space s: leaf 3 DID changed from did:a to did:b (credential-stability violation)", "rejectedPhase1"),
        ("Rejecting MLS commit for space s: unmodelled proposal type(s) [\"Foo\"] — Phase-1 authorization is fail-closed", "rejectedPhase1"),
        ("Rejecting MLS commit for space s: addee credential is empty or non-UTF8", "rejectedPhase1"),
        // Phase-2 (`authorization::verify_pops`) — full phrase spelled out
        ("Rejecting MLS commit for space s: addee did:x — KeyPackage is missing the required proof-of-possession leaf extension", "rejectedPop"),
        // Phase-2 — the two strings that only spell the acronym `PoP`
        ("Rejecting MLS commit for space s: addee did:x — malformed PoP signature bytes: bad", "rejectedPop"),
        ("Rejecting MLS commit for space s: addee did:x — PoP does not verify against the credential-DID identity key: bad", "rejectedPop"),
        // Commit-bind (`commit_bind::verify_commit_bind_bytes` and its
        // absent-proof pre-check in `MlsManager::decrypt`)
        ("Rejecting MLS commit for space s: commit-bind signature invalid for committer did:x: bad sig", "rejectedCommitBind"),
        ("Rejecting MLS commit for space s: committer did:x presented without a commit-bind signature", "rejectedCommitBind"),
        // Phase-3 (`authorization::authorize_committer_capability`)
        ("Rejecting MLS commit for space s: Remove of an active member requires a committer capability proof, none presented", "rejectedCommitterCapability"),
        ("Rejecting MLS commit for space s: presented capability audience did:a does not match the commit's committer did:b", "rejectedCommitterCapability"),
        ("Rejecting MLS commit for space s: committer did:x presented Read but membership removal requires Invite-or-higher", "rejectedCommitterCapability"),
        ("Rejecting MLS commit for space s: membership-changing commit has no resolvable committer DID", "rejectedCommitterCapability"),
        // Fallback
        ("Failed to process message: WrongEpoch", "rejectedOther"),
    ];
    for (err, expected_kind) in cases {
        let json = serde_json::to_value(classify_rejection(err)).unwrap();
        assert_eq!(json["kind"], expected_kind, "misclassified: {err}");
    }
}

#[test]
fn unchecked_removal_report_round_trips() {
    let report = TestUncheckedRemovalReport {
        commit_b64: "AAECAw==".into(),
        commit_bind_sig_b64: "BAUGBw==".into(),
        committer_did: "did:key:zAttacker".into(),
        target_did: "did:key:zVictim".into(),
    };
    let json = serde_json::to_value(&report).unwrap();
    assert_eq!(json["commitB64"], "AAECAw==");
    assert_eq!(json["committerDid"], "did:key:zAttacker");
}
