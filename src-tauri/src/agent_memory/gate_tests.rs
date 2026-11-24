//! Tests for the gate of memory writes (spec 097 contracts "When a write asks", T027).

use super::*;

#[test]
fn a_write_without_foreign_content_runs_unasked_but_not_in_plan() {
    assert_eq!(decide(PermissionMode::Manual, false), Decision::Allow);
    assert_eq!(decide(PermissionMode::Auto, false), Decision::Allow);
    assert_eq!(decide(PermissionMode::Plan, false), Decision::Deny);
}

#[test]
fn a_write_after_foreign_content_asks_but_not_in_plan() {
    assert_eq!(decide(PermissionMode::Manual, true), Decision::Ask);
    assert_eq!(decide(PermissionMode::Auto, true), Decision::Ask);
    assert_eq!(decide(PermissionMode::Plan, true), Decision::Deny);
}
