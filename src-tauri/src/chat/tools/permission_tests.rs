use super::permission::{decide, decide_for_tool, Decision, PermissionMode};
use super::RiskClass;

#[test]
fn manual_mode_always_asks() {
    for risk in [RiskClass::Safe, RiskClass::Change, RiskClass::Risky] {
        assert_eq!(
            decide(PermissionMode::Manual, risk),
            Decision::Ask,
            "{risk:?}"
        );
    }
}

#[test]
fn auto_mode_allows_safe_and_change_and_asks_risky() {
    assert_eq!(
        decide(PermissionMode::Auto, RiskClass::Safe),
        Decision::Allow
    );
    assert_eq!(
        decide(PermissionMode::Auto, RiskClass::Change),
        Decision::Allow
    );
    assert_eq!(
        decide(PermissionMode::Auto, RiskClass::Risky),
        Decision::Ask
    );
}

#[test]
fn plan_mode_allows_safe_and_denies_change_and_risky_without_asking() {
    assert_eq!(
        decide(PermissionMode::Plan, RiskClass::Safe),
        Decision::Allow
    );
    assert_eq!(
        decide(PermissionMode::Plan, RiskClass::Change),
        Decision::Deny
    );
    assert_eq!(
        decide(PermissionMode::Plan, RiskClass::Risky),
        Decision::Deny
    );
}

#[test]
fn permission_mode_parses_from_preference_string() {
    assert_eq!(
        PermissionMode::parse("manual"),
        Some(PermissionMode::Manual)
    );
    assert_eq!(PermissionMode::parse("auto"), Some(PermissionMode::Auto));
    assert_eq!(PermissionMode::parse("plan"), Some(PermissionMode::Plan));
    assert_eq!(PermissionMode::parse("bogus"), None);
}

#[test]
fn default_permission_mode_is_manual() {
    assert_eq!(PermissionMode::default(), PermissionMode::Manual);
}

/// Spec 060 FR-029: a tool with a dialog of its own is asked about once, there. Manual and Auto ask
/// nothing more, Plan still refuses it, and an ordinary risky tool still asks.
#[test]
fn a_self_confirming_risky_tool_is_not_asked_twice_but_plan_still_refuses_it() {
    for mode in [PermissionMode::Manual, PermissionMode::Auto] {
        assert_eq!(
            decide_for_tool(mode, RiskClass::Risky, true),
            Decision::Allow,
            "{mode:?}"
        );
        assert_eq!(
            decide_for_tool(mode, RiskClass::Risky, false),
            decide(mode, RiskClass::Risky),
            "{mode:?}"
        );
    }
    assert_eq!(
        decide_for_tool(PermissionMode::Plan, RiskClass::Risky, true),
        Decision::Deny
    );
}
