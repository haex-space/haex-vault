use std::time::Duration;

use super::prompts::{RegistrationGate, MAX_OPEN_REGISTRATIONS, REGISTRATION_TIMEOUT};

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
