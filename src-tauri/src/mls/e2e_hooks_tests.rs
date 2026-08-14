#![cfg(all(test, feature = "e2e-hooks"))]

use super::*;

#[test]
fn commit_gate_outcome_serializes_with_a_kind_tag() {
    let json = serde_json::to_value(TestCommitGateOutcome::Accepted).unwrap();
    assert_eq!(json["kind"], "accepted");
}
