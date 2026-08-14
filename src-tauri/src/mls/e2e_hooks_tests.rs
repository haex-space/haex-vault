#![cfg(all(test, feature = "e2e-hooks"))]

use super::*;

#[test]
fn commit_gate_outcome_serializes_with_a_kind_tag() {
    let json = serde_json::to_value(TestCommitGateOutcome::Accepted).unwrap();
    assert_eq!(json["kind"], "accepted");
}

#[test]
fn classify_maps_each_gate_to_its_own_variant() {
    let cases = [
        ("Rejecting MLS commit for space s: addee did:x is not a member of this space (haex_space_members ⋈ haex_identities)", "rejectedPhase1"),
        ("Rejecting MLS commit for space s: addee did:x — KeyPackage is missing the required proof-of-possession leaf extension", "rejectedPop"),
        ("Rejecting MLS commit for space s: commit-bind signature invalid for committer did:x: bad sig", "rejectedCommitBind"),
        ("Rejecting MLS commit for space s: Remove of an active member requires a committer capability proof, none presented", "rejectedCommitterCapability"),
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
