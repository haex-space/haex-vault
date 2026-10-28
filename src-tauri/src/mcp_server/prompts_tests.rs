use std::time::Duration;

use super::prompts::{
    call_gate, RegistrationGate, CALL_TIMEOUT, MAX_OPEN_REGISTRATIONS, REGISTRATION_TIMEOUT,
    SECRET_SCOPE,
};
use super::registry::AgentMode;
use crate::chat::tools::action_tool::AgentActionDef;
use crate::chat::tools::permission::Decision;

#[test]
fn a_registration_waits_five_minutes() {
    assert_eq!(REGISTRATION_TIMEOUT, Duration::from_secs(300));
}

/// FR-011: one open dialog per agent, three in all, and a slot comes back when its dialog ends.
#[test]
fn the_gate_keeps_one_per_slug_and_three_in_all() {
    let gate = RegistrationGate::default();
    let first = gate.try_open("claude-code").expect("first");
    assert!(gate.try_open("claude-code").is_none(), "one per slug");
    let _second = gate.try_open("codex").expect("second");
    let _third = gate.try_open("other").expect("third");
    assert_eq!(gate.open_count(), MAX_OPEN_REGISTRATIONS);
    assert!(gate.try_open("fourth").is_none(), "three in all");
    drop(first);
    assert!(gate.try_open("claude-code").is_some(), "freed again");
}

#[test]
fn a_hundred_attempts_of_one_agent_keep_one_slot() {
    let gate = RegistrationGate::default();
    let _open = gate.try_open("loop").expect("first");
    let granted = (0..100).filter(|_| gate.try_open("loop").is_some()).count();
    assert_eq!(granted, 0);
    assert_eq!(gate.open_count(), 1);
}

fn action(effect: &str, scope: &str) -> AgentActionDef {
    serde_json::from_value(serde_json::json!({
        "toolName": "x_y",
        "actionId": "x.y",
        "description": "",
        "inputSchema": {},
        "effect": effect,
        "scope": scope,
    }))
    .expect("definition")
}

#[test]
fn a_call_prompt_waits_five_minutes() {
    assert_eq!(CALL_TIMEOUT, Duration::from_secs(300));
}

/// FR-021: Manuell asks for everything, Auto only for destroying, Plan runs reading and refuses
/// the rest without asking.
#[test]
fn the_approval_mode_decides_whether_a_call_asks() {
    use AgentMode::{Auto, Manual, Plan};
    use Decision::{Allow, Ask, Deny};
    let expected = [
        (Manual, "read", Ask),
        (Manual, "write", Ask),
        (Manual, "destructive", Ask),
        (Auto, "read", Allow),
        (Auto, "write", Allow),
        (Auto, "destructive", Ask),
        (Plan, "read", Allow),
        (Plan, "write", Deny),
        (Plan, "destructive", Deny),
    ];
    for (mode, effect, decision) in expected {
        assert_eq!(
            call_gate(mode, &action(effect, "settings.device"), false),
            decision,
            "{mode:?} {effect}"
        );
    }
}

#[test]
fn a_secret_asks_in_every_mode() {
    for mode in [AgentMode::Manual, AgentMode::Auto, AgentMode::Plan] {
        assert_eq!(
            call_gate(mode, &action("read", SECRET_SCOPE), false),
            Decision::Ask,
            "{mode:?}"
        );
    }
}

/// Spec 065 T006: an action that asks the user itself skips the call prompt in Manuell and Auto;
/// Plan refuses it unless it only reads.
#[test]
fn an_action_that_confirms_itself_is_not_asked_twice() {
    use AgentMode::{Auto, Manual, Plan};
    use Decision::{Allow, Deny};
    for (mode, effect, decision) in [
        (Manual, "write", Allow),
        (Auto, "destructive", Allow),
        (Plan, "read", Allow),
        (Plan, "write", Deny),
    ] {
        assert_eq!(
            call_gate(mode, &action(effect, "browser"), true),
            decision,
            "{mode:?} {effect}"
        );
    }
}
