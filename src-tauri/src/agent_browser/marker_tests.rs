use serde_json::json;

use super::{MarkerTexts, Signal};

#[test]
fn marker_distinguishes_the_builtin_and_unnamed_external_agents() {
    let builtin = MarkerTexts::new("en", true, "");
    assert_eq!(builtin.text, "Built-in agent is steering this tab");

    let unnamed = MarkerTexts::new("en", false, "");
    assert_eq!(unnamed.text, "Agent is steering this tab");

    let named = MarkerTexts::new("en", false, "Extension X");
    assert_eq!(named.text, "Extension X is steering this tab");
    assert_eq!(named.paused_text, "You have this tab; Extension X waits");
}

#[test]
fn only_the_markers_binding_and_words_are_signals() {
    let call = |name: &str, payload: &str| json!({ "name": name, "payload": payload });
    assert_eq!(Signal::of(&call("__holzi", "stop")), Some(Signal::Stop));
    assert_eq!(
        Signal::of(&call("__holzi", "takeover")),
        Some(Signal::Takeover)
    );
    assert_eq!(Signal::of(&call("__holzi", "return")), Some(Signal::Return));
    assert_eq!(Signal::of(&call("__holzi", "input")), Some(Signal::Input));
    assert_eq!(Signal::of(&call("__holzi", "hello")), Some(Signal::Hello));
    assert_eq!(Signal::of(&call("__holzi", "steal")), None);
    assert_eq!(Signal::of(&call("other", "stop")), None);
}

#[test]
fn the_script_carries_the_state() {
    let texts = MarkerTexts::new("de", false, "X");
    assert!(texts.script(true).contains(r#""paused":true"#));
    assert!(texts.script(false).contains(r#""paused":false"#));
}
